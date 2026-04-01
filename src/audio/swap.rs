#![allow(dead_code)]
//! Lock-free chain swap between UI thread and audio thread.
//!
//! Uses crossbeam bounded(1) channel. UI sends, audio receives.
//! If the channel is full (audio hasn't consumed yet), the UI
//! force-replaces by receiving the old one first.

use crossbeam_channel::{Sender, Receiver, bounded};
use super::chain::CompiledDspChain;

/// Create a chain swap pair (sender for UI, receiver for audio).
pub fn chain_swap() -> (ChainSender, ChainReceiver) {
    let (tx, rx) = bounded(1);
    // Clone rx for the sender's drain capability
    let drain_rx = rx.clone();
    (ChainSender { tx, drain_rx }, ChainReceiver { rx })
}

/// UI-side: sends compiled chains to the audio thread.
pub struct ChainSender {
    tx: Sender<CompiledDspChain>,
    drain_rx: Receiver<CompiledDspChain>,
}

impl ChainSender {
    /// Send a new chain. Replaces any pending chain the audio hasn't consumed yet.
    pub fn send(&self, chain: CompiledDspChain) {
        // Drain any unconsumed chain
        let _ = self.drain_rx.try_recv();
        // Send the new one (should always succeed after drain)
        let _ = self.tx.try_send(chain);
    }
}

/// Audio-side: receives compiled chains. Non-blocking.
pub struct ChainReceiver {
    rx: Receiver<CompiledDspChain>,
}

impl ChainReceiver {
    /// Check for a new chain. Returns None if nothing new.
    pub fn try_recv(&self) -> Option<CompiledDspChain> {
        self.rx.try_recv().ok()
    }
}
