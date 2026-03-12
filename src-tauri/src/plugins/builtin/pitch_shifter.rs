/// Dual-buffer overlap-add pitch shifter.
///
/// Uses two fractional-rate read pointers offset by BUF_LEN/2 with a
/// Hann-window crossfade to avoid clicks at seam boundaries.
/// Quality is "voice-changer" grade — good enough for fun pitch effects
/// without requiring an FFT dependency.
///
/// # Parameters
/// | ID | Name      | Range          | Default | Unit     |
/// |----|-----------|----------------|---------|----------|
/// | 0  | Semitones | -24.0 – 24.0   |   0.0   | semitones|
/// | 1  | Mix       | 0.0  – 1.0     |   1.0   | ratio    |
/// | 2  | Fine      | -100.0 – 100.0 |   0.0   | cents    |

use super::BuiltinProcessor;

pub const ID: &str = "builtin::pitch_shifter";

/// Circular buffer length — must be power-of-2.
const BUF_LEN: usize = 8192;
const BUF_MASK: usize = BUF_LEN - 1;
const HALF_BUF: f64 = (BUF_LEN / 2) as f64;

// ── Per-channel state ─────────────────────────────────────────────────────────

struct PitchChannel {
    buf:   Vec<f32>, // circular delay buffer, length = BUF_LEN
    write: usize,    // next write position
    rd1:   f64,      // fractional read pointer 1
    rd2:   f64,      // fractional read pointer 2 (offset by HALF_BUF)
}

impl PitchChannel {
    fn new(rd_offset: f64) -> Self {
        Self {
            buf:   vec![0.0f32; BUF_LEN],
            write: 0,
            rd1:   0.0,
            rd2:   rd_offset,
        }
    }

    fn process(&mut self, input: f32, factor: f64) -> f32 {
        // Write input sample into ring buffer.
        self.buf[self.write] = input;
        self.write = (self.write + 1) & BUF_MASK;

        // Linear-interpolated reads at fractional positions.
        let out1 = lerp(&self.buf, self.rd1);
        let out2 = lerp(&self.buf, self.rd2);

        // Advance read positions at the pitch factor speed.
        self.rd1 = (self.rd1 + factor).rem_euclid(BUF_LEN as f64);
        self.rd2 = (self.rd2 + factor).rem_euclid(BUF_LEN as f64);

        // Crossfade between the two readers using a triangular (Hann-like)
        // window based on the distance from each read pointer to the write
        // pointer.  When rd1 is about to overrun (collision zone), env1 → 0
        // and rd2 carries the signal — then they swap roles.
        let dist1 = (self.write as f64 - self.rd1).rem_euclid(BUF_LEN as f64);
        let env1 = {
            let t = (dist1 / HALF_BUF).min(2.0); // 0..2
            if t < 1.0 { t } else { 2.0 - t }    // triangle 0→1→0
        } as f32;

        out1 * env1 + out2 * (1.0 - env1)
    }
}

#[inline]
fn lerp(buf: &[f32], pos: f64) -> f32 {
    let i  = pos as usize & BUF_MASK;
    let j  = (i + 1) & BUF_MASK;
    let fr = (pos - pos.floor()) as f32;
    buf[i] * (1.0 - fr) + buf[j] * fr
}

// ── Pitch Shifter ─────────────────────────────────────────────────────────────

pub struct PitchShifter {
    semitones: f32,
    fine:      f32, // cents: -100 .. +100
    mix:       f32,
    ch_l:      PitchChannel,
    ch_r:      PitchChannel,
}

impl PitchShifter {
    pub fn new() -> Self {
        Self {
            semitones: 0.0,
            fine:      0.0,
            mix:       1.0,
            ch_l:      PitchChannel::new(0.0),
            ch_r:      PitchChannel::new(HALF_BUF),
        }
    }

    #[inline]
    fn factor(&self) -> f64 {
        2.0_f64.powf((self.semitones as f64 + self.fine as f64 / 100.0) / 12.0)
    }
}

impl BuiltinProcessor for PitchShifter {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        // Hard bypass: no processing, no latency compensation needed.
        if (self.semitones == 0.0 && self.fine == 0.0) || self.mix == 0.0 {
            return;
        }

        let factor = self.factor();
        let mix    = self.mix;

        for i in 0..left.len() {
            let dry_l = left[i];
            let dry_r = right[i];
            let wet_l = self.ch_l.process(dry_l, factor);
            let wet_r = self.ch_r.process(dry_r, factor);
            left[i]  = dry_l * (1.0 - mix) + wet_l * mix;
            right[i] = dry_r * (1.0 - mix) + wet_r * mix;
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        match id {
            0 => self.semitones = value.clamp(-24.0, 24.0),
            1 => self.mix       = value.clamp(0.0,    1.0),
            2 => self.fine      = value.clamp(-100.0, 100.0),
            _ => {}
        }
    }
}
