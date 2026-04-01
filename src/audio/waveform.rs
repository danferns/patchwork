use super::smoothed::SmoothedParam;
use crate::graph::NodeId;

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

