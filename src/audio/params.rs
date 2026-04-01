#![allow(dead_code)]
//! Lock-free parameter storage between UI thread and audio thread.
//!
//! UI writes parameter values via atomic stores (no lock).
//! Audio reads once per block (control rate) via atomic loads.
//! SmoothedParam inside each processor handles per-sample interpolation.

use std::sync::atomic::{AtomicU32, Ordering};
use std::collections::HashMap;
use crate::graph::NodeId;

/// Atomic f32 — Rust doesn't have AtomicF32, so we use AtomicU32 with bit casting.
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    /// Load the current value (Relaxed ordering — fine for parameters).
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// Store a new value (Relaxed ordering — UI thread sets targets).
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Shared parameter storage between UI and audio thread.
///
/// Pre-allocated Vec of AtomicF32 slots. Each processor owns a contiguous
/// range of slots (determined at chain compile time).
///
/// UI thread: writes via `set(slot, value)` — atomic, no lock.
/// Audio thread: reads via `get(slot)` — atomic, no lock.
pub struct ParamStore {
    slots: Vec<AtomicF32>,
}

impl ParamStore {
    /// Create a store with the given number of parameter slots.
    pub fn new(num_slots: usize) -> Self {
        let mut slots = Vec::with_capacity(num_slots);
        for _ in 0..num_slots {
            slots.push(AtomicF32::new(0.0));
        }
        Self { slots }
    }

    /// UI thread: set a parameter value.
    pub fn set(&self, slot: usize, value: f32) {
        if slot < self.slots.len() {
            self.slots[slot].store(value);
        }
    }

    /// Audio thread: read a parameter value.
    pub fn get(&self, slot: usize) -> f32 {
        if slot < self.slots.len() {
            self.slots[slot].load()
        } else {
            0.0
        }
    }

    /// Audio thread: read a contiguous range of parameters into a slice.
    /// Used by processors to read all their params at once (control rate).
    pub fn read_range(&self, start: usize, count: usize, out: &mut [f32]) {
        for i in 0..count.min(out.len()) {
            out[i] = self.get(start + i);
        }
    }

    /// Total number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }
}

// Safety: AtomicF32 uses AtomicU32 internally — Send + Sync by construction.
unsafe impl Send for ParamStore {}
unsafe impl Sync for ParamStore {}

/// Maps (NodeId, param_name) → slot index in ParamStore.
/// Rebuilt when the chain is compiled. Used by the UI to know which slot to write to.
pub struct ParamMap {
    map: HashMap<(NodeId, u16), usize>, // (node_id, param_index) → slot
    next_slot: usize,
}

impl ParamMap {
    pub fn new() -> Self {
        Self { map: HashMap::new(), next_slot: 0 }
    }

    /// Allocate a contiguous range of slots for a processor.
    /// Returns the starting slot index.
    pub fn allocate(&mut self, node_id: NodeId, param_count: usize) -> usize {
        let start = self.next_slot;
        for i in 0..param_count {
            self.map.insert((node_id, i as u16), start + i);
        }
        self.next_slot += param_count;
        start
    }

    /// Look up the slot for a specific (node_id, param_index).
    pub fn get_slot(&self, node_id: NodeId, param_index: u16) -> Option<usize> {
        self.map.get(&(node_id, param_index)).copied()
    }

    /// Total number of slots allocated.
    pub fn total_slots(&self) -> usize {
        self.next_slot
    }
}
