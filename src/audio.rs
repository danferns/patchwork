// Audio Manager — manages cpal output stream, mixes audio sources, handles device selection.
// Each audio source (synth, file player) writes into a shared ring buffer.
// The cpal callback reads from the mix buffer and sends to the output device.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
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

// ── Lock-free Ring Buffer for File Playback ──────────────────────────────────

/// SPSC ring buffer for streaming decoded audio file samples from a background
/// decode thread (Symphonia) into the audio output callback.
/// Same architecture as LiveInputBuffer, with additional control signals for
/// play/pause/seek/stop coordination between UI and decode thread.
pub struct FilePlayerBuffer {
    data: UnsafeCell<Vec<f32>>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
    /// Decode thread sets when file reaches EOF
    pub finished: AtomicBool,
    /// UI sets to pause decode thread (audio callback returns silence)
    pub paused: AtomicBool,
    /// UI signals decode thread to seek
    pub seek_requested: AtomicBool,
    /// Target seek position in seconds (Mutex OK — only read by decode thread, not audio thread)
    pub seek_target_secs: Mutex<f64>,
    /// UI signals decode thread to stop
    pub stop_requested: AtomicBool,
    /// File's native sample rate (set once by decode thread on open)
    pub file_sample_rate: AtomicU32,
    /// Playback position in output samples (updated by audio callback consumer)
    pub playback_position: AtomicUsize,
    /// Decoded position in output samples (updated by decode thread as it writes).
    /// Used for playhead display when no Speaker is consuming.
    pub decoded_position: AtomicUsize,
    /// Total duration in file samples (set by decode thread from metadata)
    pub total_samples: AtomicUsize,
}

unsafe impl Send for FilePlayerBuffer {}
unsafe impl Sync for FilePlayerBuffer {}

