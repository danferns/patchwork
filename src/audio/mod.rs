// Audio Module — manages cpal output stream, mixes audio sources, handles device selection.
// Split from monolithic audio.rs into focused sub-modules.

pub mod smoothed;
pub mod buffers;
pub mod waveform;
pub mod biquad;
pub mod effects;
pub mod sources;
pub mod analysis;
pub mod decode;
pub mod manager;
pub mod callback;
pub mod processor;
pub mod processors;
pub mod params;
pub mod chain;
pub mod swap;
pub mod compile;
pub mod engine;

// Re-export everything that external code uses (preserves `use crate::audio::*` compatibility)
pub use smoothed::SmoothedParam;
pub use waveform::{Waveform, SynthParams};
pub use biquad::curve_to_eq_bands;
pub use effects::{AudioEffect, effects_same_types};
pub use sources::AudioSource;
pub use decode::probe_file_duration;
pub use manager::AudioManager;
