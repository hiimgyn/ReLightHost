/// Built-in noise suppressor via RNNoise (nnnoiseless).
///
/// # Parameters (set via `set_parameter`)
/// | ID | Name                  | Range           | Default | Unit  |
/// |----|---------------------- |-----------------|---------|-------|
/// | 0  | Mix                   | 0.0 – 1.0       | 1.0     | ratio |
/// | 1  | VAD Gate Threshold    | 0.0 – 1.0       | 0.0     | ratio |
/// | 2  | Gate Attenuation      | 0.0 – 1.0       | 0.0     | ratio |
/// | 3  | Output Gain           | -24.0 – +12.0   | 0.0     | dB    |
///
/// Gate (per RNNoise frame, aligned with that frame's output): opens when
/// VAD ≥ threshold, closes after a 200 ms hold once VAD < threshold − 0.1.
/// Closed gain is `1 − gate_attenuation`; it opens with a 3 ms and closes
/// with a 150 ms time constant. VAD is the louder channel's (max of L, R).
/// Mix and output gain glide with a 10 ms time constant.
use std::collections::VecDeque;
use nnnoiseless::DenoiseState;
use relight_addon_api::{AddonDescriptor, AddonProcessor, ParamSpec, Scale};

const fn p(id: u32, name: &'static str, min: f32, max: f32, default: f32, unit: &'static str, scale: Scale) -> ParamSpec {
    ParamSpec { id, name, min, max, default, unit, scale, step: 0.0, choices: &[] }
}

const FRAME_SIZE: usize = nnnoiseless::FRAME_SIZE; // 480
const SCALE: f32 = 32768.0;
/// Pre-scale input clamp: nnnoiseless hits UB (easyfft) on non-finite or huge input.
const INPUT_LIMIT: f32 = 1000.0;

const GATE_OPEN_MS: f32 = 3.0;
const GATE_CLOSE_MS: f32 = 150.0;
const GATE_HOLD_FRAMES: u32 = 20; // 200 ms of 10 ms frames
const GATE_HYSTERESIS: f32 = 0.1;
const PARAM_SMOOTH_MS: f32 = 10.0;

/// One-pole step per sample for a time constant of `ms` at 48 kHz.
fn one_pole(ms: f32) -> f32 { 1.0 - (-1000.0 / (ms * 48_000.0)).exp() }

/// Moves `cur` toward `target` by `k`, snapping once within 1e-6 or once
/// f32 stops making progress (it stalls short of `target`).
fn glide(cur: &mut f32, target: f32, k: f32) {
    let v = *cur + (target - *cur) * k;
    *cur = if (target - v).abs() < 1e-6 || v == *cur { target } else { v };
}

/// One gate decision per frame from that frame's `vad`: returns (open, new hold).
/// A frame at/above `thr` opens and arms the hold; the gate then stays open
/// for `GATE_HOLD_FRAMES` more frames below the close point (200 ms) and only
/// counts down while below it — between the two points it keeps its state.
fn gate_step(hold: u32, vad: f32, thr: f32) -> (bool, u32) {
    // At thr ≤ 0.1 a fixed 0.1 band would put the close point at or below 0,
    // where VAD can never go, so the gate could never close: use thr / 2 there.
    let close_below = if thr > GATE_HYSTERESIS { thr - GATE_HYSTERESIS } else { thr * 0.5 };
    if vad >= thr {
        (true, GATE_HOLD_FRAMES)
    } else if vad < close_below {
        (hold > 0, hold.saturating_sub(1))
    } else {
        (hold > 0, hold)
    }
}

pub const ID: &str = "builtin::noise_suppressor";

static ADDON_PARAMS: &[ParamSpec] = &[
    p(0, "Mix", 0.0, 1.0, 1.0, "", Scale::Linear),
    p(1, "VAD Gate Threshold", 0.0, 1.0, 0.0, "", Scale::Linear),
    p(2, "Gate Attenuation", 0.0, 1.0, 0.0, "", Scale::Linear),
    p(3, "Output Gain", -24.0, 12.0, 0.0, "dB", Scale::Db),
];
static ADDON_METERS: &[&str] = &["vad", "gate_db"];

