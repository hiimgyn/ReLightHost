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
use relight_addon_api::{AddonDescriptor, AddonProcessor, ParamSpec, Scale};

const fn p(id: u32, name: &'static str, min: f32, max: f32, default: f32, unit: &'static str, scale: Scale) -> ParamSpec {
    ParamSpec { id, name, min, max, default, unit, scale, step: 0.0, choices: &[] }
}

pub const ID: &str = "builtin::compressor";

static ADDON_PARAMS: &[ParamSpec] = &[
    p(0, "Threshold", -60.0, 0.0, -18.0, "dB", Scale::Db),
    p(1, "Ratio", 1.0, 20.0, 4.0, ":1", Scale::Log),
    p(2, "Attack", 0.1, 200.0, 10.0, "ms", Scale::Log),
    p(3, "Release", 10.0, 2000.0, 100.0, "ms", Scale::Log),
    p(4, "Makeup Gain", 0.0, 30.0, 0.0, "dB", Scale::Db),
    p(5, "Knee", 0.0, 12.0, 3.0, "dB", Scale::Db),
    p(6, "Mix", 0.0, 1.0, 1.0, "", Scale::Linear),
];
static ADDON_METERS: &[&str] = &["in_db", "gr_db", "out_db"];

pub static ADDON: AddonDescriptor = AddonDescriptor {
    id: ID,
    name: "Compressor",
    category: "Dynamics",
    version: 1,
    params: ADDON_PARAMS,
    meters: ADDON_METERS,
    sample_rates: &[],
    create: |sr| Some(Box::new(Compressor::new(sr))),
    migrate: None,
};
const _: () = assert!(relight_addon_api::check_params(ADDON_PARAMS, ADDON_METERS));

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
    makeup_s:    f32, // smoothed linear makeup
    mix_s:       f32, // smoothed mix
    started:     bool, // audio processed yet? before that, set_parameter snaps the smoothers
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
            makeup_s:       1.0,
            mix_s:          1.0,
            started:      false,
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

impl AddonProcessor for Compressor {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], meters: &mut [f32]) {
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
        // 10 ms one-pole on makeup and mix only; the gain-reduction path stays unsmoothed.
        let sm = (-1.0_f32 / (0.010 * sr)).exp();
        let follow = |cur: f32, target: f32| {
            let v = target + (cur - target) * sm;
            // `v == cur`: f32 stalled short of target.
            if (v - target).abs() < 1e-6 || v == cur { target } else { v }
        };
        let (mut in_pk, mut out_pk, mut min_red) = (0.0_f32, 0.0_f32, 1.0_f32);

