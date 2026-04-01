//! Atomic f32 for lock-free parameter passing between UI and audio threads.

use std::sync::atomic::{AtomicU32, Ordering};

/// Atomic f32 — Rust doesn't have AtomicF32, so we use AtomicU32 with bit casting.
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}