pub static ADDON: AddonDescriptor = AddonDescriptor {
    id: ID,
    name: "Noise Suppressor (RNNoise)",
    category: "Noise Reduction",
    version: 1,
    params: ADDON_PARAMS,
    meters: ADDON_METERS,
    sample_rates: &[48000],
    create: |sr| NoiseSuppressor::new(sr).map(|p| Box::new(p) as Box<dyn AddonProcessor>),
    migrate: None,
};
const _: () = assert!(relight_addon_api::check_params(ADDON_PARAMS, ADDON_METERS));

pub struct NoiseSuppressor {
    state_l: Box<DenoiseState<'static>>,
    state_r: Box<DenoiseState<'static>>,
    in_l:    VecDeque<f32>,
    in_r:    VecDeque<f32>,
    out_l:   VecDeque<f32>,
    out_r:   VecDeque<f32>,
    dry_l:   VecDeque<f32>,
    dry_r:   VecDeque<f32>,
    /// Per-sample gate gain, queued in step with `out_l`/`out_r`.
    gate_q:  VecDeque<f32>,

    // Parameters (stored as native units, not normalised)
    /// Wet/dry mix target: 0 = pass-through, 1 = fully denoised.
    mix:                 f32,
    /// VAD probability at which the gate opens (0 = always open).
    vad_gate_threshold:  f32,
    /// How much to attenuate when gated (0 = no effect, 1 = full silence).
    gate_attenuation:    f32,
    /// Output gain target as a linear multiplier (converted from dB on set_parameter).
    output_gain:         f32,

    // State
    pub last_vad: f32,
    /// VAD of the frame whose audio RNNoise emits on the next call.
    pending_vad:  f32,
    gate_gain:    f32, // current (smoothed) gate multiplier
    hold:         u32, // frames below the close point the gate stays open for
    mix_s:        f32, // smoothed mix
    gain_s:       f32, // smoothed output gain
}

// Every queue is sized for its primed frames (at most 2) plus an 8192-sample
// host block, so the audio thread never grows one.
const QUEUE_CAP: usize = FRAME_SIZE * 2 + 8192;

fn primed_queue(frames: usize, value: f32) -> VecDeque<f32> {
    let mut q = VecDeque::with_capacity(QUEUE_CAP);
    q.resize(FRAME_SIZE * frames, value);
    q
}

impl NoiseSuppressor {
    /// Returns `None` if `sample_rate` is not 48000 Hz — RNNoise's FRAME_SIZE
    /// of 480 samples is only valid at 48 kHz (480 / 48000 = 10 ms frame).
    pub fn new(sample_rate: f32) -> Option<Self> {
        if (sample_rate - 48000.0).abs() > 1.0 {
            log::warn!(
                "NoiseSuppressor requires 48 kHz; got {sample_rate} Hz — \
                 plugin will run in pass-through mode to avoid pitch/speed artifacts"
            );
            return None;
        }
        Some(Self {
            state_l: DenoiseState::new(),
            state_r: DenoiseState::new(),
            in_l:  primed_queue(0, 0.0),
            in_r:  primed_queue(0, 0.0),
            // RNNoise only emits whole 480-sample frames, so the output and
            // gate queues start one frame deep: a constant delay and a full
            // block of output for ANY block size (256, 512, 1024 … don't
            // divide 480). RNNoise's overlap-add adds one more frame of delay
            // inside process_frame, so dry is primed with two frames to stay
            // sample-aligned with the denoised signal.
            out_l: primed_queue(1, 0.0),
            out_r: primed_queue(1, 0.0),
            dry_l: primed_queue(2, 0.0),
            dry_r: primed_queue(2, 0.0),
            gate_q: primed_queue(1, 1.0),
            mix:                1.0,
            vad_gate_threshold: 0.0,
            gate_attenuation:   0.0,
            output_gain:        1.0,
            last_vad:           0.0,
            pending_vad:        0.0,
            gate_gain:          1.0,
            hold:               0,
            mix_s:              1.0,
            gain_s:             1.0,
        })
    }
}

