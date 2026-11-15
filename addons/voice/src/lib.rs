/// Voice Designer — four-stage built-in voice processor.
///
/// Signal chain: **3-Band EQ → Saturation → Doubler → Limiter**
///
/// # Parameters
/// | ID | Name     | Range        | Default | Stage       |
/// |----|----------|--------------|---------|-------------|
/// | 0  | Low      | −12 … +12 dB |   0.0   | EQ 200 Hz   |
/// | 1  | Mid      | −12 … +12 dB |   0.0   | EQ 2 kHz    |
/// | 2  | High     | −12 … +12 dB |   0.0   | EQ 8 kHz    |
/// | 3  | Drive    |  0.0 … 1.0   |   0.0   | Saturation  |
/// | 4  | Width    |  0.0 … 1.0   |   0.0   | Doubler     |
/// | 5  | Ceiling  | −12 … 0 dB   |   0.0   | Limiter     |
use std::f64::consts::PI;
use relight_addon_api::{AddonDescriptor, AddonProcessor, ParamSpec, Scale};

const fn p(id: u32, name: &'static str, min: f32, max: f32, default: f32, unit: &'static str, scale: Scale) -> ParamSpec {
    ParamSpec { id, name, min, max, default, unit, scale, step: 0.0, choices: &[] }
}

pub const ID: &str = "builtin::voice";

static ADDON_PARAMS: &[ParamSpec] = &[
    p(0, "Low", -12.0, 12.0, 0.0, "dB", Scale::Db),
    p(1, "Mid", -12.0, 12.0, 0.0, "dB", Scale::Db),
    p(2, "High", -12.0, 12.0, 0.0, "dB", Scale::Db),
    p(3, "Drive", 0.0, 1.0, 0.0, "", Scale::Linear),
    p(4, "Width", 0.0, 1.0, 0.0, "", Scale::Linear),
    p(5, "Ceiling", -12.0, 0.0, 0.0, "dB", Scale::Db),
];
static ADDON_METERS: &[&str] = &["limiter_gr_db"];

pub static ADDON: AddonDescriptor = AddonDescriptor {
    id: ID,
    name: "Voice Designer",
    category: "Voice",
    version: 1,
    params: ADDON_PARAMS,
    meters: ADDON_METERS,
    sample_rates: &[],
    create: |sr| Some(Box::new(Voice::new(sr))),
    migrate: None,
};
const _: () = assert!(relight_addon_api::check_params(ADDON_PARAMS, ADDON_METERS));

const P_LOW:     u32 = 0;
const P_MID:     u32 = 1;
const P_HIGH:    u32 = 2;
const P_DRIVE:   u32 = 3;
const P_WIDTH:   u32 = 4;
const P_CEILING: u32 = 5;

// ── Biquad filter (Transposed Direct Form II) ─────────────────────────────────

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64, b1: f64, b2: f64,
    a1: f64, a2: f64,
    s1: f64, s2: f64,
}

impl Biquad {
    fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, s1: 0.0, s2: 0.0 }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let xi = x as f64;
        let y  = self.b0 * xi + self.s1;
        self.s1 = self.b1 * xi - self.a1 * y + self.s2;
        self.s2 = self.b2 * xi - self.a2 * y;
        y as f32
    }

    // No 0 dB shortcut: at A = 1 the formulas below are already pass-through,
    // and snapping to identity coefficients while the state is loaded clicks.

    /// Audio EQ Cookbook — Low shelf, shelf slope S=1.
    fn set_low_shelf(&mut self, gain_db: f64, freq: f64, sr: f64) {
        let a    = 10_f64.powf(gain_db / 40.0);
        let w0   = 2.0 * PI * freq / sr;
        let cos  = w0.cos();
        let alph = w0.sin() / 2.0 * 2_f64.sqrt(); // S=1 → alpha = sin/2*sqrt(2)
        let sq   = 2.0 * a.sqrt() * alph;

        let b0 =  a * ((a + 1.0) - (a - 1.0) * cos + sq);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos);
        let b2 =  a * ((a + 1.0) - (a - 1.0) * cos - sq);
        let a0 =        (a + 1.0) + (a - 1.0) * cos + sq;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos);
        let a2 =        (a + 1.0) + (a - 1.0) * cos - sq;

        self.b0 = b0/a0; self.b1 = b1/a0; self.b2 = b2/a0;
        self.a1 = a1/a0; self.a2 = a2/a0;
    }

    /// Audio EQ Cookbook — High shelf, shelf slope S=1.
    fn set_high_shelf(&mut self, gain_db: f64, freq: f64, sr: f64) {
        let a    = 10_f64.powf(gain_db / 40.0);
        let w0   = 2.0 * PI * freq / sr;
        let cos  = w0.cos();
        let alph = w0.sin() / 2.0 * 2_f64.sqrt();
        let sq   = 2.0 * a.sqrt() * alph;

        let b0 =  a * ((a + 1.0) + (a - 1.0) * cos + sq);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos);
        let b2 =  a * ((a + 1.0) + (a - 1.0) * cos - sq);
        let a0 =        (a + 1.0) - (a - 1.0) * cos + sq;
        let a1 =  2.0 * ((a - 1.0) - (a + 1.0) * cos);
        let a2 =        (a + 1.0) - (a - 1.0) * cos - sq;

        self.b0 = b0/a0; self.b1 = b1/a0; self.b2 = b2/a0;
        self.a1 = a1/a0; self.a2 = a2/a0;
    }

    /// Audio EQ Cookbook — Peaking EQ.
    fn set_peak(&mut self, gain_db: f64, freq: f64, q: f64, sr: f64) {
        let a    = 10_f64.powf(gain_db / 40.0);
        let w0   = 2.0 * PI * freq / sr;
        let cos  = w0.cos();
        let alph = w0.sin() / (2.0 * q);

        let b0 = 1.0 + alph * a;
        let b1 = -2.0 * cos;
        let b2 = 1.0 - alph * a;
        let a0 = 1.0 + alph / a;
        let a2 = 1.0 - alph / a;

        self.b0 = b0/a0; self.b1 = b1/a0; self.b2 = b2/a0;
        self.a1 = b1/a0; self.a2 = a2/a0;
    }
}

