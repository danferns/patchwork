// Audio Manager — manages cpal output stream, mixes audio sources, handles device selection.
// Each audio source (synth, file player) writes into a shared ring buffer.
// The cpal callback reads from the mix buffer and sends to the output device.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::graph::NodeId;

// ── Lock-free Ring Buffer for Live Audio Input ──────────────────────────────

/// Single-producer single-consumer lock-free ring buffer for passing audio
/// samples from a CPAL input callback thread to the audio output callback thread.
/// No mutex — uses atomic read/write positions for synchronization.
pub struct LiveInputBuffer {
    data: UnsafeCell<Vec<f32>>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

// Safety: LiveInputBuffer is designed for single-producer (input callback)
// single-consumer (output callback) use. The atomic positions ensure that
// the producer and consumer never access the same region simultaneously.
unsafe impl Send for LiveInputBuffer {}
unsafe impl Sync for LiveInputBuffer {}

impl LiveInputBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: UnsafeCell::new(vec![0.0f32; capacity]),
            capacity,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        }
    }

    /// Write samples from the CPAL input callback (producer).
    /// Wraps around the ring buffer. If the buffer is full, overwrites old data.
    pub fn write(&self, samples: &[f32]) {
        let data = unsafe { &mut *self.data.get() };
        let mut wp = self.write_pos.load(Ordering::Relaxed);
        // For multi-channel input, mix down to mono by averaging channels
        for &s in samples {
            data[wp % self.capacity] = s;
            wp = wp.wrapping_add(1);
        }
        self.write_pos.store(wp, Ordering::Release);
    }

    /// Write interleaved multi-channel samples, mixing down to mono.
    pub fn write_interleaved(&self, samples: &[f32], channels: usize) {
        if channels <= 1 {
            self.write(samples);
            return;
        }
        let data = unsafe { &mut *self.data.get() };
        let mut wp = self.write_pos.load(Ordering::Relaxed);
        let inv_ch = 1.0 / channels as f32;
        for frame in samples.chunks_exact(channels) {
            let mono: f32 = frame.iter().sum::<f32>() * inv_ch;
            data[wp % self.capacity] = mono;
            wp = wp.wrapping_add(1);
        }
        self.write_pos.store(wp, Ordering::Release);
    }

    /// Read samples into the output buffer (consumer).
    /// If not enough samples are available, fills remainder with silence.
    pub fn read_into(&self, buf: &mut [f32], num_frames: usize) {
        let data = unsafe { &*self.data.get() };
        let wp = self.write_pos.load(Ordering::Acquire);
        let mut rp = self.read_pos.load(Ordering::Relaxed);

        let available = wp.wrapping_sub(rp);
        let to_read = num_frames.min(available).min(buf.len());

        for i in 0..to_read {
            buf[i] = data[rp % self.capacity];
            rp = rp.wrapping_add(1);
        }
        // Fill remainder with silence if underrun
        for i in to_read..num_frames.min(buf.len()) {
            buf[i] = 0.0;
        }
        self.read_pos.store(rp, Ordering::Release);
    }
}

// ── Audio Source Trait ────────────────────────────────────────────────────────

