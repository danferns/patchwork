#![allow(dead_code)]
//! Input processors — LiveInput (microphone) and FilePlayer (audio files).
//! Both read from lock-free ring buffers filled by their respective threads.

use std::sync::Arc;
use crate::audio::processor::{AudioProcessor, ProcessorKind, ProcessContext};
use crate::audio::buffers::{LiveInputBuffer, FilePlayerBuffer};

// ── Live Input (Microphone) ───────────────────────────────────────────────────

pub struct LiveInputProcessor {
    pub buffer: Arc<LiveInputBuffer>,
    pub gain: f32,
}

impl LiveInputProcessor {
    pub fn new(buffer: Arc<LiveInputBuffer>, gain: f32) -> Self {
        Self { buffer, gain }
    }
}

impl AudioProcessor for LiveInputProcessor {
    fn type_id(&self) -> &str { "live_input" }
    fn kind(&self) -> ProcessorKind { ProcessorKind::Source }

    fn process_block(&mut self, _input: &[f32], output: &mut [f32], ctx: &ProcessContext) {
        self.buffer.read_into(output, ctx.block_size);
        if (self.gain - 1.0).abs() > 0.001 {
            for s in output.iter_mut() { *s *= self.gain; }
        }
    }

    fn set_params(&mut self, params: &[f32]) {
        if let Some(&v) = params.first() { self.gain = v; }
    }

    fn param_count(&self) -> usize { 1 }
    fn prepare(&mut self, _sample_rate: f32, _max_block_size: usize) {}
    fn reset(&mut self) {}
}

// ── File Player ───────────────────────────────────────────────────────────────

pub struct FilePlayerProcessor {
    pub buffer: Arc<FilePlayerBuffer>,
    pub volume: f32,
}

impl FilePlayerProcessor {
    pub fn new(buffer: Arc<FilePlayerBuffer>, volume: f32) -> Self {
        Self { buffer, volume }
    }
}

impl AudioProcessor for FilePlayerProcessor {
    fn type_id(&self) -> &str { "file_player" }
    fn kind(&self) -> ProcessorKind { ProcessorKind::Source }

    fn process_block(&mut self, _input: &[f32], output: &mut [f32], ctx: &ProcessContext) {
        self.buffer.read_into(output, ctx.block_size);
        if (self.volume - 1.0).abs() > 0.001 {
            for s in output.iter_mut() { *s *= self.volume; }
        }
    }

    fn set_params(&mut self, params: &[f32]) {
        if let Some(&v) = params.first() { self.volume = v; }
    }

    fn param_count(&self) -> usize { 1 }
    fn prepare(&mut self, _sample_rate: f32, _max_block_size: usize) {}
    fn reset(&mut self) {}
}