// ── 3-band EQ per channel ─────────────────────────────────────────────────────

/// Low shelf 200 Hz, peak 2 kHz (Q 0.7), high shelf 8 kHz.
struct EqChannel {
    bands: [Biquad; 3],
}

impl EqChannel {
    fn new() -> Self {
        Self { bands: [Biquad::identity(); 3] }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let [low, mid, high] = &mut self.bands;
        high.process(mid.process(low.process(x)))
    }
}

// ── Soft-clip saturation ──────────────────────────────────────────────────────
// Parallel blend of dry and tanh-saturated wet signal.
// drive=0 → bypass, drive=1 → fully wet, pre-gain 2.2×.

#[inline]
fn saturate(x: f32, drive: f32) -> f32 {
    if drive < 0.001 { return x; }
    let pre = 1.0 + drive * 1.2;           // pre-gain 1x … 2.2x
    // ÷pre, not ÷tanh(pre): unity small-signal gain, so Drive changes how
    // hard peaks saturate rather than boosting quiet speech. At full drive a
    // −6 dBFS sine loses ~2.8 dB of peak (THD ~8 %), not 12 dB.
    let wet = (x * pre).tanh() / pre;
    x * (1.0 - drive) + wet * drive        // parallel blend
}

// ── Mono-safe widener ─────────────────────────────────────────────────────────
// L = L + w·D, R = R − w·D with D = mid delayed ~12 ms: width 0 is an exact
// identity and the mono sum L+R never changes (no comb filter on mono calls).

const DELAY_BUF: usize = 4096; // power-of-2 for masking

struct Doubler {
    buf:   Vec<f32>,
    write: usize,
    delay: usize,
}

impl Doubler {
    fn new(sample_rate: f32) -> Self {
        let delay = ((sample_rate * 0.012) as usize).clamp(1, DELAY_BUF - 1);
        Self { buf: vec![0.0f32; DELAY_BUF], write: 0, delay }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, width: f32) -> (f32, f32) {
        // Fed even at width 0, so raising Width never replays stale audio.
        self.buf[self.write] = (l + r) * 0.5;
        self.write = (self.write + 1) & (DELAY_BUF - 1);
        let d = width * self.buf[(self.write + DELAY_BUF - self.delay) & (DELAY_BUF - 1)];
        (l + d, r - d)
    }
}

// ── Peak limiter ─────────────────────────────────────────────────────────────
// Instantaneous attack, smooth release (~50 ms).

struct Limiter {
    gain:    f32,
    rel_coeff: f32,
}

impl Limiter {
    fn new(sample_rate: f32) -> Self {
        Self {
            gain:      1.0,
            rel_coeff: f32::exp(-1.0 / (0.05 * sample_rate)), // 50 ms time constant
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, ceiling: f32) -> (f32, f32) {
        let peak   = l.abs().max(r.abs());
        let target = if peak > ceiling && peak > 1e-10 { ceiling / peak } else { 1.0 };
        // Hard attack (instantaneous gain reduction), soft release.
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain = 1.0 - self.rel_coeff * (1.0 - self.gain);
            self.gain = self.gain.min(1.0);
        }
        (l * self.gain, r * self.gain)
    }
}

