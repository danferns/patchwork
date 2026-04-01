use super::smoothed::SmoothedParam;
use super::biquad::BiquadFilter;

// ── Audio Effects ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AudioEffect {
    Gain { level: SmoothedParam },                         // 0..2, 1.0 = unity
    LowPass { cutoff: SmoothedParam, state: f32 },         // Simple 1-pole
    HighPass { cutoff: SmoothedParam, state: f32 },
    Delay {
        time_ms: f32,
        feedback: SmoothedParam,
        buffer: Vec<f32>,
        write_pos: usize,
        /// Max buffer capacity in samples. Once allocated, never resized — only
        /// the read offset changes when delay time is adjusted. This avoids
        /// allocation on the audio thread and the click from buffer zeroing.
        max_delay_samples: usize,
    },
    Distortion { drive: SmoothedParam },                   // 1..20
    /// Schroeder reverb — 4 comb filters + 2 allpass filters.
    Reverb {
        room_size: SmoothedParam,
        damping: SmoothedParam,
        mix: SmoothedParam,
        comb_buffers: [Vec<f32>; 4],
        comb_pos: [usize; 4],
        comb_filter_state: [f32; 4],
        allpass_buffers: [Vec<f32>; 2],
        allpass_pos: [usize; 2],
        initialized: bool,
    },
    /// Parametric EQ — bank of biquad filters derived from curve points.
    ParametricEq { bands: Vec<BiquadFilter>, curve_hash: u64 },
}

