#![allow(dead_code)]
//! SynthProcessor — oscillator with FM modulation support.

use crate::audio::processor::{AudioProcessor, ProcessorKind, ProcessContext};
use crate::audio::smoothed::SmoothedParam;
use crate::audio::waveform::Waveform;

pub struct SynthProcessor {
    pub waveform: Waveform,
    pub frequency: f32,
    pub amplitude: SmoothedParam,
    pub phase: f32,
    pub active: bool,
    /// Buffer index for FM modulator (set by chain compiler, not a param)
    pub fm_buffer_idx: Option<usize>,
    pub fm_depth: f32,
}

impl SynthProcessor {
    pub fn new(waveform: Waveform, frequency: f32, amplitude: f32) -> Self {
        Self {
            waveform, frequency,
            amplitude: SmoothedParam::new(amplitude, 5.0),
            phase: 0.0, active: true,
            fm_buffer_idx: None, fm_depth: 0.0,
        }
    }
}

impl AudioProcessor for SynthProcessor {
    fn type_id(&self) -> &str { "synth" }
    fn kind(&self) -> ProcessorKind { ProcessorKind::Source }

    fn process_block(&mut self, input: &[f32], output: &mut [f32], ctx: &ProcessContext) {
        let target_amp = if self.active { self.amplitude.target } else { 0.0 };
        self.amplitude.set(target_amp);

        if !self.active && self.amplitude.current < 0.0001 {
            output.fill(0.0);
            return;
        }

        // Check if input has FM signal (non-zero)
        let has_fm = self.fm_depth > 0.0 && input.iter().take(ctx.block_size).any(|&s| s.abs() > 0.0001);

        for i in 0..ctx.block_size {
            let fm_mod = if has_fm { input[i] * self.fm_depth } else { 0.0 };
            let freq = self.frequency + fm_mod;
            let amp = self.amplitude.tick();
            output[i] = self.waveform.sample(self.phase) * amp;
            self.phase += freq / ctx.sample_rate;
            if self.phase >= 1.0 { self.phase -= 1.0; }
            if self.phase < 0.0 { self.phase += 1.0; }
        }
    }

    fn set_params(&mut self, params: &[f32]) {
        // params[0] = frequency, params[1] = amplitude, params[2] = active (0/1),
        // params[3] = fm_depth, params[4] = waveform (0=Sine, 1=Square, 2=Saw, 3=Triangle, 4=Noise)
        if let Some(&v) = params.first() { self.frequency = v; }
        if let Some(&v) = params.get(1) { self.amplitude.set(v); }
        if let Some(&v) = params.get(2) { self.active = v > 0.5; }
        if let Some(&v) = params.get(3) { self.fm_depth = v; }
        if let Some(&v) = params.get(4) {
            self.waveform = match v as u32 {
                0 => Waveform::Sine,
                1 => Waveform::Square,
                2 => Waveform::Saw,
                3 => Waveform::Triangle,
                _ => Waveform::Noise,
            };
        }
    }

    fn param_count(&self) -> usize { 5 }

    fn prepare(&mut self, _sample_rate: f32, _max_block_size: usize) {
        self.amplitude = SmoothedParam::new(self.amplitude.target, 5.0);
    }

    fn reset(&mut self) { self.phase = 0.0; }
}
