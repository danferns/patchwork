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
    /// Sources that should be rendered but NOT mixed to output directly.
    /// These only feed into Mixers or FM carriers.
    pub render_only: std::collections::HashSet<NodeId>,
    pub master_volume: f32,
    pub sample_rate: f32,
}

impl Default for SharedAudioState {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            effects: HashMap::new(),
            active_chains: HashMap::new(),
            render_only: std::collections::HashSet::new(),
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
    pub rodio_sinks: HashMap<NodeId, rodio::Sink>,
    rodio_stream: Option<rodio::OutputStream>,
    _rodio_handle: Option<rodio::OutputStreamHandle>,
    pub file_playing: HashMap<NodeId, bool>,
    pub file_durations: HashMap<NodeId, f64>,  // seconds
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

    /// Update mixer inputs for a node
    pub fn set_mixer(&self, node_id: NodeId, inputs: Vec<(NodeId, f32)>) {
        if let Ok(mut s) = self.state.try_lock() {
            s.sources.insert(node_id, AudioSource::Mixer { inputs });
        }
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

    pub fn get_master_volume(&self) -> f32 {
        self.state.try_lock().ok().map(|s| s.master_volume).unwrap_or(0.8)
    }

    pub fn set_master_volume(&self, vol: f32) {
        if let Ok(mut s) = self.state.try_lock() {
            s.master_volume = vol.clamp(0.0, 1.0);
        }
    }

    /// Remove a source from active chains (Speaker disconnected or muted)
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

    // ── Two-pass FM synthesis ─────────────────────────────────────────
    // Pass 1: identify modulators (no fm_source) vs carriers (have fm_source)
    // Pass 2: generate modulators first, then carriers using modulator output

    // Collect dependency info before mutating sources
    // Each source either: has no deps (plain synth), has FM dep, or is a Mixer (deps on inputs)
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
            _ => {}
        }
    }

    // Dependency-ordered rendering: render nodes whose deps are all satisfied first
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
                    // Mix all input sources weighted by gain
                    let mut buf = vec![0.0f32; num_frames];
                    let inputs_snapshot: Vec<(NodeId, f32)> = inputs.clone();
                    for (src_id, gain) in &inputs_snapshot {
                        if let Some(src_buf) = rendered.get(src_id) {
                            for frame in 0..num_frames {
                                buf[frame] += src_buf[frame] * gain;
                            }
                        }
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
            _ => {}
        }
    }

    // ── Mix rendered buffers to output (skip render-only nodes) ────────
    for &nid in &active_ids {
        // Skip render-only sources — they only feed into Mixers/FM, not output directly
        if s.render_only.contains(&nid) { continue; }

        if let Some(buf) = rendered.get(&nid) {
            for frame in 0..num_frames {
                let sample = buf[frame];
                for ch in 0..channels {
                    data[frame * channels + ch] += sample * master_vol;
                }
            }
        }

        // Apply the active chain's effects
        if let Some(fx_chain) = s.active_chains.get_mut(&nid) {
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
