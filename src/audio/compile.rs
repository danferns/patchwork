#![allow(dead_code)]
//! compile_from_state() — converts SharedAudioState snapshot into a CompiledDspChain.
//!
//! Phase 4b: Uses real AudioProcessor structs (not adapters) and populates
//! ParamStore with atomic parameter slots. Returns (chain, param_map) so
//! the UI can write params via atomic stores.

use std::sync::Arc;
use std::collections::HashMap;
use crate::graph::NodeId;
use super::chain::{CompiledDspChain, DspOp, MixerOp, MixerInput};
use super::analysis::SharedAudioState;
use super::sources::AudioSource;
use super::effects::AudioEffect;
use super::processor::AudioProcessor;
use super::params::{ParamStore, ParamMap};
use super::processors::effects::*;
use super::processors::synth::SynthProcessor;

/// Compile a CompiledDspChain from the current SharedAudioState snapshot.
/// Returns the chain + a ParamMap the UI uses to write params atomically.
pub fn compile_from_state(state: &SharedAudioState, sample_rate: f32) -> (CompiledDspChain, ParamMap) {
    let max_block_size = 2048;
    let mut chain = CompiledDspChain::empty(sample_rate, max_block_size);
    let mut param_map = ParamMap::new();

    let active_ids: Vec<NodeId> = state.active_chains.keys().copied().collect();
    if active_ids.is_empty() {
        chain.param_store = Arc::new(ParamStore::new(0));
        return (chain, param_map);
    }

    // Build dependency order
    let mut deps: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for &nid in &active_ids {
        match state.sources.get(&nid) {
            Some(AudioSource::Synth(params)) => {
                let mut d = Vec::new();
                if let Some(fm_src) = params.fm_source { d.push(fm_src); }
                deps.insert(nid, d);
            }
            Some(AudioSource::Mixer { inputs }) => {
                deps.insert(nid, inputs.iter().map(|(id, _)| *id).collect());
            }
            Some(AudioSource::Sampler { input_source, .. }) => {
                let mut d = Vec::new();
                if let Some(src_id) = input_source { d.push(*src_id); }
                deps.insert(nid, d);
            }
            _ => { deps.insert(nid, vec![]); }
        }
    }

    // Topological sort
    let mut sorted: Vec<NodeId> = Vec::new();
    let mut remaining = active_ids.clone();
    for _ in 0..6 {
        if remaining.is_empty() { break; }
        let mut waiting = Vec::new();
        for nid in remaining {
            let all_ready = deps.get(&nid).map(|d| d.iter().all(|dep| sorted.contains(dep))).unwrap_or(true);
            if all_ready { sorted.push(nid); } else { waiting.push(nid); }
        }
        remaining = waiting;
    }
    for nid in remaining { sorted.push(nid); }

    // Allocate buffers
    let silence_buf = chain.alloc_buffer();
    let mut node_buffers: HashMap<NodeId, usize> = HashMap::new();
    for &nid in &sorted { node_buffers.insert(nid, chain.alloc_buffer()); }
    let temp_buf = chain.alloc_buffer();

    // Create processors and ops
    for &nid in &sorted {
        let out_buf = node_buffers[&nid];
        let is_render_only = state.render_only.contains(&nid);

        if let Some(source) = state.sources.get(&nid) {
            match source {
                AudioSource::Mixer { inputs } => {
                    let proc_idx = chain.processors.len();
                    chain.processors.push(Box::new(super::processors::mixer::MixerProcessor::new()));

                    let mut mix_inputs = Vec::new();
                    let mut ch_effects = Vec::new();

                    for (ch, (src_id, gain)) in inputs.iter().enumerate() {
                        let src_buf = node_buffers.get(src_id).copied().unwrap_or(silence_buf);
                        mix_inputs.push(MixerInput { source_buffer: src_buf, gain: *gain });

                        let ch_key = (nid, ch);
                        let mut proc_indices = Vec::new();
                        if let Some(effects) = state.channel_chains.get(&ch_key) {
                            for effect in effects {
                                let fx_idx = chain.processors.len();
                                let (proc, param_count) = effect_to_processor(effect, sample_rate);
                                let param_start = param_map.allocate(nid, param_count);
                                init_effect_params(&chain, &param_map, nid, effect, param_start);
                                chain.processors.push(proc);
                                proc_indices.push(fx_idx);
                            }
                        }
                        ch_effects.push(proc_indices);
                    }

                    chain.mixer_ops.push(MixerOp {
                        processor_idx: proc_idx,
                        output_buffer: out_buf,
                        inputs: mix_inputs,
                        channel_effects: ch_effects,
                        temp_buffer: temp_buf,
                        param_start: 0, param_count: 0,
                        mix_to_output: !is_render_only,
                        output_gain_slot: None,
                        node_id: nid,
                    });
                }

                AudioSource::Synth(params) => {
                    let proc_idx = chain.processors.len();
                    let mut synth = SynthProcessor::new(params.waveform, params.frequency, params.amplitude);
                    synth.phase = params.phase;
                    synth.active = params.active;
                    synth.fm_depth = params.fm_depth;
                    synth.amplitude.current = params.amp_smooth.current;

                    // Allocate 4 param slots: freq, amp, active, fm_depth
                    let param_start = param_map.allocate(nid, 4);
                    chain.processors.push(Box::new(synth));

                    chain.ops.push(DspOp {
                        processor_idx: proc_idx,
                        input_buffer: silence_buf,
                        output_buffer: out_buf,
                        param_start, param_count: 4,
                        mix_to_output: false,
                        output_gain_slot: None,
                        node_id: nid,
                    });

                    // Apply effects chain for this source
                    add_effect_chain(&mut chain, &mut param_map, nid, state, sample_rate, out_buf, is_render_only);
                }

                AudioSource::LiveInput { buffer, gain } => {
                    let proc_idx = chain.processors.len();
                    chain.processors.push(Box::new(super::processors::input::LiveInputProcessor {
                        buffer: buffer.clone(), gain: *gain,
                    }));

                    chain.ops.push(DspOp {
                        processor_idx: proc_idx,
                        input_buffer: silence_buf,
                        output_buffer: out_buf,
                        param_start: 0, param_count: 0,
                        mix_to_output: false,
                        output_gain_slot: None,
                        node_id: nid,
                    });

                    add_effect_chain(&mut chain, &mut param_map, nid, state, sample_rate, out_buf, is_render_only);
                }

                AudioSource::FilePlayer { buffer, volume } => {
                    let proc_idx = chain.processors.len();
                    chain.processors.push(Box::new(super::processors::input::FilePlayerProcessor {
                        buffer: buffer.clone(), volume: *volume,
                    }));

                    chain.ops.push(DspOp {
                        processor_idx: proc_idx,
                        input_buffer: silence_buf,
                        output_buffer: out_buf,
                        param_start: 0, param_count: 0,
                        mix_to_output: false,
                        output_gain_slot: None,
                        node_id: nid,
                    });

                    add_effect_chain(&mut chain, &mut param_map, nid, state, sample_rate, out_buf, is_render_only);
                }

                AudioSource::Sampler { buffer, volume, input_source } => {
                    let proc_idx = chain.processors.len();
                    chain.processors.push(Box::new(
                        super::processors::sampler::SamplerProcessor::new(buffer.clone(), *volume)
                    ));

                    // Wire the upstream source's buffer as input (for recording)
                    let in_buf = input_source
                        .and_then(|src_id| node_buffers.get(&src_id).copied())
                        .unwrap_or(silence_buf);

                    let param_start = param_map.allocate(nid, 3);
                    chain.ops.push(DspOp {
                        processor_idx: proc_idx,
                        input_buffer: in_buf,
                        output_buffer: out_buf,
                        param_start, param_count: 3,
                        mix_to_output: false,
                        output_gain_slot: None,
                        node_id: nid,
                    });

                    add_effect_chain(&mut chain, &mut param_map, nid, state, sample_rate, out_buf, is_render_only);
                }
            }
        }
    }

    // Prepare all processors
    for proc in &mut chain.processors {
        proc.prepare(sample_rate, max_block_size);
    }

    // Create ParamStore with correct size and initialize values
    let total_slots = param_map.total_slots();
    let store = ParamStore::new(total_slots);

    // Initialize synth params in store
    for &nid in &sorted {
        if let Some(AudioSource::Synth(params)) = state.sources.get(&nid) {
            if let Some(slot) = param_map.get_slot(nid, 0) {
                store.set(slot, params.frequency);
                store.set(slot + 1, params.amplitude);
                store.set(slot + 2, if params.active { 1.0 } else { 0.0 });
                store.set(slot + 3, params.fm_depth);
            }
        }
    }

    chain.param_store = Arc::new(store);
    chain.param_scratch = vec![0.0f32; 32];

    (chain, param_map)
}

