use super::waveform::SynthParams;
use super::buffers::{LiveInputBuffer, FilePlayerBuffer, SamplerBuffer};
use crate::graph::NodeId;
use std::sync::Arc;

// ── Audio Source Entry ────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum AudioSource {
    Synth(SynthParams),
    /// Mixer: references source NodeIds with per-channel gain.
    /// The audio callback mixes the referenced sources' outputs.
    Mixer {
        /// (source_node_id, gain) pairs — gain is 0.0–1.0
        inputs: Vec<(NodeId, f32)>,
    },
    /// Live audio input from a microphone / line-in device.
    /// Reads samples from a lock-free ring buffer filled by a CPAL input stream.
    LiveInput {
        buffer: Arc<LiveInputBuffer>,
        gain: f32,
    },
    /// Audio file playback — decoded by Symphonia in a background thread,
    /// samples fed through a lock-free ring buffer into the audio callback.
    FilePlayer {
        buffer: Arc<FilePlayerBuffer>,
        volume: f32,
    },
    /// Audio sampler — records from input, plays back on trigger.
    /// Buffer is shared between UI (trim controls, waveform display) and audio thread.
    Sampler {
        buffer: Arc<SamplerBuffer>,
        volume: f32,
        /// The upstream source NodeId that feeds audio into this sampler.
        /// Used by the compiler to wire the correct input buffer.
        input_source: Option<NodeId>,
    },
}