// ── Voice processor ───────────────────────────────────────────────────────────

pub struct Voice {
    sample_rate: f32,
    /// Parameter targets by id (EQ in dB, drive, width, ceiling as linear gain).
    target: [f32; 6],
    /// 20 ms one-pole smoothed values of `target`.
    cur:    [f32; 6],
    smooth: f32,
    started: bool, // audio processed yet? before that, set_parameter snaps the smoothers
    // DSP state
    eq_l:    EqChannel,
    eq_r:    EqChannel,
    doubler: Doubler,
    limiter: Limiter,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let unity = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        Self {
            sample_rate,
            target: unity,
            cur:    unity,
            smooth: (-1.0 / (0.020 * sample_rate)).exp(),
            started: false,
            eq_l:    EqChannel::new(),
            eq_r:    EqChannel::new(),
            doubler: Doubler::new(sample_rate),
            limiter: Limiter::new(sample_rate),
        }
    }

    /// Recompute band `k` from its smoothed gain; both channels share coefficients.
    fn set_band(&mut self, k: usize) {
        let (db, sr) = (self.cur[k] as f64, self.sample_rate as f64);
        let b = &mut self.eq_l.bands[k];
        match k {
            0 => b.set_low_shelf(db, 200.0, sr),
            1 => b.set_peak(db, 2000.0, 0.7, sr),
            _ => b.set_high_shelf(db, 8000.0, sr),
        }
        let r = &mut self.eq_r.bands[k];
        *r = Biquad { s1: r.s1, s2: r.s2, ..self.eq_l.bands[k] };
    }
}