/// Add the effects chain for a source node to the compiled chain.
fn add_effect_chain(
    chain: &mut CompiledDspChain,
    param_map: &mut ParamMap,
    nid: NodeId,
    state: &SharedAudioState,
    sample_rate: f32,
    mut current_buf: usize,
    is_render_only: bool,
) {
    if let Some(effects) = state.active_chains.get(&nid) {
        if effects.is_empty() {
            // No effects — mark source for direct output
            if !is_render_only {
                if let Some(last_op) = chain.ops.iter_mut().rev().find(|op| op.node_id == nid) {
                    last_op.mix_to_output = true;
                }
            }
            return;
        }

        for (i, effect) in effects.iter().enumerate() {
            let fx_proc_idx = chain.processors.len();
            let (proc, param_count) = effect_to_processor(effect, sample_rate);
            let _param_start = if param_count > 0 { param_map.allocate(nid, param_count) } else { 0 };
            chain.processors.push(proc);

            let fx_out = if i == effects.len() - 1 { current_buf } else { chain.alloc_buffer() };
            let is_last = i == effects.len() - 1;

            chain.ops.push(DspOp {
                processor_idx: fx_proc_idx,
                input_buffer: current_buf,
                output_buffer: fx_out,
                param_start: _param_start,
                param_count,
                mix_to_output: is_last && !is_render_only,
                output_gain_slot: None,
                node_id: nid,
            });
            current_buf = fx_out;
        }
    } else if !is_render_only {
        // No effects chain entry — mark source for direct output
        if let Some(last_op) = chain.ops.iter_mut().rev().find(|op| op.node_id == nid) {
            last_op.mix_to_output = true;
        }
    }
}