impl AddonProcessor for NoiseSuppressor {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], meters: &mut [f32]) {
        // Sanitise before anything sees the input (non-finite → 0, clamp);
        // the dry path carries the same sanitised value.
        let clean = |s: f32| if s.is_finite() { s.clamp(-INPUT_LIMIT, INPUT_LIMIT) } else { 0.0 };
        for &s in left.iter() {
            let s = clean(s);
            self.dry_l.push_back(s);
            self.in_l.push_back(s * SCALE); // RNNoise expects PCM-16 amplitude
        }
        for &s in right.iter() {
            let s = clean(s);
            self.dry_r.push_back(s);
            self.in_r.push_back(s * SCALE);
        }

        // Drain complete 480-sample frames through the denoiser.
        let mut fi_l = [0.0f32; FRAME_SIZE];
        let mut fi_r = [0.0f32; FRAME_SIZE];
        let mut fo_l = [0.0f32; FRAME_SIZE];
        let mut fo_r = [0.0f32; FRAME_SIZE];
        let (k_open, k_close) = (one_pole(GATE_OPEN_MS), one_pole(GATE_CLOSE_MS));
        let closed = 1.0 - self.gate_attenuation;

        while self.in_l.len() >= FRAME_SIZE {
            for s in fi_l.iter_mut() { *s = self.in_l.pop_front().unwrap_or(0.0); }
            for s in fi_r.iter_mut() { *s = self.in_r.pop_front().unwrap_or(0.0); }

            let vad_l = self.state_l.process_frame(&mut fo_l, &fi_l);
            let vad_r = self.state_r.process_frame(&mut fo_r, &fi_r);
            self.last_vad = vad_l.max(vad_r);

            for &s in &fo_l { self.out_l.push_back(s / SCALE); }
            for &s in &fo_r { self.out_r.push_back(s / SCALE); }

            // fo_* holds the previous input frame's audio: gate it on that
            // frame's VAD, with this frame's VAD as lookahead.
            let vad = self.pending_vad.max(self.last_vad);
            self.pending_vad = self.last_vad;
            let open;
            (open, self.hold) = gate_step(self.hold, vad, self.vad_gate_threshold);
            let (target, k) = if open { (1.0, k_open) } else { (closed, k_close) };
            for _ in 0..FRAME_SIZE {
                glide(&mut self.gate_gain, target, k);
                self.gate_q.push_back(self.gate_gain);
            }
        }

        // Wet/dry blend × gate × output gain. The primed queues (see `new`)
        // always hold at least `n` samples here.
        let k = one_pole(PARAM_SMOOTH_MS);
        let mut gate = 1.0f32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            glide(&mut self.mix_s, self.mix, k);
            glide(&mut self.gain_s, self.output_gain, k);
            gate = self.gate_q.pop_front().unwrap_or(1.0);
            let dry_l = self.dry_l.pop_front().unwrap_or(0.0);
            let dry_r = self.dry_r.pop_front().unwrap_or(0.0);
            let wet_l = self.out_l.pop_front().unwrap_or(dry_l);
            let wet_r = self.out_r.pop_front().unwrap_or(dry_r);
            *l = (dry_l + self.mix_s * (wet_l - dry_l)) * gate * self.gain_s;
            *r = (dry_r + self.mix_s * (wet_r - dry_r)) * gate * self.gain_s;
        }
        meters[0] = self.last_vad;
        meters[1] = 20.0 * gate.max(1e-5).log10();
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        match id {
            0 => self.mix                = value.clamp(0.0, 1.0),
            1 => self.vad_gate_threshold = value.clamp(0.0, 1.0),
            2 => self.gate_attenuation   = value.clamp(0.0, 1.0),
            3 => self.output_gain        = 10f32.powf(value / 20.0),
            _ => {}
        }
    }

    /// Back to the just-created state: queues re-primed as in `new` (no
    /// reallocation), fresh RNNoise states (reset runs on a control thread,
    /// so allocating is allowed), gate open with no hold, smoothed params
    /// snapped to their targets.
    fn reset(&mut self) {
        for (q, frames, v) in [
            (&mut self.in_l, 0, 0.0), (&mut self.in_r, 0, 0.0),
            (&mut self.out_l, 1, 0.0), (&mut self.out_r, 1, 0.0),
            (&mut self.dry_l, 2, 0.0), (&mut self.dry_r, 2, 0.0),
            (&mut self.gate_q, 1, 1.0),
        ] {
            q.clear();
            q.resize(FRAME_SIZE * frames, v);
        }
        self.state_l = DenoiseState::new();
        self.state_r = DenoiseState::new();
        self.gate_gain = 1.0;
        self.hold = 0;
        self.last_vad = 0.0;
        self.pending_vad = 0.0;
        self.mix_s = self.mix;
        self.gain_s = self.output_gain;
    }

    /// Output lags input by two RNNoise frames (see `new`).
    fn latency_samples(&self) -> u32 { (FRAME_SIZE * 2) as u32 }
}

