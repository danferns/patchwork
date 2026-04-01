use std::collections::HashMap;
use super::sources::AudioSource;
use super::effects::AudioEffect;
use crate::graph::NodeId;

// ── Shared Audio State ───────────────────────────────────────────────────────

/// Shared between UI thread and audio callback thread
pub struct SharedAudioState {
    pub sources: HashMap<NodeId, AudioSource>,
    pub effects: HashMap<NodeId, Vec<AudioEffect>>,
    /// Audio chains: source_node_id → Vec of effect specs in order.
    /// Only sources in this map actually play (routed through a Speaker).
    pub active_chains: HashMap<NodeId, Vec<AudioEffect>>,
    /// Per-channel effect chains for Mixer nodes.
    /// Key: (mixer_node_id, channel_index). Effects are applied to each channel's
    /// source buffer DURING mixing, before accumulation — so each channel can have
    /// independently evolving filter state and delay buffers.
    pub channel_chains: HashMap<(NodeId, usize), Vec<AudioEffect>>,
    /// Sources that should be rendered but NOT mixed to output directly.
    /// These only feed into Mixers or FM carriers.
    pub render_only: std::collections::HashSet<NodeId>,
    pub master_volume: f32,
    pub sample_rate: f32,
    /// Real-time audio analysis of the master output mix
    pub analysis: AudioAnalysis,
    /// Per-source audio analysis (keyed by source NodeId).
    /// Computed from each source's rendered buffer before mixing.
    pub source_analysis: HashMap<NodeId, AudioAnalysis>,
    /// Which source NodeIds to compute per-source analysis for.
    /// Set by AudioAnalyzer nodes that are connected to specific audio sources.
    pub analyze_sources: std::collections::HashSet<NodeId>,
    /// Audio callback performance metrics
    pub callback_duration_us: f32,   // last callback duration in microseconds
    pub callback_budget_us: f32,     // budget per callback (buffer_size / sample_rate * 1e6)
}

/// Real-time audio analysis — computed from the output mix each audio callback.
/// All values are 0.0–1.0 (smoothed).
#[derive(Clone, Debug)]
pub struct AudioAnalysis {
    /// RMS amplitude (overall volume level)
    pub amplitude: f32,
    /// Peak sample value
    pub peak: f32,
    /// Low frequency energy (bass, ~0–300 Hz)
    pub bass: f32,
    /// Mid frequency energy (~300–2000 Hz)
    pub mid: f32,
    /// High frequency energy (treble, ~2000 Hz+)
    pub treble: f32,
    // Internal filter states for band splitting (not exposed)
    bass_state: f32,
    #[allow(dead_code)]
    mid_state: f32, // reserved for future multi-pole mid filter
    treble_state: f32,
}

impl Default for AudioAnalysis {
    fn default() -> Self {
        Self {
            amplitude: 0.0, peak: 0.0,
            bass: 0.0, mid: 0.0, treble: 0.0,
            bass_state: 0.0, mid_state: 0.0, treble_state: 0.0,
        }
    }
}

impl AudioAnalysis {
    /// Update analysis from a mono output buffer. Uses simple one-pole band-split
    /// filters and exponential smoothing for stable, reactive values.
    pub fn update(&mut self, data: &[f32], channels: usize, sample_rate: f32) {
        let num_frames = data.len() / channels;
        if num_frames == 0 { return; }

        let mut sum_sq = 0.0f32;
        let mut peak = 0.0f32;
        let mut bass_energy = 0.0f32;
        let mut mid_energy = 0.0f32;
        let mut treble_energy = 0.0f32;

        // One-pole filter coefficients for band splitting
        let bass_coeff = (std::f32::consts::TAU * 300.0 / sample_rate).min(1.0);
        let treble_coeff = (std::f32::consts::TAU * 2000.0 / sample_rate).min(1.0);

        for frame in 0..num_frames {
            // Mix channels to mono
            let mut sample = 0.0f32;
            for ch in 0..channels {
                sample += data[frame * channels + ch];
            }
            sample /= channels as f32;

            sum_sq += sample * sample;
            peak = peak.max(sample.abs());

            // Band-split using one-pole filters
            self.bass_state += bass_coeff * (sample - self.bass_state);
            let bass_sample = self.bass_state; // low-passed = bass

            self.treble_state += treble_coeff * (sample - self.treble_state);
            let treble_sample = sample - self.treble_state; // high-passed = treble

            let mid_sample = self.treble_state - self.bass_state; // band-passed = mid

            bass_energy += bass_sample * bass_sample;
            mid_energy += mid_sample * mid_sample;
            treble_energy += treble_sample * treble_sample;
        }

        let rms = (sum_sq / num_frames as f32).sqrt();
        let bass_rms = (bass_energy / num_frames as f32).sqrt();
        let mid_rms = (mid_energy / num_frames as f32).sqrt();
        let treble_rms = (treble_energy / num_frames as f32).sqrt();

        // Exponential smoothing (attack fast, decay slow)
        let attack = 0.6;
        let decay = 0.05;
        let smooth = |old: f32, new: f32| {
            if new > old { old + attack * (new - old) }
            else { old + decay * (new - old) }
        };

        self.amplitude = smooth(self.amplitude, rms.min(1.0));
        self.peak = smooth(self.peak, peak.min(1.0));
        // Scale band energies to roughly 0–1 (multiply by a boost factor)
        self.bass = smooth(self.bass, (bass_rms * 3.0).min(1.0));
        self.mid = smooth(self.mid, (mid_rms * 4.0).min(1.0));
        self.treble = smooth(self.treble, (treble_rms * 5.0).min(1.0));
    }
}

impl Default for SharedAudioState {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            effects: HashMap::new(),
            active_chains: HashMap::new(),
            channel_chains: HashMap::new(),
            render_only: std::collections::HashSet::new(),
            master_volume: 1.0,
            sample_rate: 44100.0,
            analysis: AudioAnalysis::default(),
            source_analysis: HashMap::new(),
            analyze_sources: std::collections::HashSet::new(),
            callback_duration_us: 0.0,
            callback_budget_us: 0.0,
        }
    }
}