impl FilePlayerBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: UnsafeCell::new(vec![0.0f32; capacity]),
            capacity,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
            finished: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            seek_requested: AtomicBool::new(false),
            seek_target_secs: Mutex::new(0.0),
            stop_requested: AtomicBool::new(false),
            file_sample_rate: AtomicU32::new(44100),
            playback_position: AtomicUsize::new(0),
            decoded_position: AtomicUsize::new(0),
            total_samples: AtomicUsize::new(0),
        }
    }

    /// Write decoded mono samples into the ring buffer (producer: decode thread).
    pub fn write(&self, samples: &[f32]) {
        let data = unsafe { &mut *self.data.get() };
        let mut wp = self.write_pos.load(Ordering::Relaxed);
        for &s in samples {
            data[wp % self.capacity] = s;
            wp = wp.wrapping_add(1);
        }
        self.write_pos.store(wp, Ordering::Release);
    }

    /// Read samples into the output buffer (consumer: audio callback).
    /// Returns silence on underrun — never blocks.
    pub fn read_into(&self, buf: &mut [f32], num_frames: usize) {
        if self.paused.load(Ordering::Relaxed) {
            for i in 0..num_frames.min(buf.len()) { buf[i] = 0.0; }
            return;
        }
        let data = unsafe { &*self.data.get() };
        let wp = self.write_pos.load(Ordering::Acquire);
        let mut rp = self.read_pos.load(Ordering::Relaxed);
        let available = wp.wrapping_sub(rp);
        let to_read = num_frames.min(available).min(buf.len());
        for i in 0..to_read {
            buf[i] = data[rp % self.capacity];
            rp = rp.wrapping_add(1);
        }
        for i in to_read..num_frames.min(buf.len()) {
            buf[i] = 0.0;
        }
        self.read_pos.store(rp, Ordering::Release);
    }

    /// Reset buffer positions (called during seek to flush stale samples).
    pub fn reset(&self) {
        self.write_pos.store(0, Ordering::Release);
        self.read_pos.store(0, Ordering::Release);
        self.finished.store(false, Ordering::Release);
    }

    /// How many output samples are available but not yet consumed by the callback.
    pub fn buffered(&self) -> usize {
        let wp = self.write_pos.load(Ordering::Relaxed);
        let rp = self.read_pos.load(Ordering::Relaxed);
        wp.wrapping_sub(rp)
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
    /// Audio file playback — decoded by Symphonia in a background thread,
    /// samples fed through a lock-free ring buffer into the audio callback.
    FilePlayer {
        buffer: Arc<FilePlayerBuffer>,
        volume: f32,
    },
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

// ── Symphonia File Decode Thread ─────────────────────────────────────────────

/// Background thread function that decodes an audio file with Symphonia and
/// writes mono f32 samples into a FilePlayerBuffer ring buffer.
/// Handles seeking, pausing, looping, and stop signals via atomics.
fn decode_file_thread(
    path: String,
    buffer: Arc<FilePlayerBuffer>,
    output_sample_rate: f32,
    looping: Arc<AtomicBool>,
) {
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::probe::Hint;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::units::Time;

    let open_and_decode = |path: &str, buffer: &FilePlayerBuffer, seek_secs: f64| -> Result<(), String> {
        let file = std::fs::File::open(path).map_err(|e| format!("Open: {}", e))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| format!("Probe: {}", e))?;

        let mut reader = probed.format;

        let track = reader.default_track()
            .ok_or("No default audio track")?;
        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let file_sr = codec_params.sample_rate.unwrap_or(44100) as f32;
        let _file_channels = codec_params.channels
            .map(|ch| ch.count()).unwrap_or(2) as usize;

        buffer.file_sample_rate.store(file_sr as u32, Ordering::Release);
        if let Some(n_frames) = codec_params.n_frames {
            buffer.total_samples.store(n_frames as usize, Ordering::Release);
        }

        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Decoder: {}", e))?;

        // Seek if needed (non-zero start position)
        if seek_secs > 0.01 {
            let _ = reader.seek(
                symphonia::core::formats::SeekMode::Coarse,
                symphonia::core::formats::SeekTo::Time {
                    time: Time::new(seek_secs as u64, seek_secs.fract()),
                    track_id: Some(track_id),
                },
            );
        }

        // Resampling state: linear interpolation
        let resample_ratio = output_sample_rate / file_sr;
        let needs_resample = (resample_ratio - 1.0).abs() > 0.001;
        let mut resample_pos: f64 = 0.0;  // fractional position in source samples

        let mut sample_buf: Option<SampleBuffer<f32>> = None;

        loop {
            // Check stop signal
            if buffer.stop_requested.load(Ordering::Relaxed) {
                return Ok(());
            }

            // Check seek signal
            if buffer.seek_requested.load(Ordering::Relaxed) {
                buffer.seek_requested.store(false, Ordering::Release);
                let target = *buffer.seek_target_secs.lock().unwrap();
                let _ = reader.seek(
                    symphonia::core::formats::SeekMode::Coarse,
                    symphonia::core::formats::SeekTo::Time {
                        time: Time::new(target as u64, target.fract()),
                        track_id: Some(track_id),
                    },
                );
                buffer.reset();
                let new_pos = (target * output_sample_rate as f64) as usize;
                buffer.playback_position.store(new_pos, Ordering::Release);
                buffer.decoded_position.store(new_pos, Ordering::Release);
                resample_pos = 0.0;
                decoder.reset();
                continue;
            }

            // Check pause signal
            if buffer.paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }

            // Backpressure: if the ring buffer is nearly full, either the audio callback
            // is consuming (normal — wait briefly) or no Speaker is connected (buffer stalls).
            // In the latter case, advance read_pos to discard old samples so we keep decoding
            // at real-time pace and the playhead keeps moving.
            let buffered = buffer.buffered();
            if buffered > buffer.capacity * 3 / 4 {
                // If buffer has been full for a while (no consumer), advance read_pos to make room
                let drain = buffer.capacity / 4;
                let rp = buffer.read_pos.load(Ordering::Relaxed);
                buffer.read_pos.store(rp.wrapping_add(drain), Ordering::Release);
                // Brief sleep for real-time pacing (~drain samples at output_sample_rate)
                let sleep_ms = (drain as f64 / output_sample_rate as f64 * 1000.0) as u64;
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms.max(1)));
                continue;
            }

            // Decode next packet
            let packet = match reader.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    // End of file
                    if looping.load(Ordering::Relaxed) {
                        // Seek back to beginning
                        let _ = reader.seek(
                            symphonia::core::formats::SeekMode::Coarse,
                            symphonia::core::formats::SeekTo::Time {
                                time: Time::new(0, 0.0),
                                track_id: Some(track_id),
                            },
                        );
                        buffer.playback_position.store(0, Ordering::Release);
                        buffer.decoded_position.store(0, Ordering::Release);
                        resample_pos = 0.0;
                        decoder.reset();
                        continue;
                    }
                    buffer.finished.store(true, Ordering::Release);
                    return Ok(());
                }
                Err(_) => {
                    buffer.finished.store(true, Ordering::Release);
                    return Ok(());
                }
            };

            if packet.track_id() != track_id {
                continue; // Skip non-audio packets
            }

            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(_) => continue, // Skip decode errors
            };

            // Get samples as interleaved f32
            let spec = *decoded.spec();
            let num_decoded_frames = decoded.frames();
            if num_decoded_frames == 0 { continue; }

            let sb = sample_buf.get_or_insert_with(|| {
                SampleBuffer::<f32>::new(num_decoded_frames as u64, spec)
            });
            // Ensure capacity
            if sb.capacity() < num_decoded_frames {
                *sb = SampleBuffer::<f32>::new(num_decoded_frames as u64, spec);
            }
            sb.copy_interleaved_ref(decoded);
            let interleaved = sb.samples();
            let channels = spec.channels.count().max(1);

            // Downmix to mono
            let mono: Vec<f32> = interleaved.chunks(channels)
                .map(|frame| {
                    let sum: f32 = frame.iter().sum();
                    sum / channels as f32
                })
                .collect();

            // Resample if needed, then write to ring buffer
            if needs_resample && mono.len() > 1 {
                // Linear interpolation resampling
                let src_len = mono.len() as f64;
                let mut resampled = Vec::with_capacity((src_len * resample_ratio as f64) as usize + 1);
                while resample_pos < src_len - 1.0 {
                    let idx = resample_pos as usize;
                    let frac = resample_pos - idx as f64;
                    let s0 = mono[idx];
                    let s1 = mono[(idx + 1).min(mono.len() - 1)];
                    resampled.push(s0 + (s1 - s0) * frac as f32);
                    resample_pos += 1.0 / resample_ratio as f64;
                }
                resample_pos -= src_len - 1.0; // carry fractional part to next packet
                if resample_pos < 0.0 { resample_pos = 0.0; }
                buffer.decoded_position.fetch_add(resampled.len(), Ordering::Relaxed);
                buffer.write(&resampled);
            } else {
                buffer.decoded_position.fetch_add(mono.len(), Ordering::Relaxed);
                buffer.write(&mono);
            }
        }
    };

    if let Err(e) = open_and_decode(&path, &buffer, 0.0) {
        eprintln!("File decode error: {}", e);
        buffer.finished.store(true, Ordering::Release);
    }
}

