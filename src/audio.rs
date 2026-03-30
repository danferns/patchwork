// Audio Manager — manages cpal output stream, mixes audio sources, handles device selection.
// Each audio source (synth, file player) writes into a shared ring buffer.
// The cpal callback reads from the mix buffer and sends to the output device.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::cell::UnsafeCell;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::graph::NodeId;

// ── Parameter Smoothing ─────────────────────────────────────────────────────

/// One-pole exponential smoother for audio parameters.
/// Eliminates zipper noise when sliders/knobs change at UI rate (~60Hz)
/// by interpolating toward the target value at audio rate (~44kHz).
///
/// Typical smooth_ms values:
///   5ms  — fast response (gain, amplitude)
///  10ms  — medium (filter cutoff, mix)
///  20ms  — slow (room size, large sweeps)
#[derive(Clone, Debug)]
pub struct SmoothedParam {
    pub current: f32,
    pub target: f32,
    coeff: f32,           // per-sample smoothing coefficient
    #[allow(dead_code)]
    smooth_ms: f32,       // stored to recompute coeff if sample rate changes
}

impl SmoothedParam {
    /// Create a new smoothed parameter starting at `value` with the given
    /// smoothing time in milliseconds.
    pub fn new(value: f32, smooth_ms: f32) -> Self {
        Self {
            current: value,
            target: value,
            // Default coeff for 44100Hz — will be updated on first tick if needed
            coeff: Self::compute_coeff(smooth_ms, 44100.0),
            smooth_ms,
        }
    }

    /// Compute the per-sample coefficient from smoothing time and sample rate.
    /// Uses: coeff = 1 - e^(-2π / (smooth_time_samples))
    fn compute_coeff(smooth_ms: f32, sample_rate: f32) -> f32 {
        let samples = (smooth_ms * 0.001 * sample_rate).max(1.0);
        1.0 - (-1.0 / samples).exp()
    }

    /// Set a new target value (called from UI thread via merge_params).
    #[inline]
    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    /// Advance one sample toward target. Call once per audio sample.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.current += (self.target - self.current) * self.coeff;
        self.current
    }

    /// Snap immediately to target (no smoothing). Use on init or reset.
    #[inline]
    #[allow(dead_code)]
    pub fn snap(&mut self, value: f32) {
        self.current = value;
        self.target = value;
    }

    /// Update coefficient if sample rate changed.
    #[allow(dead_code)]
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.coeff = Self::compute_coeff(self.smooth_ms, sample_rate);
    }
}

impl Default for SmoothedParam {
    fn default() -> Self {
        Self::new(0.0, 5.0)
    }
}

