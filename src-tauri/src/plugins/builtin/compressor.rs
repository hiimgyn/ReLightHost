/// Feed-forward stereo compressor with soft knee and parallel mix.
///
/// # Parameters (set via `set_parameter`)
/// | ID | Name        | Range            | Default | Unit |
/// |----|-------------|------------------|---------|------|
/// | 0  | Threshold   | -60.0 – 0.0      | -18.0   | dB   |
/// | 1  | Ratio       | 1.0  – 20.0      |   4.0   | :1   |
/// | 2  | Attack      | 0.1  – 200.0     |  10.0   | ms   |
/// | 3  | Release     | 10.0 – 2000.0    | 100.0   | ms   |
/// | 4  | Makeup Gain | 0.0  – 30.0      |   0.0   | dB   |
/// | 5  | Knee        | 0.0  – 12.0      |   3.0   | dB   |
/// | 6  | Mix         | 0.0  – 1.0       |   1.0   | ratio|
///
/// Topology: feed-forward peak detector → soft-knee gain computer →
/// asymmetric envelope follower → makeup gain → parallel wet/dry blend.

use super::BuiltinProcessor;

pub const ID: &str = "builtin::compressor";

pub struct Compressor {
    // Parameters
    threshold_db: f32,
    ratio:        f32,
    attack_ms:    f32,
    release_ms:   f32,
    makeup_db:    f32,
    knee_db:      f32,
    mix:          f32,

    // State
    sample_rate: f32,
    envelope:    f32, // current peak envelope (linear)
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            threshold_db: -18.0,
            ratio:          4.0,
            attack_ms:     10.0,
            release_ms:   100.0,
            makeup_db:      0.0,
            knee_db:        3.0,
            mix:            1.0,
            sample_rate: sample_rate.max(1.0),
            envelope:       0.0,
        }
    }

    /// Gain reduction in dB for `level_db`, accounting for soft knee.
    /// Returns a value ≤ 0 (gain reduction) or 0 (no reduction below threshold).
    fn gain_reduction_db(&self, level_db: f32) -> f32 {
        let slope     = 1.0 / self.ratio - 1.0; // ≤ 0
        let half_knee = self.knee_db / 2.0;
        let t_lo      = self.threshold_db - half_knee;
        let t_hi      = self.threshold_db + half_knee;

        if level_db <= t_lo {
            0.0 // below knee — no reduction
        } else if self.knee_db <= 0.0 || level_db >= t_hi {
            slope * (level_db - self.threshold_db) // hard knee / above knee
        } else {
            // Soft knee interpolation
            // x = 0 at lower edge, 1 at upper edge → quadratic blend
            let x = (level_db - t_lo) / self.knee_db;
            slope * self.knee_db * x * x / 2.0
        }
    }
}

impl BuiltinProcessor for Compressor {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let sr = self.sample_rate;

        // Pre-compute time-constant coefficients; these are cheap and avoid
        // caching issues if sample_rate were ever updated later.
        let attack_coeff = if self.attack_ms < 0.001 {
            0.0_f32 // near-instantaneous attack
        } else {
            (-1.0_f32 / (self.attack_ms * 0.001 * sr)).exp()
        };
        let release_coeff = (-1.0_f32 / (self.release_ms * 0.001 * sr)).exp();
        let makeup        = 10f32.powf(self.makeup_db / 20.0);
        let mix           = self.mix;

        for i in 0..left.len() {
            let peak = left[i].abs().max(right[i].abs());

            // Asymmetric envelope follower: fast attack, slow release.
            self.envelope = if peak > self.envelope {
                attack_coeff  * self.envelope + (1.0 - attack_coeff)  * peak
            } else {
                release_coeff * self.envelope + (1.0 - release_coeff) * peak
            };
            // Clamp to avoid log(0) or runaway on DC offsets.
            self.envelope = self.envelope.clamp(0.0, 10.0);

            // Convert envelope to dB; use a floor of -140 dBFS for silence.
            let env_db = if self.envelope > 1e-7 {
                20.0 * self.envelope.log10()
            } else {
                -140.0
            };

            // Gain = reduction × makeup (always ≥ 0 because makeup compensates).
            let gain = 10f32.powf(self.gain_reduction_db(env_db) / 20.0) * makeup;

            // Parallel compression: dry + (compressed − dry) × mix
            let factor = 1.0 - mix + gain * mix;
            left[i]  *= factor;
            right[i] *= factor;
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        match id {
            0 => self.threshold_db = value,
            1 => self.ratio        = value.max(1.0),
            2 => self.attack_ms    = value.max(0.0),
            3 => self.release_ms   = value.max(1.0),
            4 => self.makeup_db    = value,
            5 => self.knee_db      = value.max(0.0),
            6 => self.mix          = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_vad(&self) -> f32 { 0.0 }
}