/// Probe an audio file for its duration using Symphonia metadata (fast, no full decode).
pub fn probe_file_duration(path: &str) -> Option<f64> {
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::probe::Hint;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::meta::MetadataOptions;

    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;
    let track = probed.format.default_track()?;
    let n_frames = track.codec_params.n_frames?;
    let sr = track.codec_params.sample_rate.unwrap_or(44100) as f64;
    Some(n_frames as f64 / sr)
}

// ── Audio Manager ────────────────────────────────────────────────────────────

pub struct AudioManager {
    pub state: Arc<Mutex<SharedAudioState>>,
    stream: Option<cpal::Stream>,
    pub output_device_name: String,
    pub _input_device_name: String,
    // File playback via Symphonia decode thread → FilePlayerBuffer → CPAL callback
    pub file_buffers: HashMap<NodeId, Arc<FilePlayerBuffer>>,
    file_threads: HashMap<NodeId, std::thread::JoinHandle<()>>,
    file_looping: HashMap<NodeId, Arc<AtomicBool>>,
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
            file_buffers: HashMap::new(),
            file_threads: HashMap::new(),
            file_looping: HashMap::new(),
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
            file_buffers: HashMap::new(),
            file_threads: HashMap::new(),
            file_looping: HashMap::new(),
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

    /// Play an audio file — if paused, resume; if stopped/new, start fresh.
    /// Decoded by Symphonia in a background thread → FilePlayerBuffer → CPAL callback.
    pub fn play_file(&mut self, node_id: NodeId, path: &str) -> Result<(), String> {
        // If paused, just resume
        if let Some(buf) = self.file_buffers.get(&node_id) {
            if buf.paused.load(Ordering::Relaxed) {
                buf.paused.store(false, Ordering::Release);
                self.file_playing.insert(node_id, true);
                return Ok(());
            }
            if !buf.finished.load(Ordering::Relaxed) {
                // Already playing
                self.file_playing.insert(node_id, true);
                return Ok(());
            }
        }

        // Stop any existing playback for this node
        self.stop_file(node_id);

        // Get output sample rate from shared state
        let output_sr = self.state.lock()
            .map(|s| s.sample_rate)
            .unwrap_or(44100.0);

        // Probe file for duration (fast metadata read, no full decode)
        if !self.file_durations.contains_key(&node_id) {
            if let Some(dur) = probe_file_duration(path) {
                self.file_durations.insert(node_id, dur);
            }
        }

        // Create ring buffer (2 seconds at output sample rate)
        let capacity = (output_sr * 2.0) as usize;
        let buffer = Arc::new(FilePlayerBuffer::new(capacity));

        // Looping flag shared with decode thread
        let looping = Arc::new(AtomicBool::new(false));
        self.file_looping.insert(node_id, looping.clone());

        // Spawn decode thread
        let buf_clone = buffer.clone();
        let path_owned = path.to_string();
        let looping_clone = looping.clone();
        let handle = std::thread::Builder::new()
            .name(format!("file-decode-{}", node_id))
            .spawn(move || {
                decode_file_thread(path_owned, buf_clone, output_sr, looping_clone);
            })
            .map_err(|e| format!("Spawn decode thread: {}", e))?;

        // Register as audio source so the CPAL callback can render it
        if let Ok(mut s) = self.state.lock() {
            s.sources.insert(node_id, AudioSource::FilePlayer {
                buffer: buffer.clone(),
                volume: 1.0,
            });
        }

        self.file_buffers.insert(node_id, buffer);
        self.file_threads.insert(node_id, handle);
        self.file_playing.insert(node_id, true);
        Ok(())
    }