// serde: serialize only the target value + smooth_ms (current state is transient)
impl serde::Serialize for SmoothedParam {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.target.serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for SmoothedParam {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f32::deserialize(deserializer)?;
        Ok(Self::new(value, 5.0))
    }
}

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
    /// Target seek position in milliseconds (atomic — no mutex needed).
    /// Stored as u64 milliseconds to avoid needing AtomicF64.
    pub seek_target_ms: AtomicUsize,
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
    /// Playback rate multiplier × 1000 (1000 = 1.0x, 500 = 0.5x, 2000 = 2.0x).
    /// Stored as integer to use AtomicU32. Set by UI, read by audio callback.
    pub playback_rate_x1000: AtomicU32,
    /// Fractional read position accumulator (only accessed by audio callback thread).
    /// Not atomic — only one consumer. Wrapped in UnsafeCell for interior mutability.
    frac_read_pos: UnsafeCell<f64>,
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
            seek_target_ms: AtomicUsize::new(0),
            stop_requested: AtomicBool::new(false),
            file_sample_rate: AtomicU32::new(44100),
            playback_position: AtomicUsize::new(0),
            decoded_position: AtomicUsize::new(0),
            total_samples: AtomicUsize::new(0),
            playback_rate_x1000: AtomicU32::new(1000),
            frac_read_pos: UnsafeCell::new(0.0),
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
    /// Supports variable playback rate via linear interpolation.
    /// Returns silence on underrun — never blocks.
    pub fn read_into(&self, buf: &mut [f32], num_frames: usize) {
        if self.paused.load(Ordering::Relaxed) {
            for i in 0..num_frames.min(buf.len()) { buf[i] = 0.0; }
            return;
        }
        let data = unsafe { &*self.data.get() };
        let rate = self.playback_rate_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
        let wp = self.write_pos.load(Ordering::Acquire);
        let rp = self.read_pos.load(Ordering::Relaxed);
        let available = wp.wrapping_sub(rp);
        let frac_pos = unsafe { &mut *self.frac_read_pos.get() };

        if (rate - 1.0).abs() < 0.001 {
            // Fast path: 1x speed, no interpolation needed
            let to_read = num_frames.min(available).min(buf.len());
            let mut rp_local = rp;
            for i in 0..to_read {
                buf[i] = data[rp_local % self.capacity];
                rp_local = rp_local.wrapping_add(1);
            }
            for i in to_read..num_frames.min(buf.len()) { buf[i] = 0.0; }
            self.read_pos.store(rp_local, Ordering::Release);
        } else {
            // Variable rate with linear interpolation
            for i in 0..num_frames.min(buf.len()) {
                let int_pos = *frac_pos as usize;
                let consumed = int_pos; // how many whole samples we've advanced past rp
                if consumed + 1 >= available {
                    buf[i] = 0.0; // underrun
                    continue;
                }
                let frac = *frac_pos - int_pos as f64;
                let idx0 = (rp + int_pos) % self.capacity;
                let idx1 = (rp + int_pos + 1) % self.capacity;
                let s0 = data[idx0];
                let s1 = data[idx1];
                buf[i] = s0 + (s1 - s0) * frac as f32; // linear interpolation

                *frac_pos += rate;
            }
            // Advance read_pos by the integer part of what we consumed
            let consumed = *frac_pos as usize;
            let new_rp = rp.wrapping_add(consumed);
            *frac_pos -= consumed as f64;
            self.read_pos.store(new_rp, Ordering::Release);
        }
    }

    /// Reset buffer positions (called during seek to flush stale samples).
    pub fn reset(&self) {
        self.write_pos.store(0, Ordering::Release);
        self.read_pos.store(0, Ordering::Release);
        self.finished.store(false, Ordering::Release);
        unsafe { *self.frac_read_pos.get() = 0.0; }
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
    /// Smoothed amplitude — eliminates zipper noise on slider drag
    pub amp_smooth: SmoothedParam,
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
            amp_smooth: SmoothedParam::new(0.5, 5.0),
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

// ── Biquad Filter (2nd-order IIR) ────────────────────────────────────────────

/// Standard biquad filter — the building block for parametric EQ.
/// Coefficients from Robert Bristow-Johnson's Audio EQ Cookbook.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BiquadFilter {
    pub b0: f32, pub b1: f32, pub b2: f32,
    pub a1: f32, pub a2: f32,
    #[serde(skip)] x1: f32,
    #[serde(skip)] x2: f32,
    #[serde(skip)] y1: f32,
    #[serde(skip)] y2: f32,
}

