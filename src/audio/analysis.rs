/// Real-time audio analysis — computed per audio block.
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
    mid_state: f32,
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
    /// Update analysis from an audio buffer. Uses one-pole band-split
    /// filters and exponential smoothing for stable, reactive values.
    pub fn update(&mut self, data: &[f32], channels: usize, sample_rate: f32) {
        let num_frames = data.len() / channels;
        if num_frames == 0 { return; }

        let mut sum_sq = 0.0f32;
        let mut peak = 0.0f32;
        let mut bass_energy = 0.0f32;
        let mut mid_energy = 0.0f32;
        let mut treble_energy = 0.0f32;

        let bass_coeff = (std::f32::consts::TAU * 300.0 / sample_rate).min(1.0);
        let treble_coeff = (std::f32::consts::TAU * 2000.0 / sample_rate).min(1.0);

        for frame in 0..num_frames {
            let mut sample = 0.0f32;
            for ch in 0..channels {
                sample += data[frame * channels + ch];
            }
            sample /= channels as f32;

            sum_sq += sample * sample;
            peak = peak.max(sample.abs());

            self.bass_state += bass_coeff * (sample - self.bass_state);
            let bass_sample = self.bass_state;

            self.treble_state += treble_coeff * (sample - self.treble_state);
            let treble_sample = sample - self.treble_state;
            let mid_sample = self.treble_state - self.bass_state;

            bass_energy += bass_sample * bass_sample;
            mid_energy += mid_sample * mid_sample;
            treble_energy += treble_sample * treble_sample;
        }

        let rms = (sum_sq / num_frames as f32).sqrt();
        let bass_rms = (bass_energy / num_frames as f32).sqrt();
        let mid_rms = (mid_energy / num_frames as f32).sqrt();
        let treble_rms = (treble_energy / num_frames as f32).sqrt();

        let attack = 0.6;
        let decay = 0.05;
        let smooth = |old: f32, new: f32| {
            if new > old { old + attack * (new - old) }
            else { old + decay * (new - old) }
        };

        self.amplitude = smooth(self.amplitude, rms.min(1.0));
        self.peak = smooth(self.peak, peak.min(1.0));
        self.bass = smooth(self.bass, (bass_rms * 3.0).min(1.0));
        self.mid = smooth(self.mid, (mid_rms * 4.0).min(1.0));
        self.treble = smooth(self.treble, (treble_rms * 5.0).min(1.0));
    }
}
