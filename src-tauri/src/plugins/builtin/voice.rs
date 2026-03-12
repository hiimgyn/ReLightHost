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
use super::BuiltinProcessor;

pub const ID: &str = "builtin::voice";

const P_LOW:     u32 = 0;
const P_MID:     u32 = 1;
const P_HIGH:    u32 = 2;
const P_DRIVE:   u32 = 3;
const P_WIDTH:   u32 = 4;
const P_CEILING: u32 = 5;

// ── Biquad filter (Transposed Direct Form II) ─────────────────────────────────

#[derive(Clone)]
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

    /// Audio EQ Cookbook — Low shelf, shelf slope S=1.
    fn set_low_shelf(&mut self, gain_db: f64, freq: f64, sr: f64) {
        if gain_db.abs() < 0.05 { *self = Self::identity(); return; }
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
        if gain_db.abs() < 0.05 { *self = Self::identity(); return; }
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
        if gain_db.abs() < 0.05 { *self = Self::identity(); return; }
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

struct EqChannel {
    low:  Biquad,
    mid:  Biquad,
    high: Biquad,
}

impl EqChannel {
    fn new() -> Self {
        Self { low: Biquad::identity(), mid: Biquad::identity(), high: Biquad::identity() }
    }

    fn rebuild(&mut self, low_db: f64, mid_db: f64, high_db: f64, sr: f64) {
        self.low.set_low_shelf(low_db,   200.0, sr);
        self.mid.set_peak(     mid_db,  2000.0, 0.7, sr);
        self.high.set_high_shelf(high_db, 8000.0, sr);
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.high.process(self.mid.process(self.low.process(x)))
    }
}

// ── Soft-clip saturation ──────────────────────────────────────────────────────
// Parallel blend of dry and tanh-saturated wet signal.
// drive=0 → bypass, drive=1 → heavy harmonic saturation.

#[inline]
fn saturate(x: f32, drive: f32) -> f32 {
    if drive < 0.001 { return x; }
    let pre = 1.0 + drive * 7.0;           // pre-gain 1x … 8x
    let norm = pre.tanh().max(1e-6);        // normalise so max-out = 1 at max-in = 1
    let wet = (x * pre).tanh() / norm;
    x * (1.0 - drive) + wet * drive        // parallel blend
}

// ── Haas-effect doubler ───────────────────────────────────────────────────────
// Delays one channel by ~12 ms to create stereo width perception from mono voice.

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
        if width < 0.001 { return (l, r); }
        // Feed a mono mix into the delay line for a source-independent effect.
        let mono = (l + r) * 0.5;
        self.buf[self.write] = mono;
        self.write = (self.write + 1) & (DELAY_BUF - 1);
        let delayed = self.buf[(self.write + DELAY_BUF - self.delay) & (DELAY_BUF - 1)];
        // Left: dry, Right: blended toward delayed mono → stereo width from mono source.
        (l, r * (1.0 - width) + delayed * width)
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
    // Parameters
    low_db:    f32,
    mid_db:    f32,
    high_db:   f32,
    drive:     f32,
    width:     f32,
    ceiling_db: f32,
    // DSP state
    eq_l:    EqChannel,
    eq_r:    EqChannel,
    doubler: Doubler,
    limiter: Limiter,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            low_db: 0.0, mid_db: 0.0, high_db: 0.0,
            drive: 0.0, width: 0.0, ceiling_db: 0.0,
            eq_l:    EqChannel::new(),
            eq_r:    EqChannel::new(),
            doubler: Doubler::new(sample_rate),
            limiter: Limiter::new(sample_rate),
        }
    }

    fn rebuild_eq(&mut self) {
        let sr = self.sample_rate as f64;
        let l  = self.low_db  as f64;
        let m  = self.mid_db  as f64;
        let h  = self.high_db as f64;
        self.eq_l.rebuild(l, m, h, sr);
        self.eq_r.rebuild(l, m, h, sr);
    }
}

impl BuiltinProcessor for Voice {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let ceiling = 10_f32.powf(self.ceiling_db / 20.0);
        for i in 0..left.len() {
            // 1. EQ
            let mut l = self.eq_l.process(left[i]);
            let mut r = self.eq_r.process(right[i]);
            // 2. Saturation
            l = saturate(l, self.drive);
            r = saturate(r, self.drive);
            // 3. Doubler
            let (dl, dr) = self.doubler.process(l, r, self.width);
            l = dl; r = dr;
            // 4. Limiter
            let (ll, lr) = self.limiter.process(l, r, ceiling);
            left[i]  = ll;
            right[i] = lr;
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        let eq_changed = matches!(id, P_LOW | P_MID | P_HIGH);
        match id {
            P_LOW     => self.low_db     = value.clamp(-12.0, 12.0),
            P_MID     => self.mid_db     = value.clamp(-12.0, 12.0),
            P_HIGH    => self.high_db    = value.clamp(-12.0, 12.0),
            P_DRIVE   => self.drive      = value.clamp(0.0, 1.0),
            P_WIDTH   => self.width      = value.clamp(0.0, 1.0),
            P_CEILING => self.ceiling_db = value.clamp(-12.0, 0.0),
            _ => {}
        }
        if eq_changed { self.rebuild_eq(); }
    }
}