impl BiquadFilter {
    /// Create a peaking EQ biquad filter.
    /// freq: center frequency (Hz), gain_db: boost/cut in dB, q: bandwidth, sr: sample rate
    pub fn peaking_eq(freq: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sr;
        let sin_w0 = w0.sin();
        let cos_w0 = w0.cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// Create a low shelf biquad filter.
    pub fn low_shelf(freq: f32, gain_db: f32, sr: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sr;
        let sin_w0 = w0.sin();
        let cos_w0 = w0.cos();
        let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / 0.7 - 1.0) + 2.0).sqrt(); // S=0.7
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// Create a high shelf biquad filter.
    pub fn high_shelf(freq: f32, gain_db: f32, sr: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sr;
        let sin_w0 = w0.sin();
        let cos_w0 = w0.cos();
        let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / 0.7 - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// Bypass filter (unity pass-through).
    #[allow(dead_code)]
    pub fn bypass() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    /// Process a single sample through this biquad.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
                            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Build a bank of biquad filters from EQ curve points.
/// Points are in normalized coords: x=0..1 (mapped to 20Hz..20kHz log), y=0..1 (mapped to -24..+24 dB).
/// Samples the curve at 10 log-spaced frequency bands.
pub fn curve_to_eq_bands(points: &[[f32; 2]], sample_rate: f32) -> Vec<BiquadFilter> {
    // 10 frequency bands on log scale (Hz)
    let freqs: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
    let q = 1.4; // moderate bandwidth

    let mut bands = Vec::new();
    for (i, &freq) in freqs.iter().enumerate() {
        // Convert frequency to normalized x (0-1, log scale)
        let x = (freq.ln() - 20.0_f32.ln()) / (20000.0_f32.ln() - 20.0_f32.ln());
        // Evaluate the curve at this x position
        let y = evaluate_curve_at(points, x);
        // Convert y (0-1) to gain in dB (-24 to +24)
        let gain_db = (y - 0.5) * 48.0; // 0.5 = 0dB, 0.0 = -24dB, 1.0 = +24dB

        if gain_db.abs() > 0.5 {
            let filter = if i == 0 {
                BiquadFilter::low_shelf(freq, gain_db, sample_rate)
            } else if i == freqs.len() - 1 {
                BiquadFilter::high_shelf(freq, gain_db, sample_rate)
            } else {
                BiquadFilter::peaking_eq(freq, gain_db, q, sample_rate)
            };
            bands.push(filter);
        }
    }
    bands
}

/// Simple cubic Hermite interpolation of a curve defined by sorted [x,y] points.
fn evaluate_curve_at(points: &[[f32; 2]], x: f32) -> f32 {
    if points.is_empty() { return 0.5; } // flat (0dB)
    if points.len() == 1 { return points[0][1]; }
    if x <= points[0][0] { return points[0][1]; }
    if x >= points.last().unwrap()[0] { return points.last().unwrap()[1]; }

    // Find the segment
    for i in 0..points.len() - 1 {
        let x0 = points[i][0];
        let x1 = points[i + 1][0];
        if x >= x0 && x <= x1 {
            let t = if (x1 - x0).abs() < 1e-6 { 0.0 } else { (x - x0) / (x1 - x0) };
            let y0 = points[i][1];
            let y1 = points[i + 1][1];
            // Cubic Hermite smooth interpolation
            let t2 = t * t;
            let t3 = t2 * t;
            let h = 2.0 * t3 - 3.0 * t2 + 1.0;
            return y0 * h + y1 * (1.0 - h);
        }
    }
    0.5
}

// ── Audio Effects ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AudioEffect {
    Gain { level: SmoothedParam },                         // 0..2, 1.0 = unity
    LowPass { cutoff: SmoothedParam, state: f32 },         // Simple 1-pole
    HighPass { cutoff: SmoothedParam, state: f32 },
    Delay {
        time_ms: f32,
        feedback: SmoothedParam,
        buffer: Vec<f32>,
        write_pos: usize,
        /// Max buffer capacity in samples. Once allocated, never resized — only
        /// the read offset changes when delay time is adjusted. This avoids
        /// allocation on the audio thread and the click from buffer zeroing.
        max_delay_samples: usize,
    },
    Distortion { drive: SmoothedParam },                   // 1..20
    /// Schroeder reverb — 4 comb filters + 2 allpass filters.
    Reverb {
        room_size: SmoothedParam,
        damping: SmoothedParam,
        mix: SmoothedParam,
        comb_buffers: [Vec<f32>; 4],
        comb_pos: [usize; 4],
        comb_filter_state: [f32; 4],
        allpass_buffers: [Vec<f32>; 2],
        allpass_pos: [usize; 2],
        initialized: bool,
    },
    /// Parametric EQ — bank of biquad filters derived from curve points.
    ParametricEq { bands: Vec<BiquadFilter>, curve_hash: u64 },
}

impl AudioEffect {
    pub fn name(&self) -> &'static str {
        match self {
            AudioEffect::Gain { .. } => "Gain",
            AudioEffect::LowPass { .. } => "Low Pass",
            AudioEffect::HighPass { .. } => "High Pass",
            AudioEffect::Delay { .. } => "Delay",
            AudioEffect::Distortion { .. } => "Distortion",
            AudioEffect::Reverb { .. } => "Reverb",
            AudioEffect::ParametricEq { .. } => "Parametric EQ",
        }
    }

