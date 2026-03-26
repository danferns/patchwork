// Audio Manager — manages cpal output stream, mixes audio sources, handles device selection.
// Each audio source (synth, file player) writes into a shared ring buffer.
// The cpal callback reads from the mix buffer and sends to the output device.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::graph::NodeId;

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
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            waveform: Waveform::Sine,
            frequency: 440.0,
            amplitude: 0.5,
            phase: 0.0,
            active: true,
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
fn effects_same_types(a: &[AudioEffect], b: &[AudioEffect]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.type_tag() == y.type_tag())
}

// ── Audio Source Entry ────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum AudioSource {
    Synth(SynthParams),
    // File player uses rodio's Sink — handled separately
}

// ── Shared Audio State ───────────────────────────────────────────────────────

/// Shared between UI thread and audio callback thread
pub struct SharedAudioState {
    pub sources: HashMap<NodeId, AudioSource>,
    pub effects: HashMap<NodeId, Vec<AudioEffect>>,
    pub master_volume: f32,
    pub sample_rate: f32,
}

impl Default for SharedAudioState {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            effects: HashMap::new(),
            master_volume: 0.8,
            sample_rate: 44100.0,
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
    pub rodio_sink: Option<rodio::Sink>,
    rodio_stream: Option<rodio::OutputStream>,
    _rodio_handle: Option<rodio::OutputStreamHandle>,
    pub file_playing: HashMap<NodeId, bool>,
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
            rodio_sink: None,
            rodio_stream: None,
            _rodio_handle: None,
            file_playing: HashMap::new(),
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
            rodio_sink: None,
            rodio_stream: None,
            _rodio_handle: None,
            file_playing: HashMap::new(),
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

    /// Play an audio file via rodio
    pub fn play_file(&mut self, node_id: NodeId, path: &str) -> Result<(), String> {
        // Initialize rodio stream if needed
        if self.rodio_stream.is_none() {
            let (stream, handle) = rodio::OutputStream::try_default()
                .map_err(|e| format!("Rodio output: {}", e))?;
            self.rodio_stream = Some(stream);
            self._rodio_handle = Some(handle);
        }

        let handle = self._rodio_handle.as_ref().ok_or("No rodio handle")?;
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Open file: {}", e))?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))
            .map_err(|e| format!("Decode: {}", e))?;

        let sink = rodio::Sink::try_new(handle)
            .map_err(|e| format!("Sink: {}", e))?;
        sink.append(source);

        self.rodio_sink = Some(sink);
        self.file_playing.insert(node_id, true);
        Ok(())
    }

    /// Pause/resume file playback
    pub fn toggle_file(&mut self, _node_id: NodeId) {
        if let Some(sink) = &self.rodio_sink {
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
    }

    /// Stop file playback
    pub fn stop_file(&mut self, node_id: NodeId) {
        if let Some(sink) = self.rodio_sink.take() {
            sink.stop();
        }
        self.file_playing.remove(&node_id);
    }

    /// Cleanup when a node is deleted
    pub fn cleanup_node(&mut self, node_id: NodeId) {
        self.remove_source(node_id);
        self.stop_file(node_id);
    }
}

// ── Audio Callback (runs on audio thread) ────────────────────────────────────

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
        Err(_) => return, // UI thread holds lock, output silence this buffer
    };

    let sample_rate = s.sample_rate;
    let master_vol = s.master_volume;
    let num_frames = data.len() / channels;

    // Mix all active sources
    // Collect IDs to avoid borrow conflicts between sources and effects
    let node_ids: Vec<NodeId> = s.sources.keys().copied().collect();
    for node_id in node_ids {
        let source = match s.sources.get_mut(&node_id) { Some(s) => s, None => continue };
        match source {
            AudioSource::Synth(params) => {
                if !params.active || params.amplitude < 0.001 {
                    continue;
                }

                for frame in 0..num_frames {
                    let sample = params.waveform.sample(params.phase) * params.amplitude;

                    // Advance phase
                    params.phase += params.frequency / sample_rate;
                    if params.phase >= 1.0 {
                        params.phase -= 1.0;
                    }

                    // Write to all channels
                    for ch in 0..channels {
                        data[frame * channels + ch] += sample * master_vol;
                    }
                }
            }
        }

        // Apply effects chain (separate borrow scope)
        if let Some(fx_chain) = s.effects.get_mut(&node_id) {
            for frame in 0..num_frames {
                for ch in 0..channels {
                    let idx = frame * channels + ch;
                    let mut sample = data[idx];
                    for fx in fx_chain.iter_mut() {
                        sample = fx.process(sample, sample_rate);
                    }
                    data[idx] = sample;
                }
            }
        }
    }

    // Clamp output to prevent clipping
    for sample in data.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }
}
