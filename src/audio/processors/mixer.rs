#![allow(dead_code)]
//! MixerProcessor — mixes multiple upstream source buffers with per-channel gain.

use crate::audio::processor::{AudioProcessor, ProcessorKind, ProcessContext};

pub struct MixerProcessor {
    /// (buffer_index, gain) pairs — set by chain compiler
    pub input_buffers: Vec<(usize, f32)>,
}

impl MixerProcessor {
    pub fn new() -> Self {
        Self { input_buffers: Vec::new() }
    }
}

impl AudioProcessor for MixerProcessor {
    fn type_id(&self) -> &str { "mixer" }
    fn kind(&self) -> ProcessorKind { ProcessorKind::Mixer }

    fn process_block(&mut self, input: &[f32], output: &mut [f32], ctx: &ProcessContext) {
        // In the new engine, the engine pre-mixes all connected inputs with per-port
        // gains before calling process_block. We just copy input → output.
        output[..ctx.block_size].copy_from_slice(&input[..ctx.block_size]);
    }

    fn set_params(&mut self, _params: &[f32]) {
        // Gains are read directly by the engine from per-node AtomicF32 params
    }

    fn param_count(&self) -> usize { 0 }
    fn prepare(&mut self, _sample_rate: f32, _max_block_size: usize) {}
    fn reset(&mut self) {}
}