    /// Update user-controlled params from another effect, preserving processing state.
    /// SmoothedParams: sets the TARGET (smoothed per-sample in process()).
    pub fn merge_params(&mut self, other: &AudioEffect) {
        match (self, other) {
            (AudioEffect::Gain { level }, AudioEffect::Gain { level: new_level }) => {
                level.set(new_level.target);
            }
            (AudioEffect::LowPass { cutoff, .. }, AudioEffect::LowPass { cutoff: new_cutoff, .. }) => {
                cutoff.set(new_cutoff.target);
                // filter state preserved!
            }
            (AudioEffect::HighPass { cutoff, .. }, AudioEffect::HighPass { cutoff: new_cutoff, .. }) => {
                cutoff.set(new_cutoff.target);
            }
            (AudioEffect::Delay { time_ms, feedback, .. }, AudioEffect::Delay { time_ms: new_time, feedback: new_fb, .. }) => {
                *time_ms = *new_time; // time changes read offset, no buffer resize
                feedback.set(new_fb.target);
                // buffer, write_pos, max_delay_samples all preserved
            }
            (AudioEffect::Distortion { drive }, AudioEffect::Distortion { drive: new_drive }) => {
                drive.set(new_drive.target);
            }
            (AudioEffect::Reverb { room_size, damping, mix, .. },
             AudioEffect::Reverb { room_size: new_room, damping: new_damp, mix: new_mix, .. }) => {
                room_size.set(new_room.target);
                damping.set(new_damp.target);
                mix.set(new_mix.target);
                // Buffers and positions preserved
            }
            (AudioEffect::ParametricEq { bands, curve_hash },
             AudioEffect::ParametricEq { bands: new_bands, curve_hash: new_hash }) => {
                if *curve_hash != *new_hash {
                    // Curve shape changed — replace filter bank (preserving state where possible)
                    *bands = new_bands.clone();
                    *curve_hash = *new_hash;
                }
                // If hash unchanged, keep existing bands with their filter state (no clicks)
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
            AudioEffect::Reverb { .. } => 5,
            AudioEffect::ParametricEq { .. } => 6,
        }
    }

    /// Process a single sample through this effect.
    /// SmoothedParams are ticked per-sample for glitch-free parameter changes.
    pub fn process(&mut self, sample: f32, sample_rate: f32) -> f32 {
        match self {
            AudioEffect::Gain { level } => sample * level.tick(),
            AudioEffect::LowPass { cutoff, state } => {
                let c = cutoff.tick().max(20.0);
                let rc = 1.0 / (std::f32::consts::TAU * c);
                let dt = 1.0 / sample_rate;
                let alpha = dt / (rc + dt);
                *state = *state + alpha * (sample - *state);
                *state
            }
            AudioEffect::HighPass { cutoff, state } => {
                let c = cutoff.tick().max(20.0);
                let rc = 1.0 / (std::f32::consts::TAU * c);
                let dt = 1.0 / sample_rate;
                let alpha = rc / (rc + dt);
                let out = alpha * (*state + sample - *state);
                *state = sample;
                out
            }
            AudioEffect::Delay { time_ms, feedback, buffer, write_pos, max_delay_samples } => {
                // Pre-allocate buffer to max size on first use (2 seconds max).
                // Never resize — only the read offset changes when delay time is tweaked.
                let max_samples = if *max_delay_samples == 0 {
                    let ms = (2000.0 * sample_rate / 1000.0) as usize; // 2 second max
                    *max_delay_samples = ms;
                    ms
                } else {
                    *max_delay_samples
                };
                if buffer.len() != max_samples {
                    buffer.resize(max_samples, 0.0);
                    *write_pos = 0;
                }

                // Read from a variable offset behind write_pos (no buffer resize needed)
                let delay_samples = ((*time_ms * sample_rate / 1000.0) as usize)
                    .clamp(1, max_samples - 1);
                let read_pos = if *write_pos >= delay_samples {
                    *write_pos - delay_samples
                } else {
                    max_samples - (delay_samples - *write_pos)
                };

                let delayed = buffer[read_pos];
                let fb = feedback.tick();
                let output = sample + delayed * fb;
                buffer[*write_pos] = output;
                *write_pos = (*write_pos + 1) % max_samples;
                output
            }
            AudioEffect::Distortion { drive } => {
                let driven = sample * drive.tick();
                driven.tanh()
            }
            AudioEffect::Reverb { room_size, damping, mix, comb_buffers, comb_pos, comb_filter_state, allpass_buffers, allpass_pos, initialized } => {
                // Initialize buffers on first use (tuned for 44.1kHz, scale for other rates)
                if !*initialized {
                    let sr_scale = (sample_rate / 44100.0).max(0.5);
                    // Comb filter delay lengths (in samples) — prime-ish values to avoid resonance
                    let comb_lengths: [usize; 4] = [
                        (1116.0 * sr_scale) as usize,
                        (1188.0 * sr_scale) as usize,
                        (1277.0 * sr_scale) as usize,
                        (1356.0 * sr_scale) as usize,
                    ];
                    // Allpass delay lengths
                    let allpass_lengths: [usize; 2] = [
                        (556.0 * sr_scale) as usize,
                        (441.0 * sr_scale) as usize,
                    ];
                    for (i, &len) in comb_lengths.iter().enumerate() {
                        comb_buffers[i] = vec![0.0; len.max(1)];
                        comb_pos[i] = 0;
                        comb_filter_state[i] = 0.0;
                    }
                    for (i, &len) in allpass_lengths.iter().enumerate() {
                        allpass_buffers[i] = vec![0.0; len.max(1)];
                        allpass_pos[i] = 0;
                    }
                    *initialized = true;
                }

                let rs = room_size.tick().clamp(0.0, 1.0);
                let feedback = rs * 0.28 + 0.7; // 0.7 .. 0.98
                let damp = damping.tick().clamp(0.0, 1.0);
                let damp1 = damp;
                let damp2 = 1.0 - damp;

                // Parallel comb filters
                let mut comb_out = 0.0f32;
                for i in 0..4 {
                    let buf = &mut comb_buffers[i];
                    let pos = &mut comb_pos[i];
                    let filt = &mut comb_filter_state[i];
                    if buf.is_empty() { continue; }

                    let delayed = buf[*pos];
                    // Low-pass comb feedback (damping)
                    *filt = delayed * damp2 + *filt * damp1;
                    buf[*pos] = sample + *filt * feedback;
                    *pos = (*pos + 1) % buf.len();
                    comb_out += delayed;
                }
                comb_out *= 0.25; // average the 4 combs

                // Series allpass filters (diffusion)
                let mut out = comb_out;
                for i in 0..2 {
                    let buf = &mut allpass_buffers[i];
                    let pos = &mut allpass_pos[i];
                    if buf.is_empty() { continue; }

                    let delayed = buf[*pos];
                    let ap_out = -out + delayed;
                    buf[*pos] = out + delayed * 0.5;
                    *pos = (*pos + 1) % buf.len();
                    out = ap_out;
                }

                // Wet/dry mix (smoothed)
                let wet = mix.tick().clamp(0.0, 1.0);
                sample * (1.0 - wet) + out * wet
            }
            AudioEffect::ParametricEq { bands, .. } => {
                let mut s = sample;
                for band in bands.iter_mut() {
                    s = band.process(s);
                }
                s
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
            callback_duration_us: 0.0,
            callback_budget_us: 0.0,
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
                let target = buffer.seek_target_ms.load(Ordering::Relaxed) as f64 / 1000.0;
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

            // Backpressure: if the ring buffer is nearly full, the audio callback
            // hasn't consumed yet.  Just wait briefly — never discard samples, as that
            // creates audible gaps ("plays, stops, plays, stops" stuttering).
            let buffered = buffer.buffered();
            if buffered > buffer.capacity * 3 / 4 {
                std::thread::sleep(std::time::Duration::from_millis(5));
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
    /// Dropout counter — lives outside the Mutex so the audio callback can
    /// increment it even when try_lock fails (which is the whole point).
    pub dropout_count: Arc<AtomicU32>,
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
            dropout_count: Arc::new(AtomicU32::new(0)),
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
            dropout_count: Arc::new(AtomicU32::new(0)),
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
            if let Ok(mut s) = self.state.try_lock() {
                s.sample_rate = sample_rate;
            }
        }

        let state = self.state.clone();
        let dropouts = self.dropout_count.clone();

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(data, channels, &state, &dropouts);
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
        if let Ok(mut s) = self.state.try_lock() {
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
            // Preserve running state from the audio thread (phase + smoother current value)
            let (existing_phase, existing_amp_smooth) = match s.sources.get(&node_id) {
                Some(AudioSource::Synth(existing)) => (existing.phase, existing.amp_smooth.current),
                _ => (0.0, params.amplitude),
            };
            let mut amp_smooth = SmoothedParam::new(params.amplitude, 5.0);
            amp_smooth.current = existing_amp_smooth; // preserve current, set new target
            s.sources.insert(node_id, AudioSource::Synth(SynthParams {
                phase: existing_phase,
                amp_smooth,
                ..params
            }));
        }
        // If lock fails, skip this frame's update (audio thread is busy — no clicking)
    }

    /// Remove a source
    pub fn remove_source(&self, node_id: NodeId) {
        if let Ok(mut s) = self.state.try_lock() {
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
        let output_sr = self.state.try_lock()
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
        if let Ok(mut s) = self.state.try_lock() {
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
            buf.seek_target_ms.store((position_secs * 1000.0) as usize, Ordering::Release);
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
            let sr = self.state.try_lock().map(|s| s.sample_rate).unwrap_or(44100.0) as f64;
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

    /// Set playback speed (turntable style — affects both tempo and pitch).
    /// 1.0 = normal, 0.5 = half speed (lower pitch), 2.0 = double speed (higher pitch).
    pub fn set_file_speed(&self, node_id: NodeId, speed: f32) {
        if let Some(buf) = self.file_buffers.get(&node_id) {
            let rate = (speed.clamp(0.1, 4.0) * 1000.0) as u32;
            buf.playback_rate_x1000.store(rate, Ordering::Relaxed);
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
        if let Ok(mut s) = self.state.try_lock() {
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
    // Sync the smoother target with the UI-set amplitude
    params.amp_smooth.set(if params.active { params.amplitude } else { 0.0 });

    // Don't early-return if amplitude is low — the smoother needs to ramp down
    // to avoid clicks when gate closes
    if !params.active && params.amp_smooth.current < 0.0001 {
        return buf;
    }

    for frame in 0..num_frames {
        // FM: offset frequency by modulator's sample × depth
        let freq = params.frequency + match fm_buf {
            Some(mod_samples) => mod_samples[frame] * params.fm_depth,
            None => 0.0,
        };

        let amp = params.amp_smooth.tick();
        buf[frame] = params.waveform.sample(params.phase) * amp;

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
    dropout_counter: &AtomicU32,
) {
    // Zero the buffer
    for sample in data.iter_mut() {
        *sample = 0.0;
    }

    // Use try_lock to avoid blocking the audio thread — if UI holds the lock, skip this buffer
    let mut s = match state.try_lock() {
        Ok(s) => s,
        Err(_) => {
            // Atomic increment — works even when the mutex is held (that's the whole point)
            dropout_counter.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let callback_start = std::time::Instant::now();
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

    // Record callback performance metrics
    let elapsed = callback_start.elapsed();
    s.callback_duration_us = elapsed.as_micros() as f32;
    s.callback_budget_us = num_frames as f32 / sample_rate * 1_000_000.0;
}