/// Convert an AudioEffect enum variant into a real AudioProcessor.
fn effect_to_processor(effect: &AudioEffect, _sample_rate: f32) -> (Box<dyn AudioProcessor>, usize) {
    match effect {
        AudioEffect::Gain { level } => {
            (Box::new(GainProcessor::new(level.target)), 1)
        }
        AudioEffect::LowPass { cutoff, state } => {
            let mut p = LowPassProcessor::new(cutoff.target);
            p.state = *state;
            (Box::new(p), 1)
        }
        AudioEffect::HighPass { cutoff, state } => {
            let mut p = HighPassProcessor::new(cutoff.target);
            p.state = *state;
            (Box::new(p), 1)
        }
        AudioEffect::Delay { time_ms, feedback, .. } => {
            (Box::new(DelayProcessor::new(*time_ms, feedback.target)), 2)
        }
        AudioEffect::Distortion { drive } => {
            (Box::new(DistortionProcessor::new(drive.target)), 1)
        }
        AudioEffect::Reverb { room_size, damping, mix, .. } => {
            (Box::new(ReverbProcessor::new(room_size.target, damping.target, mix.target)), 3)
        }
        AudioEffect::ParametricEq { bands, curve_hash } => {
            (Box::new(EqProcessor::new(bands.clone(), *curve_hash)), 0)
        }
    }
}

/// Initialize effect parameter values in the ParamStore.
fn init_effect_params(_chain: &CompiledDspChain, _param_map: &ParamMap, _nid: NodeId, _effect: &AudioEffect, _param_start: usize) {
    // TODO: write initial values to param store once it's created
    // For now, processors are initialized with correct values in their constructors
}
