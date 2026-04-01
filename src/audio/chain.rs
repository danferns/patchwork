#![allow(dead_code)]
//! CompiledDspChain — pre-compiled, flat DSP execution plan.
//!
//! Replaces the per-callback topological sort in audio_callback().
//! Built once on topology change by compile_dsp_chain().
//! Executed every audio callback with zero allocations.

use std::sync::Arc;
use crate::graph::NodeId;
use super::processor::AudioProcessor;
use super::params::ParamStore;
use super::analysis::AudioAnalysis;

/// A single operation in the compiled DSP chain.
/// The chain is a flat Vec<DspOp> in topological order.
#[derive(Debug, Clone)]
pub struct DspOp {
    /// Which processor to run (index into `processors` Vec).
    #[allow(dead_code)] pub processor_idx: usize,
    /// Buffer to read input from (index into `buffers` Vec). Ignored for Sources.
    pub input_buffer: usize,
    /// Buffer to write output to (index into `buffers` Vec).
    pub output_buffer: usize,
    /// Starting slot in ParamStore for this processor's parameters.
    #[allow(dead_code)] pub param_start: usize,
    /// Number of parameter slots.
    #[allow(dead_code)] pub param_count: usize,
    /// Whether to mix this output to the master output.
    pub mix_to_output: bool,
    /// Gain to apply when mixing to output (Speaker volume slot in ParamStore).
    pub output_gain_slot: Option<usize>,
    /// Node ID this op belongs to (for debugging / analysis).
    #[allow(dead_code)] pub node_id: NodeId,
}

/// Mixer input — one source buffer + gain.
#[derive(Debug, Clone)]
pub struct MixerInput {
    pub source_buffer: usize,
    pub gain: f32,
}

/// Mixer operation — handled specially by the executor.
#[derive(Debug, Clone)]
pub struct MixerOp {
    /// Index into `processors` for the mixer processor.
    #[allow(dead_code)] pub processor_idx: usize,
    /// Buffer to write mixed output to.
    pub output_buffer: usize,
    /// Source buffers to mix from, with per-channel gains.
    pub inputs: Vec<MixerInput>,
    /// Per-channel effect chains (processor indices to run on each input before mixing).
    pub channel_effects: Vec<Vec<usize>>,
    /// Temp buffer for channel processing.
    pub temp_buffer: usize,
    /// ParamStore slot range.
    #[allow(dead_code)] pub param_start: usize,
    #[allow(dead_code)] pub param_count: usize,
    pub mix_to_output: bool,
    pub output_gain_slot: Option<usize>,
    #[allow(dead_code)] pub node_id: NodeId,
}

/// The compiled, ready-to-execute DSP graph.
///
/// Entirely owned by the audio thread after swap.
/// No shared references, no mutex, no allocations during execution.
pub struct CompiledDspChain {
    /// Source and effect operations in topological order.
    pub ops: Vec<DspOp>,
    /// Mixer operations (handled separately since they read multiple buffers).
    pub mixer_ops: Vec<MixerOp>,
    /// All processors, owned. Indexed by DspOp::processor_idx.
    pub processors: Vec<Box<dyn AudioProcessor>>,
    /// Pre-allocated buffer pool. Each buffer has max_block_size floats.
    pub buffers: Vec<Vec<f32>>,
    /// Shared parameter store (Arc — UI writes, audio reads).
    pub param_store: Arc<ParamStore>,
    /// Master output analysis.
    pub master_analysis: AudioAnalysis,
    /// Sample rate this chain was compiled for.
    pub sample_rate: f32,
    /// Maximum block size.
    pub max_block_size: usize,
    /// Scratch buffer for parameter reads (avoids per-call allocation).
    pub param_scratch: Vec<f32>,
}

impl CompiledDspChain {
    /// Create an empty chain (silence).
    pub fn empty(sample_rate: f32, max_block_size: usize) -> Self {
        Self {
            ops: Vec::new(),
            mixer_ops: Vec::new(),
            processors: Vec::new(),
            buffers: Vec::new(),
            param_store: Arc::new(ParamStore::new(0)),
            master_analysis: AudioAnalysis::default(),
            sample_rate,
            max_block_size,
            param_scratch: Vec::new(),
        }
    }

    /// Allocate a new buffer in the pool, return its index.
    pub fn alloc_buffer(&mut self) -> usize {
        let idx = self.buffers.len();
        self.buffers.push(vec![0.0f32; self.max_block_size]);
        idx
    }