impl AudioEffect {
    pub fn name(&self) -> &'static str {
        match self {
            AudioEffect::Gain { .. } => "Gain",
            AudioEffect::LowPass { .. } => "Low Pass",
            AudioEffect::HighPass { .. } => "High Pass",
            AudioEffect::Delay { .. } => "Delay",
            AudioEffect::Distortion { .. } => "Distortion",
            AudioEffect::Reverb { .. } => "Reverb",
            AudioEffect::ParametricEq { .. } => "Parametric EQ",
        }
    }

    /// Update user-controlled params from another effect, preserving processing state.
    /// SmoothedParams: sets the TARGET (smoothed per-sample in process()).
    pub fn merge_params(&mut self, other: &AudioEffect) {
        match (self, other) {
            (AudioEffect::Gain { level }, AudioEffect::Gain { level: new_level }) => {
                level.set(new_level.target);
            }
            (AudioEffect::LowPass { cutoff, .. }, AudioEffect::LowPass { cutoff: new_cutoff, .. }) => {
                cutoff.set(new_cutoff.target);
                // filter state preserved!
            }
            (AudioEffect::HighPass { cutoff, .. }, AudioEffect::HighPass { cutoff: new_cutoff, .. }) => {
                cutoff.set(new_cutoff.target);
            }
            (AudioEffect::Delay { time_ms, feedback, .. }, AudioEffect::Delay { time_ms: new_time, feedback: new_fb, .. }) => {
                *time_ms = *new_time; // time changes read offset, no buffer resize
                feedback.set(new_fb.target);
                // buffer, write_pos, max_delay_samples all preserved
            }
            (AudioEffect::Distortion { drive }, AudioEffect::Distortion { drive: new_drive }) => {
                drive.set(new_drive.target);
            }
            (AudioEffect::Reverb { room_size, damping, mix, .. },
             AudioEffect::Reverb { room_size: new_room, damping: new_damp, mix: new_mix, .. }) => {
                room_size.set(new_room.target);
                damping.set(new_damp.target);
                mix.set(new_mix.target);
                // Buffers and positions preserved
            }
            (AudioEffect::ParametricEq { bands, curve_hash },
             AudioEffect::ParametricEq { bands: new_bands, curve_hash: new_hash }) => {
                if *curve_hash != *new_hash {
                    // Curve shape changed — replace filter bank (preserving state where possible)
                    *bands = new_bands.clone();
                    *curve_hash = *new_hash;
                }
                // If hash unchanged, keep existing bands with their filter state (no clicks)
            }
            _ => {} // Type mismatch — shouldn't happen if effects_same_types() passed
        }
    }

    /// Get a type discriminant for comparison
    fn type_tag(&self) -> u8 {
        match self {
            AudioEffect::Gain { .. } => 0,
            AudioEffect::LowPass { .. } => 1,
            AudioEffect::HighPass { .. } => 2,
            AudioEffect::Delay { .. } => 3,
            AudioEffect::Distortion { .. } => 4,
            AudioEffect::Reverb { .. } => 5,
            AudioEffect::ParametricEq { .. } => 6,
        }
    }

    /// Process a single sample through this effect.
    /// SmoothedParams are ticked per-sample for glitch-free parameter changes.
    pub fn process(&mut self, sample: f32, sample_rate: f32) -> f32 {
        match self {
            AudioEffect::Gain { level } => sample * level.tick(),
            AudioEffect::LowPass { cutoff, state } => {
                let c = cutoff.tick().max(20.0);
                let rc = 1.0 / (std::f32::consts::TAU * c);
                let dt = 1.0 / sample_rate;
                let alpha = dt / (rc + dt);
                *state = *state + alpha * (sample - *state);
                *state
            }
            AudioEffect::HighPass { cutoff, state } => {
                let c = cutoff.tick().max(20.0);
                let rc = 1.0 / (std::f32::consts::TAU * c);
                let dt = 1.0 / sample_rate;
                let alpha = rc / (rc + dt);
                let out = alpha * (*state + sample - *state);
                *state = sample;
                out
            }
            AudioEffect::Delay { time_ms, feedback, buffer, write_pos, max_delay_samples } => {
                // Pre-allocate buffer to max size on first use (2 seconds max).
                // Never resize — only the read offset changes when delay time is tweaked.
                let max_samples = if *max_delay_samples == 0 {
                    let ms = (2000.0 * sample_rate / 1000.0) as usize; // 2 second max
                    *max_delay_samples = ms;
                    ms
                } else {
                    *max_delay_samples
                };
                if buffer.len() != max_samples {
                    buffer.resize(max_samples, 0.0);
                    *write_pos = 0;
                }

                // Read from a variable offset behind write_pos (no buffer resize needed)
                let delay_samples = ((*time_ms * sample_rate / 1000.0) as usize)
                    .clamp(1, max_samples - 1);
                let read_pos = if *write_pos >= delay_samples {
                    *write_pos - delay_samples
                } else {
                    max_samples - (delay_samples - *write_pos)
                };

                let delayed = buffer[read_pos];
                let fb = feedback.tick();
                let output = sample + delayed * fb;
                buffer[*write_pos] = output;
                *write_pos = (*write_pos + 1) % max_samples;
                output
            }
            AudioEffect::Distortion { drive } => {
                let driven = sample * drive.tick();
                driven.tanh()
            }
            AudioEffect::Reverb { room_size, damping, mix, comb_buffers, comb_pos, comb_filter_state, allpass_buffers, allpass_pos, initialized } => {
                // Initialize buffers on first use (tuned for 44.1kHz, scale for other rates)
                if !*initialized {
                    let sr_scale = (sample_rate / 44100.0).max(0.5);
                    // Comb filter delay lengths (in samples) — prime-ish values to avoid resonance
                    let comb_lengths: [usize; 4] = [
                        (1116.0 * sr_scale) as usize,
                        (1188.0 * sr_scale) as usize,
                        (1277.0 * sr_scale) as usize,
                        (1356.0 * sr_scale) as usize,
                    ];
                    // Allpass delay lengths
                    let allpass_lengths: [usize; 2] = [
                        (556.0 * sr_scale) as usize,
                        (441.0 * sr_scale) as usize,
                    ];
                    for (i, &len) in comb_lengths.iter().enumerate() {
                        comb_buffers[i] = vec![0.0; len.max(1)];
                        comb_pos[i] = 0;
                        comb_filter_state[i] = 0.0;
                    }
                    for (i, &len) in allpass_lengths.iter().enumerate() {
                        allpass_buffers[i] = vec![0.0; len.max(1)];
                        allpass_pos[i] = 0;
                    }
                    *initialized = true;
                }

                let rs = room_size.tick().clamp(0.0, 1.0);
                let feedback = rs * 0.28 + 0.7; // 0.7 .. 0.98
                let damp = damping.tick().clamp(0.0, 1.0);
                let damp1 = damp;
                let damp2 = 1.0 - damp;

                // Parallel comb filters
                let mut comb_out = 0.0f32;
                for i in 0..4 {
                    let buf = &mut comb_buffers[i];
                    let pos = &mut comb_pos[i];
                    let filt = &mut comb_filter_state[i];
                    if buf.is_empty() { continue; }

                    let delayed = buf[*pos];
                    // Low-pass comb feedback (damping)
                    *filt = delayed * damp2 + *filt * damp1;
                    buf[*pos] = sample + *filt * feedback;
                    *pos = (*pos + 1) % buf.len();
                    comb_out += delayed;
                }
                comb_out *= 0.25; // average the 4 combs

                // Series allpass filters (diffusion)
                let mut out = comb_out;
                for i in 0..2 {
                    let buf = &mut allpass_buffers[i];
                    let pos = &mut allpass_pos[i];
                    if buf.is_empty() { continue; }

                    let delayed = buf[*pos];
                    let ap_out = -out + delayed;
                    buf[*pos] = out + delayed * 0.5;
                    *pos = (*pos + 1) % buf.len();
                    out = ap_out;
                }

                // Wet/dry mix (smoothed)
                let wet = mix.tick().clamp(0.0, 1.0);
                sample * (1.0 - wet) + out * wet
            }
            AudioEffect::ParametricEq { bands, .. } => {
                let mut s = sample;
                for band in bands.iter_mut() {
                    s = band.process(s);
                }
                s
            }
        }
    }
}

/// Check if two effects chains have the same types in the same order
pub fn effects_same_types(a: &[AudioEffect], b: &[AudioEffect]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.type_tag() == y.type_tag())
}

