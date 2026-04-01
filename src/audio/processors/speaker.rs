//! SpeakerProcessor — outputs audio to the master bus.
//!
//! In the VCV Rack-style engine, Speaker is the node that routes audio
//! to the hardware output. Its process_block copies input (scaled by volume)
//! to its output buffer. The engine then mixes all active speakers to master.

use crate::audio::processor::{AudioProcessor, ProcessorKind, ProcessContext};
use crate::audio::smoothed::SmoothedParam;

pub struct SpeakerProcessor {
    pub volume: SmoothedParam,
    pub active: bool,
}

impl SpeakerProcessor {
    pub fn new(volume: f32) -> Self {
        Self {
            volume: SmoothedParam::new(volume, 5.0),
            active: true,
        }
    }
}

impl AudioProcessor for SpeakerProcessor {
    fn type_id(&self) -> &str { "speaker" }
    fn kind(&self) -> ProcessorKind { ProcessorKind::Output }

    fn process_block(&mut self, input: &[f32], output: &mut [f32], ctx: &ProcessContext) {
        if !self.active {
            // Ramp down to avoid click
            self.volume.set(0.0);
        }

        for i in 0..ctx.block_size {
            let vol = self.volume.tick();
            output[i] = input[i] * vol;
        }
    }

    fn set_params(&mut self, params: &[f32]) {
        // params[0] = volume, params[1] = active (>0.5)
        if let Some(&v) = params.first() {
            if self.active {
                self.volume.set(v.clamp(0.0, 1.0));
            }
        }
        if let Some(&v) = params.get(1) {
            self.active = v > 0.5;
        }
    }

    fn param_count(&self) -> usize { 2 }
    fn prepare(&mut self, _sample_rate: f32, _max_block_size: usize) {}
    fn reset(&mut self) { self.volume.snap(self.volume.target); }
}