impl AddonProcessor for Voice {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], meters: &mut [f32]) {
        let mut min_gain = 1.0_f32;
        for i in 0..left.len() {
            // 0. Smoothing. EQ coefficients are recomputed per sample, but only
            //    while a band is still moving (snapped within 1e-3 dB).
            for k in 0..6 {
                let t = self.target[k];
                if self.cur[k] != t {
                    let v = t + (self.cur[k] - t) * self.smooth;
                    let eps = if k < 3 { 1e-3 } else { 1e-6 };
                    self.cur[k] = if (v - t).abs() < eps { t } else { v };
                    if k < 3 { self.set_band(k); }
                }
            }
            let [_, _, _, drive, width, ceiling] = self.cur;
            // 1. EQ
            let mut l = self.eq_l.process(left[i]);
            let mut r = self.eq_r.process(right[i]);
            // 2. Saturation
            l = saturate(l, drive);
            r = saturate(r, drive);
            // 3. Width
            (l, r) = self.doubler.process(l, r, width);
            // 4. Limiter
            (left[i], right[i]) = self.limiter.process(l, r, ceiling);
            min_gain = min_gain.min(self.limiter.gain);
        }
        self.started |= !left.is_empty();
        if let [gr, ..] = meters {
            *gr = (-20.0 * min_gain.max(1e-7).log10()).max(0.0);
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        if !value.is_finite() { return; }
        let v = match id {
            P_LOW | P_MID | P_HIGH => value.clamp(-12.0, 12.0),
            P_DRIVE | P_WIDTH      => value.clamp(0.0, 1.0),
            P_CEILING              => 10_f32.powf(value.clamp(-12.0, 0.0) / 20.0),
            _ => return,
        };
        let k = id as usize;
        self.target[k] = v;
        if !self.started {
            self.cur[k] = v;
            if k < 3 { self.set_band(k); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_keeps_quiet_signals_at_unity_and_only_squashes_peaks() {
        for drive in [0.5, 1.0] {
            let quiet = saturate(0.001, drive) / 0.001;
            assert!((quiet - 1.0).abs() < 0.01, "drive {drive}: small-signal gain {quiet}");
            assert!(saturate(1.0, drive) < 1.0, "drive {drive}: peaks saturate");
        }
    }

    #[test]
    fn width_raised_after_zero_plays_current_audio_not_stale() {
        let mut v = Voice::new(48_000.0);
        let run = |v: &mut Voice, x: f32, n: usize| {
            let (mut l, mut r) = (vec![x; n], vec![x; n]);
            v.process(&mut l, &mut r, &mut []);
            r
        };
        v.set_parameter(P_WIDTH, 1.0);
        run(&mut v, 0.5, 4800);
        v.set_parameter(P_WIDTH, 0.0);
        run(&mut v, 0.0, 4800); // width 0: the delay line must still be fed
        v.set_parameter(P_WIDTH, 1.0);
        assert!(run(&mut v, 0.0, 64).iter().all(|&s| s == 0.0), "stale audio replayed");
    }

    const SR: f32 = 48_000.0;

    fn sine(n: usize, freq: f32, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / SR).sin()).collect()
    }

    /// Process both channels in 480-sample blocks; returns (L, R).
    fn render(v: &mut Voice, l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (l.to_vec(), r.to_vec());
        for (cl, cr) in l.chunks_mut(480).zip(r.chunks_mut(480)) {
            v.process(cl, cr, &mut [0.0]);
        }
        (l, r)
    }

    fn max_step(y: &[f32]) -> f32 {
        y.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn band_to_zero_db_does_not_click() {
        let x = sine(24_000 + 4_800, 100.0, 0.25);
        for band in [P_LOW, P_MID, P_HIGH] {
            for end in [0.0, 0.04] {
                let mut v = Voice::new(SR);
                v.set_parameter(band, 12.0);
                let (mut y, _) = render(&mut v, &x[..24_000], &x[..24_000]);
                v.set_parameter(band, end);
                y.extend(render(&mut v, &x[24_000..], &x[24_000..]).0);
                let before = max_step(&y[19_200..24_000]);
                let after = max_step(&y[23_999..]);
                println!("band {band} -> {end} dB: {:.3}x", after / before);
                assert!(after <= 1.5 * before, "band {band} -> {end} dB: step {after} vs {before}");
            }
        }
    }

    #[test]
    fn mono_sum_is_flat_at_full_width() {
        let mut s = 1u32;
        let mut noise = |n: usize| -> Vec<f32> {
            (0..n).map(|_| { s = s.wrapping_mul(1664525).wrapping_add(1013904223); ((s >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0) * 0.25 }).collect()
        };
        let (l, r) = (noise(24_000), noise(24_000));
        let (mut v0, mut v1) = (Voice::new(SR), Voice::new(SR));
        v1.set_parameter(P_WIDTH, 1.0);
        let (l0, r0) = render(&mut v0, &l, &r);
        let (l1, r1) = render(&mut v1, &l, &r);
        assert!(l1 != l0 || r1 != r0, "width 1 must change the image");
        for i in 0..l.len() {
            let d = ((l1[i] + r1[i]) - (l0[i] + r0[i])).abs();
            assert!(d < 1e-6, "sample {i}: mono sum differs by {d}");
        }
    }

    #[test]
    fn full_drive_loses_at_most_3db_on_minus6_sine() {
        let amp = 10f32.powf(-6.0 / 20.0);
        let x = sine(48_000, 1000.0, amp);
        let mut v = Voice::new(SR);
        v.set_parameter(P_DRIVE, 1.0);
        let (y, _) = render(&mut v, &x, &x);
        let y = &y[24_000..]; // 24 whole cycles of 1 kHz
        let peak = y.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let peak_db = 20.0 * (peak / amp).log10();
        let bin = |k: f64| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &s) in y.iter().enumerate() {
                let ph = std::f64::consts::TAU * k * 1000.0 * i as f64 / SR as f64;
                re += s as f64 * ph.cos();
                im += s as f64 * ph.sin();
            }
            re * re + im * im
        };
        let thd = ((2..=15).map(|k| bin(k as f64)).sum::<f64>() / bin(1.0)).sqrt();
        println!("peak {peak_db:.2} dB, THD {:.1}%", thd * 100.0);
        assert!(peak_db >= -3.0, "peak {peak_db} dB");
        assert!(thd > 0.05, "THD {thd}");
    }

    #[test]
    fn low_shelf_100hz_matches_ui() {
        let mut b = Biquad::identity();
        b.set_low_shelf(12.0, 200.0, SR as f64);
        let w = 2.0 * PI * 100.0 / SR as f64;
        // |H(e^jw)|² for b0 + b1 z⁻¹ + b2 z⁻², 1 + a1 z⁻¹ + a2 z⁻²
        let mag2 = |c0: f64, c1: f64, c2: f64| {
            let re = c0 + c1 * w.cos() + c2 * (2.0 * w).cos();
            let im = c1 * w.sin() + c2 * (2.0 * w).sin();
            re * re + im * im
        };
        let db = 10.0 * (mag2(b.b0, b.b1, b.b2) / mag2(1.0, b.a1, b.a2)).log10();
        println!("low shelf +12 dB at 100 Hz: {db:.4}");
        assert!((db - 11.1028).abs() < 0.01, "low shelf +12 dB at 100 Hz: {db}");
    }
}