/// Each audio source generates samples on demand.
/// Called from the audio thread — must be lock-free / fast.
#[derive(Clone)]
pub struct SynthParams {
    pub waveform: Waveform,
    pub frequency: f32,
    pub amplitude: f32,
    pub phase: f32,         // current phase (0..1), updated by audio thread
    pub active: bool,
    /// FM modulation: which synth node modulates this synth's frequency
    pub fm_source: Option<NodeId>,
    /// FM modulation depth in Hz (modulator output * depth = frequency offset)
    pub fm_depth: f32,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            waveform: Waveform::Sine,
            frequency: 440.0,
            amplitude: 0.5,
            phase: 0.0,
            active: true,
            fm_source: None,
            fm_depth: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum Waveform {
    #[default]
    Sine,
    Saw,
    Square,
    Triangle,
    Noise,
}

impl Waveform {
    pub fn all() -> &'static [Waveform] {
        &[Waveform::Sine, Waveform::Saw, Waveform::Square, Waveform::Triangle, Waveform::Noise]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Waveform::Sine => "Sine",
            Waveform::Saw => "Saw",
            Waveform::Square => "Square",
            Waveform::Triangle => "Triangle",
            Waveform::Noise => "Noise",
        }
    }

    /// Generate sample at phase (0..1)
    pub fn sample(&self, phase: f32) -> f32 {
        match self {
            Waveform::Sine => (phase * std::f32::consts::TAU).sin(),
            Waveform::Saw => 2.0 * phase - 1.0,
            Waveform::Square => if phase < 0.5 { 1.0 } else { -1.0 },
            Waveform::Triangle => {
                if phase < 0.25 { phase * 4.0 }
                else if phase < 0.75 { 2.0 - phase * 4.0 }
                else { phase * 4.0 - 4.0 }
            }
            Waveform::Noise => fastrand_f32() * 2.0 - 1.0,
        }
    }
}

fn fastrand_f32() -> f32 {
    // Simple LCG random for noise - not thread safe but fine for audio
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u32> = const { Cell::new(12345) };
    }
    SEED.with(|s| {
        let v = s.get().wrapping_mul(1103515245).wrapping_add(12345);
        s.set(v);
        (v >> 16) as f32 / 32768.0
    })
}

// ── Audio Effects ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AudioEffect {
    Gain { level: f32 },                    // 0..2, 1.0 = unity
    LowPass { cutoff: f32, state: f32 },    // Simple 1-pole
    HighPass { cutoff: f32, state: f32 },
    Delay { time_ms: f32, feedback: f32, buffer: Vec<f32>, write_pos: usize },
    Distortion { drive: f32 },              // 1..20
}

impl AudioEffect {
    pub fn name(&self) -> &'static str {
        match self {
            AudioEffect::Gain { .. } => "Gain",
            AudioEffect::LowPass { .. } => "Low Pass",
            AudioEffect::HighPass { .. } => "High Pass",
            AudioEffect::Delay { .. } => "Delay",
            AudioEffect::Distortion { .. } => "Distortion",
        }
    }

    /// Update user-controlled params from another effect, preserving processing state
    pub fn merge_params(&mut self, other: &AudioEffect) {
        match (self, other) {
            (AudioEffect::Gain { level }, AudioEffect::Gain { level: new_level }) => {
                *level = *new_level;
            }
            (AudioEffect::LowPass { cutoff, .. }, AudioEffect::LowPass { cutoff: new_cutoff, .. }) => {
                *cutoff = *new_cutoff;
                // state preserved!
            }
            (AudioEffect::HighPass { cutoff, .. }, AudioEffect::HighPass { cutoff: new_cutoff, .. }) => {
                *cutoff = *new_cutoff;
            }
            (AudioEffect::Delay { time_ms, feedback, .. }, AudioEffect::Delay { time_ms: new_time, feedback: new_fb, .. }) => {
                *time_ms = *new_time;
                *feedback = *new_fb;
                // buffer and write_pos preserved!
            }
            (AudioEffect::Distortion { drive }, AudioEffect::Distortion { drive: new_drive }) => {
                *drive = *new_drive;
            }
            _ => {} // Type mismatch — shouldn't happen if effects_same_types() passed
        }
    }

    /// Get a type discriminant for comparison
    fn type_tag(&self) -> u8 {
        match self {
            AudioEffect::Gain { .. } => 0,
            AudioEffect::LowPass { .. } => 1,
            AudioEffect::HighPass { .. } => 2,
            AudioEffect::Delay { .. } => 3,
            AudioEffect::Distortion { .. } => 4,
        }
    }

    /// Process a single sample through this effect
    pub fn process(&mut self, sample: f32, sample_rate: f32) -> f32 {
        match self {
            AudioEffect::Gain { level } => sample * *level,
            AudioEffect::LowPass { cutoff, state } => {
                let rc = 1.0 / (std::f32::consts::TAU * cutoff.max(20.0));
                let dt = 1.0 / sample_rate;
                let alpha = dt / (rc + dt);
                *state = *state + alpha * (sample - *state);
                *state
            }
            AudioEffect::HighPass { cutoff, state } => {
                let rc = 1.0 / (std::f32::consts::TAU * cutoff.max(20.0));
                let dt = 1.0 / sample_rate;
                let alpha = rc / (rc + dt);
                let out = alpha * (*state + sample - *state);
                *state = sample;
                out
            }
            AudioEffect::Delay { time_ms, feedback, buffer, write_pos } => {
                let delay_samples = (*time_ms * sample_rate / 1000.0) as usize;
                if buffer.len() != delay_samples.max(1) {
                    *buffer = vec![0.0; delay_samples.max(1)];
                    *write_pos = 0;
                }
                let read_pos = *write_pos;
                let delayed = buffer[read_pos];
                let output = sample + delayed * *feedback;
                buffer[*write_pos] = output;
                *write_pos = (*write_pos + 1) % buffer.len();
                output
            }
            AudioEffect::Distortion { drive } => {
                let driven = sample * *drive;
                driven.tanh()
            }
        }
    }
}

