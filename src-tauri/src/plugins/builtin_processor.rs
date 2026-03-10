/// Built-in audio processors compiled directly into ReLightHost.
///
/// These appear in the plugin library under the "Built-in" category and can be
/// added to the plugin chain just like any external VST3/VST2 plugin.  They are
/// created and destroyed without disk I/O or DLL loading.

use std::collections::VecDeque;
use nnnoiseless::DenoiseState;

/// RNNoise frame size: must receive exactly this many samples per call.
const FRAME_SIZE: usize = nnnoiseless::FRAME_SIZE; // 480

/// Amplitude scale expected by RNNoise (trained on 16-bit PCM range).
const SCALE: f32 = 32768.0;

// ---------------------------------------------------------------------------
// NoiseSuppressor
// ---------------------------------------------------------------------------

/// Stereo noise suppressor based on the RNNoise algorithm (via `nnnoiseless`).
///
/// Processes audio in 480-sample frames per channel.  Input is buffered between
/// calls so any block size is supported with an initial latency of at most
/// FRAME_SIZE samples (~10 ms at 48 kHz).
///
/// The algorithm was trained at 48 kHz; it still works at other sample rates
/// with a slight quality reduction, so no explicit resampling is performed.
pub struct NoiseSuppressor {
    state_l: Box<DenoiseState<'static>>,
    state_r: Box<DenoiseState<'static>>,
    /// Pending scaled input — left channel
    in_l: VecDeque<f32>,
    /// Pending scaled input — right channel
    in_r: VecDeque<f32>,
    /// Denoised output ready to consume — left channel
    out_l: VecDeque<f32>,
    /// Denoised output ready to consume — right channel
    out_r: VecDeque<f32>,
    /// Dry input saved for wet/dry blending — left channel
    dry_l: VecDeque<f32>,
    /// Dry input saved for wet/dry blending — right channel
    dry_r: VecDeque<f32>,
    /// Wet/dry mix: 0.0 = full dry (bypass), 1.0 = full denoised (default).
    pub mix: f32,
    /// Latest voice-activity probability averaged over L+R (0.0 – 1.0).
    /// Updated every processed frame; read by the GUI without locking audio.
    pub last_vad: f32,
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
            mix:      1.0,
            last_vad: 0.0,
        }
    }

    /// Set the wet/dry mix (0.0 = dry, 1.0 = fully denoised).
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Return the last measured voice-activity probability (0.0 – 1.0).
    pub fn get_vad(&self) -> f32 {
        self.last_vad
    }

    /// Process a stereo buffer in-place.
    ///
    /// Samples are expected in [-1.0, 1.0].  Processed samples are written
    /// back to the same slices; any samples beyond the current denoised backlog
    /// (initial fill phase) are left as-is (pass-through), so audio is never
    /// silenced during start-up.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len();
        let mix = self.mix;

        // Save dry copies for wet/dry blending.
        for &s in left.iter()  { self.dry_l.push_back(s); }
        for &s in right.iter() { self.dry_r.push_back(s); }

        // Accumulate input scaled to RNNoise amplitude range.
        for &s in left.iter()  { self.in_l.push_back(s * SCALE); }
        for &s in right.iter() { self.in_r.push_back(s * SCALE); }

        // Drain complete 480-sample frames through the denoiser.
        let mut frame_in_l  = [0.0f32; FRAME_SIZE];
        let mut frame_in_r  = [0.0f32; FRAME_SIZE];
        let mut frame_out_l = [0.0f32; FRAME_SIZE];
        let mut frame_out_r = [0.0f32; FRAME_SIZE];

        while self.in_l.len() >= FRAME_SIZE {
            for s in frame_in_l.iter_mut() {
                *s = self.in_l.pop_front().unwrap_or(0.0);
            }
            for s in frame_in_r.iter_mut() {
                *s = self.in_r.pop_front().unwrap_or(0.0);
            }

            let vad_l = self.state_l.process_frame(&mut frame_out_l[..], &frame_in_l[..]);
            let vad_r = self.state_r.process_frame(&mut frame_out_r[..], &frame_in_r[..]);
            self.last_vad = (vad_l + vad_r) * 0.5;

            for &s in &frame_out_l { self.out_l.push_back(s / SCALE); }
            for &s in &frame_out_r { self.out_r.push_back(s / SCALE); }
        }

        // Write wet/dry blended output; pass through if denoised buffer not yet full.
        let avail = self.out_l.len().min(n);
        for i in 0..avail {
            let dry_l = self.dry_l.pop_front().unwrap_or(left[i]);
            let dry_r = self.dry_r.pop_front().unwrap_or(right[i]);
            let wet_l = self.out_l.pop_front().unwrap();
            let wet_r = self.out_r.pop_front().unwrap();
            left[i]  = dry_l + mix * (wet_l - dry_l);
            right[i] = dry_r + mix * (wet_r - dry_r);
        }
        // Trim leftover dry samples that had no denoised counterpart yet.
        while self.dry_l.len() > FRAME_SIZE * 2 { self.dry_l.pop_front(); }
        while self.dry_r.len() > FRAME_SIZE * 2 { self.dry_r.pop_front(); }
    }

    /// Unique identifier used as `path` in `PluginInfo` for the built-in registry.
    pub const ID: &'static str = "builtin::noise_suppressor";
}

impl Default for NoiseSuppressor {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: DenoiseState contains only plain f32 arrays; it is safe to send
// across threads as long as only one thread calls it at a time (enforced by
// Mutex<NoiseSuppressor> in PluginInstance).
unsafe impl Send for NoiseSuppressor {}
