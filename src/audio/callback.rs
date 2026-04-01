use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use crate::graph::NodeId;
use super::sources::AudioSource;
use super::analysis::{SharedAudioState, AudioAnalysis};
use super::waveform::SynthParams;

// ── Audio Callback (runs on audio thread) ────────────────────────────────────

/// Generate samples for a single synth into a buffer, optionally applying FM modulation.
/// Returns the buffer (for use as FM source by downstream carriers).
pub(crate) fn generate_synth_buffer(
    params: &mut SynthParams,
    num_frames: usize,
    sample_rate: f32,
    fm_buf: Option<&[f32]>,
) -> Vec<f32> {
    let mut buf = vec![0.0f32; num_frames];
    // Sync the smoother target with the UI-set amplitude
    params.amp_smooth.set(if params.active { params.amplitude } else { 0.0 });

    // Don't early-return if amplitude is low — the smoother needs to ramp down
    // to avoid clicks when gate closes
    if !params.active && params.amp_smooth.current < 0.0001 {
        return buf;
    }

    for frame in 0..num_frames {
        // FM: offset frequency by modulator's sample × depth
        let freq = params.frequency + match fm_buf {
            Some(mod_samples) => mod_samples[frame] * params.fm_depth,
            None => 0.0,
        };

        let amp = params.amp_smooth.tick();
        buf[frame] = params.waveform.sample(params.phase) * amp;

        // Advance phase (freq can go negative from FM — handle wrap in both directions)
        params.phase += freq / sample_rate;
        while params.phase >= 1.0 { params.phase -= 1.0; }
        while params.phase < 0.0 { params.phase += 1.0; }
    }
    buf
}