/// Check if two effects chains have the same types in the same order
pub fn effects_same_types(a: &[AudioEffect], b: &[AudioEffect]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.type_tag() == y.type_tag())
}

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
    // File player uses rodio's Sink — handled separately
}

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
    fn update(&mut self, data: &[f32], channels: usize, sample_rate: f32) {
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
        }
    }
}

// ── Audio Manager ────────────────────────────────────────────────────────────

pub struct AudioManager {
    pub state: Arc<Mutex<SharedAudioState>>,
    stream: Option<cpal::Stream>,
    pub output_device_name: String,
    pub _input_device_name: String,
    // File playback via rodio
    pub rodio_sinks: HashMap<NodeId, rodio::Sink>,
    rodio_stream: Option<rodio::OutputStream>,
    _rodio_handle: Option<rodio::OutputStreamHandle>,
    pub file_playing: HashMap<NodeId, bool>,
    pub file_durations: HashMap<NodeId, f64>,  // seconds
    // Live audio input streams (one per AudioInput node)
    input_streams: HashMap<NodeId, cpal::Stream>,
    pub input_buffers: HashMap<NodeId, Arc<LiveInputBuffer>>,
    // Cached device lists (refreshed periodically, not every frame)
    pub cached_output_devices: Vec<String>,
    pub cached_input_devices: Vec<String>,
}

