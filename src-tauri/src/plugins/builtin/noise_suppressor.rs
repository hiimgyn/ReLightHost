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
/// Gate: when `last_vad < vad_gate_threshold`, output is attenuated by
/// `gate_attenuation` (0 = no reduction, 1 = full silence).  Smoothed with a
/// ~70 ms time constant to prevent audible clicks.
use std::collections::VecDeque;
use nnnoiseless::DenoiseState;
use super::BuiltinProcessor;

const FRAME_SIZE: usize = nnnoiseless::FRAME_SIZE; // 480
const SCALE: f32 = 32768.0;

/// Smoothing coefficient for the VAD gate — ~70 ms time constant at 48 kHz.
const GATE_COEFF: f32 = 0.9997;

pub const ID: &str = "builtin::noise_suppressor";

pub struct NoiseSuppressor {
    state_l: Box<DenoiseState<'static>>,
    state_r: Box<DenoiseState<'static>>,
    in_l:    VecDeque<f32>,
    in_r:    VecDeque<f32>,
    out_l:   VecDeque<f32>,
    out_r:   VecDeque<f32>,
    dry_l:   VecDeque<f32>,
    dry_r:   VecDeque<f32>,

    // Parameters (stored as native units, not normalised)
    /// Wet/dry mix: 0 = pass-through, 1 = fully denoised.
    mix:                 f32,
    /// VAD probability below which gating is applied (0 = disabled).
    vad_gate_threshold:  f32,
    /// How much to attenuate when gated (0 = no effect, 1 = full silence).
    gate_attenuation:    f32,
    /// Output gain as a linear multiplier (converted from dB on set_parameter).
    output_gain:         f32,

    // State
    pub last_vad: f32,
    gate_gain:    f32, // current (smoothed) gate multiplier
}

impl NoiseSuppressor {
    pub fn new() -> Self {
        Self {
            state_l: DenoiseState::new(),
            state_r: DenoiseState::new(),
            in_l:  VecDeque::new(),
            in_r:  VecDeque::new(),
            out_l: VecDeque::new(),
            out_r: VecDeque::new(),
            dry_l: VecDeque::new(),
            dry_r: VecDeque::new(),
            mix:                1.0,
            vad_gate_threshold: 0.0,
            gate_attenuation:   0.0,
            output_gain:        1.0,
            last_vad:           0.0,
            gate_gain:          1.0,
        }
    }
}

impl BuiltinProcessor for NoiseSuppressor {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n           = left.len();
        let mix         = self.mix;
        let output_gain = self.output_gain;

        // Save dry copies for wet/dry blending.
        for &s in left.iter()  { self.dry_l.push_back(s); }
        for &s in right.iter() { self.dry_r.push_back(s); }

        // Accumulate scaled input for RNNoise (expects PCM-16 amplitude).
        for &s in left.iter()  { self.in_l.push_back(s * SCALE); }
        for &s in right.iter() { self.in_r.push_back(s * SCALE); }

        // Drain complete 480-sample frames through the denoiser.
        let mut fi_l = [0.0f32; FRAME_SIZE];
        let mut fi_r = [0.0f32; FRAME_SIZE];
        let mut fo_l = [0.0f32; FRAME_SIZE];
        let mut fo_r = [0.0f32; FRAME_SIZE];

        while self.in_l.len() >= FRAME_SIZE {
            for s in fi_l.iter_mut() { *s = self.in_l.pop_front().unwrap_or(0.0); }
            for s in fi_r.iter_mut() { *s = self.in_r.pop_front().unwrap_or(0.0); }

            let vad_l = self.state_l.process_frame(&mut fo_l, &fi_l);
            let vad_r = self.state_r.process_frame(&mut fo_r, &fi_r);
            self.last_vad = (vad_l + vad_r) * 0.5;

            for &s in &fo_l { self.out_l.push_back(s / SCALE); }
            for &s in &fo_r { self.out_r.push_back(s / SCALE); }
        }

        // Compute gate target for this block.
        // Gate is active only when both threshold and attenuation are non-zero.
        let gate_target = if self.vad_gate_threshold > 0.0
            && self.gate_attenuation > 0.0
            && self.last_vad < self.vad_gate_threshold
        {
            1.0 - self.gate_attenuation
        } else {
            1.0
        };

        // Write output with wet/dry blend + gate + output gain.
        // Pass-through samples that have no denoised counterpart yet
        // (initial FRAME_SIZE latency on startup).
        let avail = self.out_l.len().min(n);
        for i in 0..avail {
            self.gate_gain = GATE_COEFF * self.gate_gain + (1.0 - GATE_COEFF) * gate_target;
            let dry_l = self.dry_l.pop_front().unwrap_or(left[i]);
            let dry_r = self.dry_r.pop_front().unwrap_or(right[i]);
            let wet_l = self.out_l.pop_front().unwrap();
            let wet_r = self.out_r.pop_front().unwrap();
            left[i]  = (dry_l + mix * (wet_l - dry_l)) * self.gate_gain * output_gain;
            right[i] = (dry_r + mix * (wet_r - dry_r)) * self.gate_gain * output_gain;
        }
        // Pass-through during startup fill phase.
        for i in avail..n {
            left[i]  *= output_gain;
            right[i] *= output_gain;
        }
        // Prevent dry buffer from growing unbounded during the startup latency phase.
        while self.dry_l.len() > FRAME_SIZE * 2 { self.dry_l.pop_front(); }
        while self.dry_r.len() > FRAME_SIZE * 2 { self.dry_r.pop_front(); }
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

    fn get_vad(&self) -> f32 { self.last_vad }
}

impl Default for NoiseSuppressor {
    fn default() -> Self { Self::new() }
}

// SAFETY: DenoiseState contains only plain f32 arrays; safe to send across
// threads as long as only one thread calls it at a time (enforced by the
// Mutex<Option<Box<dyn BuiltinProcessor>>> in PluginInstance).
unsafe impl Send for NoiseSuppressor {}