pub(crate) fn audio_callback(
    data: &mut [f32],
    channels: usize,
    state: &Arc<Mutex<SharedAudioState>>,
    dropout_counter: &AtomicU32,
) {
    // Zero the buffer
    for sample in data.iter_mut() {
        *sample = 0.0;
    }

    // Use try_lock to avoid blocking the audio thread — if UI holds the lock, skip this buffer
    let mut s = match state.try_lock() {
        Ok(s) => s,
        Err(_) => {
            // Atomic increment — works even when the mutex is held (that's the whole point)
            dropout_counter.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let callback_start = std::time::Instant::now();
    let sample_rate = s.sample_rate;
    let master_vol = s.master_volume;
    let num_frames = data.len() / channels;

    // Only play sources that are in active_chains (routed through a Speaker node)
    let active_ids: Vec<NodeId> = s.active_chains.keys().copied().collect();

    // Move channel_chains out of shared state so we can use it freely inside the
    // Mixer rendering arm without conflicting with the s.sources borrow.
    // We put it back before this function returns.
    let mut channel_chains = std::mem::take(&mut s.channel_chains);

    // ── Dependency-ordered rendering ──────────────────────────────────────────
    // Build per-source dependency lists: Synths depend on FM modulators, Mixers
    // depend on their input sources.
    let mut deps: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for &nid in &active_ids {
        match s.sources.get(&nid) {
            Some(AudioSource::Synth(params)) => {
                let mut d = Vec::new();
                if let Some(fm_src) = params.fm_source { d.push(fm_src); }
                deps.insert(nid, d);
            }
            Some(AudioSource::Mixer { inputs }) => {
                deps.insert(nid, inputs.iter().map(|(id, _)| *id).collect());
            }
            Some(AudioSource::LiveInput { .. }) => {
                deps.insert(nid, vec![]); // No dependencies — reads from ring buffer
            }
            Some(AudioSource::FilePlayer { .. }) => {
                deps.insert(nid, vec![]); // No dependencies — reads from ring buffer
            }
            Some(AudioSource::Sampler { input_source, .. }) => {
                let mut d = Vec::new();
                if let Some(src_id) = input_source { d.push(*src_id); }
                deps.insert(nid, d);
            }
            _ => {}
        }
    }

    let mut rendered: HashMap<NodeId, Vec<f32>> = HashMap::new();
    let mut remaining: Vec<NodeId> = active_ids.clone();
    let max_passes = 6;

    for _ in 0..max_passes {
        if remaining.is_empty() { break; }
        let mut still_waiting: Vec<NodeId> = Vec::new();

        for nid in remaining {
            let node_deps = deps.get(&nid).cloned().unwrap_or_default();
            let all_deps_ready = node_deps.iter().all(|d| rendered.contains_key(d));

            if !all_deps_ready {
                still_waiting.push(nid);
                continue;
            }

            match s.sources.get_mut(&nid) {
                Some(AudioSource::Synth(params)) => {
                    let fm_buf = params.fm_source.and_then(|src_id| rendered.get(&src_id));
                    let buf = generate_synth_buffer(
                        params, num_frames, sample_rate,
                        fm_buf.map(|v| v.as_slice()),
                    );
                    rendered.insert(nid, buf);
                }
                Some(AudioSource::Mixer { inputs }) => {
                    // Mix input sources with per-channel effects applied before accumulation.
                    // Each channel (nid, ch) has its own effect chain in channel_chains,
                    // so different channels can have different filter/delay state independently —
                    // even if two channels reference the same source node.
                    let mut buf = vec![0.0f32; num_frames];
                    let inputs_snapshot: Vec<(NodeId, f32)> = inputs.clone();
                    // s.sources borrow ends here; channel_chains is a local, so no conflict.
                    for (ch, (src_id, gain)) in inputs_snapshot.iter().enumerate() {
                        if let Some(src_buf) = rendered.get(src_id) {
                            if let Some(fx_chain) = channel_chains.get_mut(&(nid, ch)) {
                                // Apply this channel's effect chain sample-by-sample
                                for frame in 0..num_frames {
                                    let mut sample = src_buf[frame];
                                    for fx in fx_chain.iter_mut() {
                                        sample = fx.process(sample, sample_rate);
                                    }
                                    buf[frame] += sample * gain;
                                }
                            } else {
                                // No effects on this channel — straight mix with gain
                                for frame in 0..num_frames {
                                    buf[frame] += src_buf[frame] * gain;
                                }
                            }
                        }
                    }
                    rendered.insert(nid, buf);
                }
                Some(AudioSource::LiveInput { buffer, gain }) => {
                    let g = *gain;
                    let mut buf = vec![0.0f32; num_frames];
                    buffer.read_into(&mut buf, num_frames);
                    if (g - 1.0).abs() > 0.001 {
                        for s in buf.iter_mut() { *s *= g; }
                    }
                    rendered.insert(nid, buf);
                }
                Some(AudioSource::FilePlayer { buffer, volume }) => {
                    let vol = *volume;
                    let mut buf = vec![0.0f32; num_frames];
                    buffer.read_into(&mut buf, num_frames);
                    if (vol - 1.0).abs() > 0.001 {
                        for s in buf.iter_mut() { *s *= vol; }
                    }
                    buffer.playback_position.fetch_add(num_frames, Ordering::Relaxed);
                    rendered.insert(nid, buf);
                }
                Some(AudioSource::Sampler { buffer, volume, input_source }) => {
                    let vol = *volume;
                    let is_recording = buffer.recording.load(Ordering::Relaxed);
                    let is_playing = buffer.playing.load(Ordering::Relaxed);

                    // If recording, feed input source audio into the sampler buffer
                    if is_recording {
                        if let Some(src_id) = input_source {
                            if let Some(src_buf) = rendered.get(src_id) {
                                buffer.record_from(src_buf);
                            }
                        }
                        rendered.insert(nid, vec![0.0f32; num_frames]);
                    } else if is_playing {
                        let mut buf = vec![0.0f32; num_frames];
                        buffer.play_into(&mut buf, num_frames);
                        if (vol - 1.0).abs() > 0.001 {
                            for s in buf.iter_mut() { *s *= vol; }
                        }
                        rendered.insert(nid, buf);
                    } else {
                        rendered.insert(nid, vec![0.0f32; num_frames]);
                    }
                }
                _ => {}
            }
        }

        remaining = still_waiting;
    }

    // Any remaining (circular deps) — render without deps to avoid silence
    for nid in remaining {
        match s.sources.get_mut(&nid) {
            Some(AudioSource::Synth(params)) => {
                let buf = generate_synth_buffer(params, num_frames, sample_rate, None);
                rendered.insert(nid, buf);
            }
            Some(AudioSource::Mixer { .. }) => {
                rendered.insert(nid, vec![0.0; num_frames]);
            }
            Some(AudioSource::LiveInput { buffer, gain }) => {
                let g = *gain;
                let mut buf = vec![0.0f32; num_frames];
                buffer.read_into(&mut buf, num_frames);
                if (g - 1.0).abs() > 0.001 {
                    for s in buf.iter_mut() { *s *= g; }
                }
                rendered.insert(nid, buf);
            }
            Some(AudioSource::FilePlayer { buffer, volume }) => {
                let vol = *volume;
                let mut buf = vec![0.0f32; num_frames];
                buffer.read_into(&mut buf, num_frames);
                if (vol - 1.0).abs() > 0.001 {
                    for s in buf.iter_mut() { *s *= vol; }
                }
                buffer.playback_position.fetch_add(num_frames, Ordering::Relaxed);
                rendered.insert(nid, buf);
            }
            Some(AudioSource::Sampler { buffer, volume, .. }) => {
                // Fallback: can only play, can't record (no guaranteed input)
                let vol = *volume;
                if buffer.playing.load(Ordering::Relaxed) {
                    let mut buf = vec![0.0f32; num_frames];
                    buffer.play_into(&mut buf, num_frames);
                    if (vol - 1.0).abs() > 0.001 {
                        for s in buf.iter_mut() { *s *= vol; }
                    }
                    rendered.insert(nid, buf);
                } else {
                    rendered.insert(nid, vec![0.0f32; num_frames]);
                }
            }
            _ => {}
        }
    }

    // ── Per-source audio analysis (for AudioAnalyzer nodes) ────────────
    for &src_id in &s.analyze_sources.clone() {
        if let Some(buf) = rendered.get(&src_id) {
            // Build a mono "data" slice for the analysis update
            let entry = s.source_analysis.entry(src_id).or_insert_with(AudioAnalysis::default);
            entry.update(buf, 1, sample_rate);
        }
    }

    // ── Mix rendered buffers to output (skip render-only nodes) ────────
    for &nid in &active_ids {
        // Skip render-only sources — they only feed into Mixers/FM, not output directly
        if s.render_only.contains(&nid) { continue; }

        // Apply effects to the MONO rendered buffer first, then copy to stereo output.
        // Previously effects were applied to the interleaved stereo `data` buffer, which
        // called process() once per output channel (2× for stereo). This caused stateful
        // effects (Delay, LowPass, HighPass) to advance their internal state twice per
        // frame — halving delay times and distorting filter cutoffs.
        if let Some(fx_chain) = s.active_chains.get_mut(&nid) {
            if !fx_chain.is_empty() {
                if let Some(buf) = rendered.get_mut(&nid) {
                    for frame in 0..num_frames {
                        let mut sample = buf[frame];
                        for fx in fx_chain.iter_mut() {
                            sample = fx.process(sample, sample_rate);
                        }
                        buf[frame] = sample;
                    }
                }
            }
        }

        // Write the (now-processed) mono signal to all output channels.
        if let Some(buf) = rendered.get(&nid) {
            for frame in 0..num_frames {
                let sample = buf[frame];
                for ch in 0..channels {
                    data[frame * channels + ch] += sample * master_vol;
                }
            }
        }
    }

    // Clamp output to prevent clipping
    for sample in data.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }

    // Compute real-time audio analysis from the final output mix
    s.analysis.update(data, channels, sample_rate);

    // Restore channel_chains (with updated internal state: filter coefficients,
    // delay buffer write positions) back into shared state for next callback.
    s.channel_chains = channel_chains;

    // Record callback performance metrics
    let elapsed = callback_start.elapsed();
    s.callback_duration_us = elapsed.as_micros() as f32;
    s.callback_budget_us = num_frames as f32 / sample_rate * 1_000_000.0;
}