impl AudioManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(SharedAudioState::default()));

        Self {
            state,
            stream: None,
            output_device_name: String::new(),
            _input_device_name: String::new(),
            rodio_sinks: HashMap::new(),
            rodio_stream: None,
            _rodio_handle: None,
            file_playing: HashMap::new(),
            file_durations: HashMap::new(),
            input_streams: HashMap::new(),
            input_buffers: HashMap::new(),
            cached_output_devices: Vec::new(),
            cached_input_devices: Vec::new(),
        }
    }

    /// Create a cheap placeholder (used during mem::replace swap)
    pub fn placeholder() -> Self {
        Self {
            state: Arc::new(Mutex::new(SharedAudioState::default())),
            stream: None,
            output_device_name: String::new(),
            _input_device_name: String::new(),
            rodio_sinks: HashMap::new(),
            rodio_stream: None,
            _rodio_handle: None,
            file_playing: HashMap::new(),
            file_durations: HashMap::new(),
            input_streams: HashMap::new(),
            input_buffers: HashMap::new(),
            cached_output_devices: Vec::new(),
            cached_input_devices: Vec::new(),
        }
    }

    /// Refresh cached device lists (call every ~60 frames, not every frame)
    #[allow(dead_code)]
    pub fn refresh_devices(&mut self) {
        let host = cpal::default_host();
        self.cached_output_devices = host.output_devices()
            .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default();
        self.cached_input_devices = host.input_devices()
            .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default();
    }

    pub fn set_device_lists(&mut self, output: Vec<String>, input: Vec<String>) {
        self.cached_output_devices = output;
        self.cached_input_devices = input;
    }

    /// Start audio output on the default (or named) device
    pub fn start_output(&mut self, device_name: Option<&str>) -> Result<(), String> {
        // Stop existing stream
        self.stream = None;

        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.output_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .ok_or_else(|| format!("Device '{}' not found", name))?
        } else {
            host.default_output_device()
                .ok_or("No default output device")?
        };

        self.output_device_name = device.name().unwrap_or_default();

        let config = device.default_output_config()
            .map_err(|e| format!("No output config: {}", e))?;

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        {
            let mut s = self.state.lock().unwrap();
            s.sample_rate = sample_rate;
        }

        let state = self.state.clone();

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(data, channels, &state);
            },
            |err| {
                eprintln!("Audio error: {}", err);
            },
            None,
        ).map_err(|e| format!("Build stream failed: {}", e))?;

        stream.play().map_err(|e| format!("Play failed: {}", e))?;
        self.stream = Some(stream);

        Ok(())
    }

    /// Stop audio output
    pub fn stop_output(&mut self) {
        self.stream = None;
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    // ── Live Audio Input ────────────────────────────────────────────────────

    /// Start capturing audio from the named input device (or default).
    /// Creates a CPAL input stream that writes into a lock-free ring buffer.
    pub fn start_input(&mut self, node_id: NodeId, device_name: Option<&str>) -> Result<(), String> {
        // Stop existing input for this node
        self.stop_input(node_id);

        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .ok_or_else(|| format!("Input device '{}' not found", name))?
        } else {
            host.default_input_device()
                .ok_or("No default input device")?
        };

        let config = device.default_input_config()
            .map_err(|e| format!("No input config: {}", e))?;

        let sample_rate = config.sample_rate().0 as usize;
        let channels = config.channels() as usize;

        // Ring buffer holds 1 second of mono audio
        let buffer = Arc::new(LiveInputBuffer::new(sample_rate));
        let buf_clone = buffer.clone();

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                buf_clone.write_interleaved(data, channels);
            },
            |err| {
                eprintln!("Audio input error: {}", err);
            },
            None,
        ).map_err(|e| format!("Build input stream failed: {}", e))?;

        stream.play().map_err(|e| format!("Input play failed: {}", e))?;

        self.input_streams.insert(node_id, stream);
        self.input_buffers.insert(node_id, buffer);

        Ok(())
    }

    /// Stop capturing audio for a node.
    pub fn stop_input(&mut self, node_id: NodeId) {
        self.input_streams.remove(&node_id); // Drops the stream, stopping capture
        self.input_buffers.remove(&node_id);
        // Remove from audio sources
        if let Ok(mut s) = self.state.lock() {
            s.sources.remove(&node_id);
        }
    }

    /// Register or update a live input source in the audio state.
    /// Called each frame from the AudioInput node's render function.
    pub fn set_live_input(&self, node_id: NodeId, buffer: &Arc<LiveInputBuffer>, gain: f32) {
        if let Ok(mut s) = self.state.try_lock() {
            s.sources.insert(node_id, AudioSource::LiveInput {
                buffer: buffer.clone(),
                gain,
            });
        }
    }

    /// Update synth parameters for a node (preserves phase from audio thread)
    pub fn set_synth(&self, node_id: NodeId, params: SynthParams) {
        if let Ok(mut s) = self.state.try_lock() {
            // Preserve the running phase from the audio thread
            let existing_phase = match s.sources.get(&node_id) {
                Some(AudioSource::Synth(existing)) => existing.phase,
                _ => 0.0,
            };
            s.sources.insert(node_id, AudioSource::Synth(SynthParams {
                phase: existing_phase,
                ..params
            }));
        }
        // If lock fails, skip this frame's update (audio thread is busy — no clicking)
    }

    /// Remove a source
    pub fn remove_source(&self, node_id: NodeId) {
        if let Ok(mut s) = self.state.lock() {
            s.sources.remove(&node_id);
            s.effects.remove(&node_id);
        }
    }

    /// Update effects chain for a node — preserves audio processing state (filter state,
    /// delay buffers) while updating user-controlled params (cutoff, feedback, etc.).
    /// This prevents clicks/pops from resetting filter state every frame.
    pub fn set_effects(&self, node_id: NodeId, new_effects: Vec<AudioEffect>) {
        if let Ok(mut s) = self.state.try_lock() {
            if let Some(existing) = s.effects.get_mut(&node_id) {
                // If chain length or types changed, replace entirely
                if existing.len() != new_effects.len() || !effects_same_types(existing, &new_effects) {
                    s.effects.insert(node_id, new_effects);
                } else {
                    // Same structure — merge params only, preserve state
                    for (old, new) in existing.iter_mut().zip(new_effects.iter()) {
                        old.merge_params(new);
                    }
                }
            } else {
                s.effects.insert(node_id, new_effects);
            }
        }
    }

    /// Set the active audio chain for a source node (called by Speaker node logic).
    /// Only sources with active chains will produce sound.
    #[allow(dead_code)]
    pub fn set_active_chain(&self, source_node_id: NodeId, effects: Vec<AudioEffect>) {
        if let Ok(mut s) = self.state.try_lock() {
            if let Some(existing) = s.active_chains.get_mut(&source_node_id) {
                if existing.len() != effects.len() || !effects_same_types(existing, &effects) {
                    s.active_chains.insert(source_node_id, effects);
                } else {
                    for (old, new) in existing.iter_mut().zip(effects.iter()) {
                        old.merge_params(new);
                    }
                }
            } else {
                s.active_chains.insert(source_node_id, effects);
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_master_volume(&self) -> f32 {
        self.state.try_lock().ok().map(|s| s.master_volume).unwrap_or(0.8)
    }

    /// Remove a source from active chains (Speaker disconnected or muted)
    #[allow(dead_code)]
    pub fn remove_active_chain(&self, source_node_id: NodeId) {
        if let Ok(mut s) = self.state.try_lock() {
            s.active_chains.remove(&source_node_id);
        }
    }

    /// Play an audio file — if paused, resume; if stopped/new, start fresh
    pub fn play_file(&mut self, node_id: NodeId, path: &str) -> Result<(), String> {
        // If this node has an existing paused sink, just resume it
        if let Some(sink) = self.rodio_sinks.get(&node_id) {
            if sink.is_paused() {
                sink.play();
                self.file_playing.insert(node_id, true);
                return Ok(());
            }
            if !sink.empty() {
                // Already playing
                self.file_playing.insert(node_id, true);
                return Ok(());
            }
        }

        // Remove any finished/old sink for this node
        if let Some(old_sink) = self.rodio_sinks.remove(&node_id) {
            old_sink.stop();
        }

        // Initialize rodio stream if needed
        if self.rodio_stream.is_none() {
            let (stream, handle) = rodio::OutputStream::try_default()
                .map_err(|e| format!("Rodio output: {}", e))?;
            self.rodio_stream = Some(stream);
            self._rodio_handle = Some(handle);
        }

        let handle = self._rodio_handle.as_ref().ok_or("No rodio handle")?;

        // Get duration first (separate decoder instance)
        if !self.file_durations.contains_key(&node_id) {
            if let Ok(f) = std::fs::File::open(path) {
                if let Ok(dec) = rodio::Decoder::new(std::io::BufReader::new(f)) {
                    use rodio::Source;
                    if let Some(dur) = dec.total_duration() {
                        self.file_durations.insert(node_id, dur.as_secs_f64());
                    } else {
                        // MP3 and some formats don't report duration — calculate from samples
                        let sample_rate = dec.sample_rate() as f64;
                        let channels = dec.channels() as f64;
                        if sample_rate > 0.0 && channels > 0.0 {
                            let total_samples = dec.count() as f64; // consumes the decoder
                            let duration = total_samples / sample_rate / channels;
                            if duration > 0.0 {
                                self.file_durations.insert(node_id, duration);
                            }
                        }
                    }
                }
            }
        }

        let file = std::fs::File::open(path)
            .map_err(|e| format!("Open file: {}", e))?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))
            .map_err(|e| format!("Decode: {}", e))?;

        let sink = rodio::Sink::try_new(handle)
            .map_err(|e| format!("Sink: {}", e))?;
        sink.append(source);

        self.rodio_sinks.insert(node_id, sink);
        self.file_playing.insert(node_id, true);
        Ok(())
    }

    /// Pause file playback for a specific node (keeps position, can resume)
    pub fn pause_file(&mut self, node_id: NodeId) {
        if let Some(sink) = self.rodio_sinks.get(&node_id) {
            sink.pause();
        }
        self.file_playing.insert(node_id, false);
    }

    /// Seek file playback to a specific position (seconds).
    /// Stops current sink, creates new one skipping to the position.
    pub fn seek_file(&mut self, node_id: NodeId, path: &str, position_secs: f64) -> Result<(), String> {
        // Stop current playback
        self.rodio_sinks.remove(&node_id);

        let handle = self._rodio_handle.as_ref().ok_or("No rodio handle")?;
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Open: {}", e))?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))
            .map_err(|e| format!("Decode: {}", e))?;

        // Skip to the seek position
        use rodio::Source;
        let skipped = source.skip_duration(std::time::Duration::from_secs_f64(position_secs));

        let sink = rodio::Sink::try_new(handle)
            .map_err(|e| format!("Sink: {}", e))?;
        sink.append(skipped);

        self.rodio_sinks.insert(node_id, sink);
        self.file_playing.insert(node_id, true);
        Ok(())
    }

    /// Check if a specific node's file sink is paused
    pub fn is_file_paused(&self, node_id: NodeId) -> bool {
        self.rodio_sinks.get(&node_id).map(|s| s.is_paused()).unwrap_or(false)
    }

    /// Check if a specific node's sink has finished playing (empty = done)
    pub fn is_file_finished(&self, node_id: NodeId) -> bool {
        self.rodio_sinks.get(&node_id).map(|s| s.empty() && !s.is_paused()).unwrap_or(false)
    }

    /// Get duration of a file for a node (in seconds)
    pub fn get_file_duration(&self, node_id: NodeId) -> f64 {
        self.file_durations.get(&node_id).copied().unwrap_or(0.0)
    }

    /// Set volume for a specific node's file sink
    pub fn set_file_volume(&self, node_id: NodeId, volume: f32) {
        if let Some(sink) = self.rodio_sinks.get(&node_id) {
            sink.set_volume(volume);
        }
    }

    /// Stop file playback completely for a specific node (resets position)
    pub fn stop_file(&mut self, node_id: NodeId) {
        if let Some(sink) = self.rodio_sinks.remove(&node_id) {
            sink.stop();
        }
        self.file_playing.remove(&node_id);
    }

    /// Cleanup when a node is deleted
    pub fn cleanup_node(&mut self, node_id: NodeId) {
        self.remove_source(node_id);
        self.stop_file(node_id);
        self.stop_input(node_id);
    }

    /// Read the latest audio analysis (amplitude, bass, mid, treble).
    /// If `source_id` is Some, returns analysis for that specific source.
    /// If None, returns the master output mix analysis.
    pub fn get_analysis(&self) -> Option<AudioAnalysis> {
        self.state.try_lock().ok().map(|s| s.analysis.clone())
    }

    /// Get per-source analysis for a specific audio source node.
    pub fn get_source_analysis(&self, source_id: NodeId) -> Option<AudioAnalysis> {
        self.state.try_lock().ok().and_then(|s| s.source_analysis.get(&source_id).cloned())
    }

    /// Register a source node for per-source analysis.
    pub fn request_analysis(&self, source_id: NodeId) {
        if let Ok(mut s) = self.state.try_lock() {
            s.analyze_sources.insert(source_id);
        }
    }
}