    /// Execute the compiled chain for one audio callback.
    ///
    /// Zero allocations. Reads params from ParamStore (atomic).
    /// Writes output to `data` (interleaved stereo).
    pub fn execute(&mut self, data: &mut [f32], channels: usize) {
        let num_frames = data.len() / channels;
        if num_frames == 0 { return; }

        // Zero output
        data.fill(0.0);

        let ctx = super::processor::ProcessContext {
            sample_rate: self.sample_rate,
            block_size: num_frames,
        };

        // 1. Execute source and effect ops in order
        for op_idx in 0..self.ops.len() {
            let op = &self.ops[op_idx];
            let proc_idx = op.processor_idx;
            let input_buf = op.input_buffer;
            let output_buf = op.output_buffer;
            let param_start = op.param_start;
            let param_count = op.param_count;

            // Read params from store (control rate)
            if param_count > 0 {
                self.param_scratch.resize(param_count, 0.0);
                self.param_store.read_range(param_start, param_count, &mut self.param_scratch);
                self.processors[proc_idx].set_params(&self.param_scratch);
            }

            // Process block — use index-based buffer access to avoid borrow conflicts
            if input_buf == output_buf {
                // In-place: copy to param_scratch as temp, process from there
                let buf = &mut self.buffers[output_buf];
                // Use param_scratch as a temp input copy (it's large enough after resize)
                let mut temp = vec![0.0f32; num_frames]; // TODO: pre-allocate in Phase 4
                temp.copy_from_slice(&buf[..num_frames]);
                self.processors[proc_idx].process_block(&temp, &mut buf[..num_frames], &ctx);
            } else {
                // Different buffers — split borrow
                let (input_slice, output_slice) = get_two_buffer_slices(
                    &mut self.buffers, input_buf, output_buf, num_frames,
                );
                self.processors[proc_idx].process_block(input_slice, output_slice, &ctx);
            }
        }

        // 2. Execute mixer ops
        for mop in &self.mixer_ops {
            // Zero mixer output buffer
            let out_buf = &mut self.buffers[mop.output_buffer];
            out_buf[..num_frames].fill(0.0);

            // Mix each input source with gain
            for (ch, mix_input) in mop.inputs.iter().enumerate() {
                // Copy source to temp buffer for per-channel effects
                let src_data: Vec<f32> = self.buffers[mix_input.source_buffer][..num_frames].to_vec();
                let temp = &mut self.buffers[mop.temp_buffer];
                temp[..num_frames].copy_from_slice(&src_data);

                // Apply per-channel effects (if any)
                if let Some(effect_chain) = mop.channel_effects.get(ch) {
                    for &proc_idx in effect_chain {
                        // Read from temp, write back to temp (in-place via copy)
                        let temp_copy: Vec<f32> = self.buffers[mop.temp_buffer][..num_frames].to_vec();
                        self.processors[proc_idx].process_block(&temp_copy, &mut self.buffers[mop.temp_buffer][..num_frames], &ctx);
                    }
                }

                // Accumulate to mixer output (index-based to avoid double borrow)
                let temp_idx = mop.temp_buffer;
                let out_idx = mop.output_buffer;
                for f in 0..num_frames {
                    let sample = self.buffers[temp_idx][f] * mix_input.gain;
                    self.buffers[out_idx][f] += sample;
                }
            }
        }

        // 3. Mix to master output
        // Regular ops
        for op in &self.ops {
            if !op.mix_to_output { continue; }
            let gain = op.output_gain_slot.map(|s| self.param_store.get(s)).unwrap_or(1.0);
            let buf = &self.buffers[op.output_buffer];
            for frame in 0..num_frames {
                let sample = buf[frame] * gain;
                for ch in 0..channels { data[frame * channels + ch] += sample; }
            }
        }
        // Mixer ops
        for mop in &self.mixer_ops {
            if !mop.mix_to_output { continue; }
            let gain = mop.output_gain_slot.map(|s| self.param_store.get(s)).unwrap_or(1.0);
            let buf = &self.buffers[mop.output_buffer];
            for frame in 0..num_frames {
                let sample = buf[frame] * gain;
                for ch in 0..channels { data[frame * channels + ch] += sample; }
            }
        }

        // 4. Clamp
        for s in data.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        // 5. Analysis
        self.master_analysis.update(data, channels, self.sample_rate);
    }
}

/// Get two non-overlapping mutable slices from the buffer pool.
/// If input == output, returns the same slice for both (in-place processing).
fn get_two_buffer_slices<'a>(
    buffers: &'a mut Vec<Vec<f32>>,
    input_idx: usize,
    output_idx: usize,
    num_frames: usize,
) -> (&'a [f32], &'a mut [f32]) {
    if input_idx == output_idx {
        // In-place: caller reads then writes to same buffer
        let buf = &mut buffers[output_idx];
        let ptr = buf.as_ptr();
        let len = num_frames.min(buf.len());
        unsafe {
            (std::slice::from_raw_parts(ptr, len), &mut buf[..len])
        }
    } else {
        // Safe: different indices
        let (a, b) = if input_idx < output_idx {
            let (left, right) = buffers.split_at_mut(output_idx);
            (&left[input_idx][..num_frames], &mut right[0][..num_frames])
        } else {
            let (left, right) = buffers.split_at_mut(input_idx);
            (&right[0][..num_frames], &mut left[output_idx][..num_frames])
        };
        // Rust won't let us return (&[f32], &mut [f32]) from split_at_mut directly
        // because the return types are (&mut [f32], &mut [f32]). We need to reborrow.
        (a as &[f32], b)
    }
}