        for i in 0..left.len() {
            self.makeup_s = follow(self.makeup_s, makeup);
            self.mix_s = follow(self.mix_s, mix);
            let (makeup, mix) = (self.makeup_s, self.mix_s);
            let peak = left[i].abs().max(right[i].abs());

            // Asymmetric envelope follower: fast attack, slow release.
            self.envelope = if peak > self.envelope {
                attack_coeff  * self.envelope + (1.0 - attack_coeff)  * peak
            } else {
                release_coeff * self.envelope + (1.0 - release_coeff) * peak
            };
            // Clamp to avoid log(0) or runaway on DC offsets.
            // A NaN input sample would otherwise latch the envelope at NaN forever.
            self.envelope = if self.envelope.is_finite() { self.envelope.clamp(0.0, 10.0) } else { 0.0 };

            // Convert envelope to dB; use a floor of -140 dBFS for silence.
            let env_db = if self.envelope > 1e-7 {
                20.0 * self.envelope.log10()
            } else {
                -140.0
            };

            // Gain = reduction × makeup (always ≥ 0 because makeup compensates).
            let red = 10f32.powf(self.gain_reduction_db(env_db) / 20.0);
            let gain = red * makeup;
            min_red = min_red.min(1.0 - mix + red * mix); // applied reduction, without makeup

            // Parallel compression: dry + (compressed − dry) × mix
            let factor = 1.0 - mix + gain * mix;
            in_pk = in_pk.max(peak);
            left[i]  *= factor;
            right[i] *= factor;
            out_pk = out_pk.max(left[i].abs().max(right[i].abs()));
        }
        self.started |= !left.is_empty();
        if let [i, g, o, ..] = meters {
            let db = |x: f32| 20.0 * x.max(1e-7).log10();
            *i = db(in_pk);
            *g = -db(min_red);
            *o = db(out_pk);
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        match id {
            0 => self.threshold_db = value,
            1 => self.ratio        = value.max(1.0),
            2 => self.attack_ms    = value.max(0.0),
            3 => self.release_ms   = value.max(1.0),
            4 => {
                self.makeup_db = value;
                if !self.started { self.makeup_s = 10f32.powf(value / 20.0); }
            }
            5 => self.knee_db      = value.max(0.0),
            6 => {
                self.mix = value.clamp(0.0, 1.0);
                if !self.started { self.mix_s = self.mix; }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (std::f32::consts::TAU * 1000.0 * i as f32 / SR).sin()).collect()
    }

    #[test]
    fn gr_meter_reports_applied_reduction() {
        let mut c = Compressor::new(SR);
        let amp = 10f32.powf(-6.0 / 20.0);
        let (mut l, mut r) = (sine(48_000, amp), sine(48_000, amp));
        let mut m = [0.0f32; 3];
        for (cl, cr) in l.chunks_mut(480).zip(r.chunks_mut(480)) {
            c.process(cl, cr, &mut m);
        }
        assert!((m[1] - 8.13).abs() < 0.1, "gr {}", m[1]);
        assert!((m[2] - (m[0] - m[1])).abs() < 0.1, "in {} gr {} out {}", m[0], m[1], m[2]);
    }

    #[test]
    fn mix_zero_is_identity() {
        let mut c = Compressor::new(SR);
        c.set_parameter(6, 0.0);
        let src = sine(4800, 0.9);
        let (mut l, mut r) = (src.clone(), src.clone());
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        assert_eq!(&l[4800 - 480..], &src[4800 - 480..]);
    }

    #[test]
    fn ratio_one_is_makeup_only() {
        let mut c = Compressor::new(SR);
        c.set_parameter(1, 1.0);
        c.set_parameter(4, 12.0);
        let (mut l, mut r) = (vec![0.5f32; 4800], vec![0.5f32; 4800]);
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        let g = l[4799] / 0.5;
        assert!((g - 10f32.powf(0.6)).abs() < 1e-5, "gain {g}");
    }

    #[test]
    fn nan_resets_envelope() {
        let mut c = Compressor::new(SR);
        let (mut l, mut r) = (vec![0.9f32; 4800], vec![0.9f32; 4800]);
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        let (mut l, mut r) = (vec![f32::NAN; 8], vec![f32::NAN; 8]);
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        assert_eq!(c.envelope, 0.0);
        let (mut l, mut r) = (vec![0.001f32; 16], vec![0.001f32; 16]);
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        assert!(l.iter().all(|v| (v - 0.001).abs() < 1e-6));
    }

    #[test]
    fn smoothers_reach_target_exactly() {
        let mut c = Compressor::new(SR);
        c.process(&mut [0.0; 480], &mut [0.0; 480], &mut [0.0; 3]); // started: params now glide
        c.set_parameter(4, 30.0);
        c.set_parameter(6, 0.3);
        let (mut l, mut r) = (sine(24_000, 0.1), sine(24_000, 0.1));
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        assert_eq!(c.makeup_s, 10f32.powf(30.0 / 20.0));
        assert_eq!(c.mix_s, 0.3);
    }

    #[test]
    fn makeup_jump_is_click_free() {
        let mut c = Compressor::new(SR);
        c.set_parameter(4, 30.0);
        let (mut l, mut r) = (vec![0.01f32; 4800], vec![0.01f32; 4800]);
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        let mut y = vec![l[4799]];
        c.set_parameter(4, 0.0);
        let (mut l, mut r) = (vec![0.01f32; 4800], vec![0.01f32; 4800]);
        c.process(&mut l, &mut r, &mut [0.0; 3]);
        y.extend(&l);
        let max = y.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(max < 1e-3, "max step {max}");
    }
}