impl Default for NoiseSuppressor {
    fn default() -> Self { Self::new(48000.0).expect("48 kHz is always valid") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_output_is_the_input_delayed_by_exactly_two_frames_for_any_block_size() {
        for block in [256usize, 512, 1024, 100] {
            let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
            ns.set_parameter(0, 0.0); // mix = 0: dry path only, so the delay is exact
            ns.reset(); // snap the mix glide: mix is exactly 0 from sample 0
            let input: Vec<f32> = (0..block * 20).map(|i| (i as f32 * 1e-4).sin() * 0.5).collect();
            let mut output = Vec::with_capacity(input.len());
            for chunk in input.chunks(block) {
                let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
                ns.process(&mut l, &mut r, &mut [0.0; 2]);
                output.extend_from_slice(&l);
            }
            for (t, &y) in output.iter().enumerate() {
                let expected = if t < 2 * FRAME_SIZE { 0.0 } else { input[t - 2 * FRAME_SIZE] };
                assert!((y - expected).abs() < 1e-6, "block {block}: sample {t} = {y}, expected {expected}");
            }
        }
    }

    /// Harmonic stack gliding 140 ± 40 Hz at 3 Hz (scale 0.25) from `onset`,
    /// over a −50 dBFS noise floor everywhere.
    fn gliding_voice(n: usize, onset: usize) -> Vec<f32> {
        let tau = std::f32::consts::TAU;
        let floor = 10f32.powf(-50.0 / 20.0);
        let (mut seed, mut phase) = (7u32, 0.0f32);
        (0..n).map(|i| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0) * floor;
            let f0 = 140.0 + 40.0 * (tau * 3.0 * i as f32 / 48_000.0).sin();
            phase = (phase + tau * f0 / 48_000.0) % tau;
            let voice: f32 = (1..9).map(|h| (phase * h as f32).sin() / h as f32).sum();
            noise + if i >= onset { 0.25 * voice } else { 0.0 }
        }).collect()
    }

    /// Runs `ns` over (l, r) in `block`-sized chunks; returns (out_l, out_r, meters per block).
    fn render(ns: &mut NoiseSuppressor, l: &[f32], r: &[f32], block: usize) -> (Vec<f32>, Vec<f32>, Vec<[f32; 2]>) {
        let (mut ol, mut or, mut ms) = (Vec::new(), Vec::new(), Vec::new());
        for (cl, cr) in l.chunks(block).zip(r.chunks(block)) {
            let (mut bl, mut br, mut m) = (cl.to_vec(), cr.to_vec(), [0.0f32; 2]);
            ns.process(&mut bl, &mut br, &mut m);
            ol.extend(bl);
            or.extend(br);
            ms.push(m);
        }
        (ol, or, ms)
    }

    #[test]
    fn smoothers_reach_target_exactly() {
        let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
        ns.set_parameter(0, 0.3);
        ns.set_parameter(3, -6.0);
        let x = gliding_voice(24_000, 0);
        render(&mut ns, &x, &x, FRAME_SIZE);
        assert_eq!(ns.mix_s, ns.mix);
        assert_eq!(ns.gain_s, ns.output_gain);
    }

    fn rms(x: &[f32]) -> f32 { (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt() }

    #[test]
    fn silent_right_channel_does_not_cap_vad() {
        let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
        let l = gliding_voice(FRAME_SIZE * 70, 0);
        let r = vec![0.0; l.len()];
        let (_, _, ms) = render(&mut ns, &l, &r, FRAME_SIZE);
        let mean = ms[20..=60].iter().map(|m| m[0]).sum::<f32>() / 41.0;
        assert!(mean >= 0.8, "mean vad {mean}");
    }

    #[test]
    fn gate_opens_within_5ms_of_speech_onset() {
        let s = 50 * FRAME_SIZE + 470;
        let x = gliding_voice(s + 4800, s);
        let run = |threshold: f32| {
            let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
            ns.set_parameter(0, 1.0);
            ns.set_parameter(1, threshold);
            ns.set_parameter(2, 1.0);
            render(&mut ns, &x, &x, 256).0
        };
        let (gated, open) = (run(0.5), run(0.0));
        let ratio = |a: usize, b: usize| rms(&gated[a..b]) / rms(&open[a..b]);
        let after = ratio(s + 1200, s + 1440);
        let before = ratio(s - 2400, s);
        assert!(after >= 0.708, "post-onset ratio {after}");
        // The gate starts open (as after `new`/`reset`): this measures its 150 ms decay.
        assert!(before <= 0.05, "pre-onset ratio {before}");
    }

    #[test]
    fn non_finite_input_never_reaches_rnnoise() {
        let base = gliding_voice(20_000, 0);
        let run = |v: f32| {
            let mut x = base.clone();
            x[5000] = v;
            let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
            let (l, r, _) = render(&mut ns, &x, &x, 256);
            l.into_iter().chain(r).collect::<Vec<f32>>()
        };
        let zero: Vec<u32> = run(0.0).iter().map(|v| v.to_bits()).collect();
        for bad in [f32::NAN, f32::INFINITY] {
            let got: Vec<u32> = run(bad).iter().map(|v| v.to_bits()).collect();
            assert!(got == zero, "{bad} input differs from 0.0 input");
        }
        assert!(run(1e30).iter().all(|v| v.is_finite()), "1e30 input produced non-finite output");
    }

    /// The wet (denoised) path must carry the same delay as the dry path, or
    /// any mix between 0 and 1 comb-filters. A gliding pitch has no period, so
    /// a 640 or 1280 lag cannot alias onto 960.
    #[test]
    fn wet_lag_is_960_with_gliding_pitch() {
        let n = FRAME_SIZE * 60;
        let input = gliding_voice(n, 0);
        for block in [256usize, 512, 1024, 100] {
            let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
            ns.set_parameter(0, 1.0); // mix = 1: wet only
            let (out, _, _) = render(&mut ns, &input, &input, block);
            let corr = |lag: usize| -> f32 { (lag..n).map(|t| out[t] * input[t - lag]).sum() };
            let best = (0..3 * FRAME_SIZE).max_by(|&a, &b| corr(a).total_cmp(&corr(b))).unwrap();
            assert!(best.abs_diff(960) <= 4, "block {block}: wet lag {best}");
        }
    }

    #[test]
    fn hysteresis_is_exactly_0_1_above_threshold_0_1() {
        // thr 0.15: close point 0.05, not 0.075 — 0.06 holds, 0.04 counts down.
        assert_eq!(gate_step(5, 0.06, 0.15), (true, 5));
        assert_eq!(gate_step(5, 0.04, 0.15), (true, 4));
        assert_eq!(gate_step(0, 0.06, 0.15), (false, 0));
        // thr ≤ 0.1: close point thr / 2, so the gate can still close.
        assert_eq!(gate_step(1, 0.03, 0.08), (true, 0));
        assert_eq!(gate_step(0, 0.03, 0.08), (false, 0));
        assert_eq!(gate_step(5, 0.0, 0.0), (true, GATE_HOLD_FRAMES)); // thr 0: always open
    }

    #[test]
    fn gate_holds_open_for_200ms_after_the_last_open_frame() {
        let (open, mut hold) = gate_step(0, 0.9, 0.5);
        assert!(open);
        // Frame j (1-based) after the last open frame spans [(j−1)·10, j·10) ms.
        for j in 1..=21 {
            let open;
            (open, hold) = gate_step(hold, 0.0, 0.5);
            assert_eq!(open, j <= 20, "frame {j} ({}–{} ms after the last open frame)", (j - 1) * 10, j * 10);
        }
    }

    #[test]
    fn reset_flushes_queues() {
        let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
        let x = gliding_voice(FRAME_SIZE * 50, 0);
        render(&mut ns, &x, &x, 256);
        ns.reset();
        let z = vec![0.0; FRAME_SIZE * 10];
        let (l, r, _) = render(&mut ns, &z, &z, 256);
        let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 1e-3, "max |out| after reset {peak}");
    }
}