    /// Pause file playback (keeps position, decode thread sleeps)
    pub fn pause_file(&mut self, node_id: NodeId) {
        if let Some(buf) = self.file_buffers.get(&node_id) {
            buf.paused.store(true, Ordering::Release);
        }
        self.file_playing.insert(node_id, false);
    }

    /// Seek file playback to a specific position (seconds).
    /// Signals the decode thread to seek — no thread restart needed.
    pub fn seek_file(&mut self, node_id: NodeId, _path: &str, position_secs: f64) -> Result<(), String> {
        if let Some(buf) = self.file_buffers.get(&node_id) {
            *buf.seek_target_secs.lock().unwrap() = position_secs;
            buf.seek_requested.store(true, Ordering::Release);
            self.file_playing.insert(node_id, true);
            Ok(())
        } else {
            Err("No active file player".into())
        }
    }

    /// Check if paused
    pub fn is_file_paused(&self, node_id: NodeId) -> bool {
        self.file_buffers.get(&node_id)
            .map(|b| b.paused.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Check if playback has finished (EOF reached and buffer drained)
    pub fn is_file_finished(&self, node_id: NodeId) -> bool {
        self.file_buffers.get(&node_id)
            .map(|b| b.finished.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Get duration of a file for a node (in seconds)
    pub fn get_file_duration(&self, node_id: NodeId) -> f64 {
        self.file_durations.get(&node_id).copied().unwrap_or(0.0)
    }

    /// Get current playback position in seconds.
    /// Uses callback-consumed position when a Speaker is connected (accurate),
    /// falls back to decoded position when no Speaker (playhead still moves).
    pub fn get_playback_position(&self, node_id: NodeId) -> f64 {
        if let Some(buf) = self.file_buffers.get(&node_id) {
            let callback_pos = buf.playback_position.load(Ordering::Relaxed);
            let decoded_pos = buf.decoded_position.load(Ordering::Relaxed);
            // Use whichever is further ahead — callback_pos when Speaker is consuming,
            // decoded_pos when no Speaker is connected
            let pos = callback_pos.max(decoded_pos);
            let sr = self.state.lock().map(|s| s.sample_rate).unwrap_or(44100.0) as f64;
            if sr > 0.0 { pos as f64 / sr } else { 0.0 }
        } else {
            0.0
        }
    }

    /// Set volume for a specific node's file player
    pub fn set_file_volume(&self, node_id: NodeId, volume: f32) {
        if let Ok(mut s) = self.state.try_lock() {
            if let Some(AudioSource::FilePlayer { volume: v, .. }) = s.sources.get_mut(&node_id) {
                *v = volume;
            }
        }
    }

    /// Update looping flag for a specific node's file player
    pub fn set_file_looping(&self, node_id: NodeId, looping: bool) {
        if let Some(flag) = self.file_looping.get(&node_id) {
            flag.store(looping, Ordering::Release);
        }
    }

    /// Stop file playback completely (signals thread, joins, removes source)
    pub fn stop_file(&mut self, node_id: NodeId) {
        // Signal decode thread to stop
        if let Some(buf) = self.file_buffers.get(&node_id) {
            buf.stop_requested.store(true, Ordering::Release);
        }
        // Join the thread (with timeout to avoid blocking)
        if let Some(handle) = self.file_threads.remove(&node_id) {
            let _ = handle.join();
        }
        // Remove buffer and source
        self.file_buffers.remove(&node_id);
        self.file_looping.remove(&node_id);
        self.file_playing.remove(&node_id);
        // Remove from audio source registry
        if let Ok(mut s) = self.state.lock() {
            s.sources.remove(&node_id);
        }
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
            Some(AudioSource::FilePlayer { .. }) => {
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
                Some(AudioSource::FilePlayer { buffer, volume }) => {
                    let vol = *volume;
                    let mut buf = vec![0.0f32; num_frames];
                    buffer.read_into(&mut buf, num_frames);
                    if (vol - 1.0).abs() > 0.001 {
                        for s in buf.iter_mut() { *s *= vol; }
                    }
                    buffer.playback_position.fetch_add(num_frames, Ordering::Relaxed);
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
            Some(AudioSource::FilePlayer { buffer, volume }) => {
                let vol = *volume;
                let mut buf = vec![0.0f32; num_frames];
                buffer.read_into(&mut buf, num_frames);
                if (vol - 1.0).abs() > 0.001 {
                    for s in buf.iter_mut() { *s *= vol; }
                }
                buffer.playback_position.fetch_add(num_frames, Ordering::Relaxed);
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
