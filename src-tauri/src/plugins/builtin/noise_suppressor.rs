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

fn primed_queue() -> VecDeque<f32> {
    let mut q = VecDeque::with_capacity(FRAME_SIZE * 4);
    q.resize(FRAME_SIZE, 0.0);
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
            in_l:  VecDeque::with_capacity(FRAME_SIZE * 4),
            in_r:  VecDeque::with_capacity(FRAME_SIZE * 4),
            // Output and dry queues start one frame deep: RNNoise only emits
            // whole 480-sample frames, so priming them with a frame of
            // silence gives a constant one-frame delay and guarantees a full
            // block of output for ANY block size (256, 512, 1024 … don't
            // divide 480). Dry stays sample-aligned with the denoised signal.
            out_l: primed_queue(),
            out_r: primed_queue(),
            dry_l: primed_queue(),
            dry_r: primed_queue(),
            mix:                1.0,
            vad_gate_threshold: 0.0,
            gate_attenuation:   0.0,
            output_gain:        1.0,
            last_vad:           0.0,
            gate_gain:          1.0,
        })
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

        // Write output with wet/dry blend + gate + output gain. The primed
        // queues (see `new`) always hold at least `n` samples here.
        for i in 0..n {
            self.gate_gain = GATE_COEFF * self.gate_gain + (1.0 - GATE_COEFF) * gate_target;
            let dry_l = self.dry_l.pop_front().unwrap_or(0.0);
            let dry_r = self.dry_r.pop_front().unwrap_or(0.0);
            let wet_l = self.out_l.pop_front().unwrap_or(dry_l);
            let wet_r = self.out_r.pop_front().unwrap_or(dry_r);
            left[i]  = (dry_l + mix * (wet_l - dry_l)) * self.gate_gain * output_gain;
            right[i] = (dry_r + mix * (wet_r - dry_r)) * self.gate_gain * output_gain;
        }
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

    /// Output lags input by exactly one RNNoise frame (see `new`).
    fn latency_samples(&self) -> u32 { FRAME_SIZE as u32 }
}

impl Default for NoiseSuppressor {
    fn default() -> Self { Self::new(48000.0).expect("48 kHz is always valid") }
}

// SAFETY: DenoiseState contains only plain f32 arrays; safe to send across
// threads as long as only one thread calls it at a time (enforced by the
// Mutex<Option<Box<dyn BuiltinProcessor>>> in PluginInstance).
unsafe impl Send for NoiseSuppressor {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_the_input_delayed_by_exactly_one_frame_for_any_block_size() {
        for block in [256usize, 512, 1024, 100] {
            let mut ns = NoiseSuppressor::new(48_000.0).unwrap();
            ns.set_parameter(0, 0.0); // mix = 0: dry path only, so the delay is exact
            let input: Vec<f32> = (0..block * 20).map(|i| (i as f32 * 1e-4).sin() * 0.5).collect();
            let mut output = Vec::with_capacity(input.len());
            for chunk in input.chunks(block) {
                let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
                ns.process_stereo(&mut l, &mut r);
                output.extend_from_slice(&l);
            }
            for (t, &y) in output.iter().enumerate() {
                let expected = if t < FRAME_SIZE { 0.0 } else { input[t - FRAME_SIZE] };
                assert!((y - expected).abs() < 1e-6, "block {block}: sample {t} = {y}, expected {expected}");
            }
        }
    }
}