// ── Audio Callback (runs on audio thread) ────────────────────────────────────

/// Generate samples for a single synth into a buffer, optionally applying FM modulation.
/// Returns the buffer (for use as FM source by downstream carriers).
fn generate_synth_buffer(
    params: &mut SynthParams,
    num_frames: usize,
    sample_rate: f32,
    fm_buf: Option<&[f32]>,
) -> Vec<f32> {
    let mut buf = vec![0.0f32; num_frames];
    if !params.active || params.amplitude < 0.001 {
        return buf;
    }

    for frame in 0..num_frames {
        // FM: offset frequency by modulator's sample × depth
        let freq = params.frequency + match fm_buf {
            Some(mod_samples) => mod_samples[frame] * params.fm_depth,
            None => 0.0,
        };

        buf[frame] = params.waveform.sample(params.phase) * params.amplitude;

        // Advance phase (freq can go negative from FM — handle wrap in both directions)
        params.phase += freq / sample_rate;
        while params.phase >= 1.0 { params.phase -= 1.0; }
        while params.phase < 0.0 { params.phase += 1.0; }
    }
    buf
}

fn audio_callback(
    data: &mut [f32],
    channels: usize,
    state: &Arc<Mutex<SharedAudioState>>,
) {
    // Zero the buffer
    for sample in data.iter_mut() {
        *sample = 0.0;
    }

    // Use try_lock to avoid blocking the audio thread — if UI holds the lock, skip this buffer
    let mut s = match state.try_lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    let sample_rate = s.sample_rate;
    let master_vol = s.master_volume;
    let num_frames = data.len() / channels;

    // Only play sources that are in active_chains (routed through a Speaker node)
    let active_ids: Vec<NodeId> = s.active_chains.keys().copied().collect();

    // Move channel_chains out of shared state so we can use it freely inside the
    // Mixer rendering arm without conflicting with the s.sources borrow.
    // We put it back before this function returns.
    let mut channel_chains = std::mem::take(&mut s.channel_chains);

    // ── Dependency-ordered rendering ──────────────────────────────────────────
    // Build per-source dependency lists: Synths depend on FM modulators, Mixers
    // depend on their input sources.
    let mut deps: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for &nid in &active_ids {
        match s.sources.get(&nid) {
            Some(AudioSource::Synth(params)) => {
                let mut d = Vec::new();
                if let Some(fm_src) = params.fm_source { d.push(fm_src); }
                deps.insert(nid, d);
            }
            Some(AudioSource::Mixer { inputs }) => {
                deps.insert(nid, inputs.iter().map(|(id, _)| *id).collect());
            }
            Some(AudioSource::LiveInput { .. }) => {
                deps.insert(nid, vec![]); // No dependencies — reads from ring buffer
            }
            _ => {}
        }
    }

    let mut rendered: HashMap<NodeId, Vec<f32>> = HashMap::new();
    let mut remaining: Vec<NodeId> = active_ids.clone();
    let max_passes = 6;

    for _ in 0..max_passes {
        if remaining.is_empty() { break; }
        let mut still_waiting: Vec<NodeId> = Vec::new();

        for nid in remaining {
            let node_deps = deps.get(&nid).cloned().unwrap_or_default();
            let all_deps_ready = node_deps.iter().all(|d| rendered.contains_key(d));

            if !all_deps_ready {
                still_waiting.push(nid);
                continue;
            }

            match s.sources.get_mut(&nid) {
                Some(AudioSource::Synth(params)) => {
                    let fm_buf = params.fm_source.and_then(|src_id| rendered.get(&src_id));
                    let buf = generate_synth_buffer(
                        params, num_frames, sample_rate,
                        fm_buf.map(|v| v.as_slice()),
                    );
                    rendered.insert(nid, buf);
                }
                Some(AudioSource::Mixer { inputs }) => {
                    // Mix input sources with per-channel effects applied before accumulation.
                    // Each channel (nid, ch) has its own effect chain in channel_chains,
                    // so different channels can have different filter/delay state independently —
                    // even if two channels reference the same source node.
                    let mut buf = vec![0.0f32; num_frames];
                    let inputs_snapshot: Vec<(NodeId, f32)> = inputs.clone();
                    // s.sources borrow ends here; channel_chains is a local, so no conflict.
                    for (ch, (src_id, gain)) in inputs_snapshot.iter().enumerate() {
                        if let Some(src_buf) = rendered.get(src_id) {
                            if let Some(fx_chain) = channel_chains.get_mut(&(nid, ch)) {
                                // Apply this channel's effect chain sample-by-sample
                                for frame in 0..num_frames {
                                    let mut sample = src_buf[frame];
                                    for fx in fx_chain.iter_mut() {
                                        sample = fx.process(sample, sample_rate);
                                    }
                                    buf[frame] += sample * gain;
                                }
                            } else {
                                // No effects on this channel — straight mix with gain
                                for frame in 0..num_frames {
                                    buf[frame] += src_buf[frame] * gain;
                                }
                            }
                        }
                    }
                    rendered.insert(nid, buf);
                }
                Some(AudioSource::LiveInput { buffer, gain }) => {
                    let g = *gain;
                    let mut buf = vec![0.0f32; num_frames];
                    buffer.read_into(&mut buf, num_frames);
                    if (g - 1.0).abs() > 0.001 {
                        for s in buf.iter_mut() { *s *= g; }
                    }
                    rendered.insert(nid, buf);
                }
                _ => {}
            }
        }

        remaining = still_waiting;
    }

    // Any remaining (circular deps) — render without deps to avoid silence
    for nid in remaining {
        match s.sources.get_mut(&nid) {
            Some(AudioSource::Synth(params)) => {
                let buf = generate_synth_buffer(params, num_frames, sample_rate, None);
                rendered.insert(nid, buf);
            }
            Some(AudioSource::Mixer { .. }) => {
                rendered.insert(nid, vec![0.0; num_frames]);
            }
            Some(AudioSource::LiveInput { buffer, gain }) => {
                let g = *gain;
                let mut buf = vec![0.0f32; num_frames];
                buffer.read_into(&mut buf, num_frames);
                if (g - 1.0).abs() > 0.001 {
                    for s in buf.iter_mut() { *s *= g; }
                }
                rendered.insert(nid, buf);
            }
            _ => {}
        }
    }

    // ── Per-source audio analysis (for AudioAnalyzer nodes) ────────────
    for &src_id in &s.analyze_sources.clone() {
        if let Some(buf) = rendered.get(&src_id) {
            // Build a mono "data" slice for the analysis update
            let entry = s.source_analysis.entry(src_id).or_insert_with(AudioAnalysis::default);
            entry.update(buf, 1, sample_rate);
        }
    }

    // ── Mix rendered buffers to output (skip render-only nodes) ────────
    for &nid in &active_ids {
        // Skip render-only sources — they only feed into Mixers/FM, not output directly
        if s.render_only.contains(&nid) { continue; }

        // Apply effects to the MONO rendered buffer first, then copy to stereo output.
        // Previously effects were applied to the interleaved stereo `data` buffer, which
        // called process() once per output channel (2× for stereo). This caused stateful
        // effects (Delay, LowPass, HighPass) to advance their internal state twice per
        // frame — halving delay times and distorting filter cutoffs.
        if let Some(fx_chain) = s.active_chains.get_mut(&nid) {
            if !fx_chain.is_empty() {
                if let Some(buf) = rendered.get_mut(&nid) {
                    for frame in 0..num_frames {
                        let mut sample = buf[frame];
                        for fx in fx_chain.iter_mut() {
                            sample = fx.process(sample, sample_rate);
                        }
                        buf[frame] = sample;
                    }
                }
            }
        }

        // Write the (now-processed) mono signal to all output channels.
        if let Some(buf) = rendered.get(&nid) {
            for frame in 0..num_frames {
                let sample = buf[frame];
                for ch in 0..channels {
                    data[frame * channels + ch] += sample * master_vol;
                }
            }
        }
    }

    // Clamp output to prevent clipping
    for sample in data.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }

    // Compute real-time audio analysis from the final output mix
    s.analysis.update(data, channels, sample_rate);

    // Restore channel_chains (with updated internal state: filter coefficients,
    // delay buffer write positions) back into shared state for next callback.
    s.channel_chains = channel_chains;
}
