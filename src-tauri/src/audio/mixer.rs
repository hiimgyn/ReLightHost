use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use crate::audio::vu_meter::VUMeter;

/// One interleaved stereo frame. Ring buffers between legs carry whole
/// frames so a producer/consumer race can never split an L/R pair (which
/// used to leave the channels swapped until the next split).
pub type StereoFrame = [f32; 2];

/// Drains up to `left.len()` frames from `cons` into `left`/`right`; missing
/// frames become silence. Returns how many frames were missing (underruns).
pub fn pop_frames(cons: &mut ringbuf::HeapCons<StereoFrame>, left: &mut [f32], right: &mut [f32]) -> u64 {
    use ringbuf::traits::Consumer;
    let mut underruns = 0;
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        let [fl, fr] = cons.try_pop().unwrap_or_else(|| {
            underruns += 1;
            [0.0, 0.0]
        });
        *l = fl;
        *r = fr;
    }
    underruns
}

/// Bounds the latency a ring buffer between two legs can accumulate.
///
/// The backlog captured before the output leg starts, plus clock drift
/// between two devices, otherwise stays as permanent latency (up to the
/// ring's full capacity). Tracks the *minimum* occupancy seen over a window
/// — the latency that is truly excess, regardless of how bursty the
/// producer is — and trims the oldest frames so that minimum becomes one
/// consumer block.
// ponytail: drops frames (one small click per trim) instead of resampling;
// add an adaptive resampler if drift trims turn out to be audible.
pub struct BacklogTrimmer {
    window: usize,
    elapsed: usize,
    min_seen: usize,
}

impl BacklogTrimmer {
    /// `window` in frames — e.g. one second's worth.
    pub fn new(window: usize) -> Self {
        Self { window: window.max(1), elapsed: 0, min_seen: usize::MAX }
    }

    /// Call right before popping `block` frames. Returns frames dropped.
    pub fn before_pop<T>(&mut self, cons: &mut ringbuf::HeapCons<T>, block: usize) -> usize {
        use ringbuf::traits::{Consumer, Observer};
        self.min_seen = self.min_seen.min(cons.occupied_len());
        self.elapsed += block;
        if self.elapsed < self.window {
            return 0;
        }
        let excess = self.min_seen.saturating_sub(block);
        self.elapsed = 0;
        self.min_seen = usize::MAX;
        if excess > 0 { cons.skip(excess) } else { 0 }
    }
}

pub type AudioProcessFn = Box<dyn Fn(&mut [f32], &mut [f32]) + Send + 'static>;

pub struct MixerState {
    pub process_fn: Arc<Mutex<Option<AudioProcessFn>>>,
    pub vu_meter: Arc<VUMeter>,
    pub muted: Arc<std::sync::atomic::AtomicBool>,
    pub loopback_enabled: Arc<std::sync::atomic::AtomicBool>,
    pub dsp_load_u32: Arc<std::sync::atomic::AtomicU32>,
    pub output_is_asio: bool,
    /// Cumulative count of ring-buffer drain misses (`try_pop()` returning
    /// `None`) in the real-time consumer loops that feed this mixer stage's
    /// output (see `backend::asio::start_output_only` and
    /// `backend::wasapi::start_render`) — surfaced via `AudioStatus::underrun_count`.
    pub underrun_count: Arc<std::sync::atomic::AtomicU64>,
    /// Set by an output thread whose device failed (see `AudioManager::get_status`).
    pub stream_failed: Arc<std::sync::atomic::AtomicBool>,
}

/// Result of processing one audio block: whether to mirror it to the
/// virtual-output producer, plus the exact `muted`/`loopback` snapshot
/// used to decide that, so the caller can reuse the SAME snapshot for the
/// main-output gate instead of re-reading the atomics (which could race
/// against a UI-thread toggle and let the two gates disagree for a block).
pub struct MixerBlockResult {
    pub mirror_to_virtual: bool,
    pub is_muted: bool,
    pub is_loopback: bool,
}

/// Runs the plugin chain in place on `left`/`right`, updates the VU meter
/// and DSP-load estimate, and resolves the mute/loopback gate. Returns
/// whether the caller should mirror this block to the virtual-output
/// producer (the caller owns that ring buffer, not this function), plus
/// the `is_muted`/`is_loopback` snapshot used to decide it.
///
/// Extracted verbatim from the original `AudioManager::toggle_monitoring`
/// output-stream closure so both the ASIO and WASAPI backends share one
/// implementation instead of two hand-kept-in-sync copies.
pub fn process_block(left: &mut [f32], right: &mut [f32], state: &MixerState, sample_rate_hz: f64) -> MixerBlockResult {
    let t0 = std::time::Instant::now();
    if let Ok(guard) = state.process_fn.try_lock() {
        if let Some(ref f) = *guard {
            f(left, right);
            let dsp_ns = t0.elapsed().as_nanos() as f64;
            if sample_rate_hz > 0.0 && !left.is_empty() {
                let block_ns = left.len() as f64 / sample_rate_hz * 1_000_000_000.0;
                if block_ns > 0.0 {
                    let ratio = (dsp_ns / block_ns) * 100.0;
                    let measured = if ratio.is_finite() { ratio.clamp(0.0, 100.0) as f32 } else { 0.0 };
                    let old = f32::from_bits(state.dsp_load_u32.load(Ordering::Relaxed));
                    let smoothed = old * 0.9 + measured * 0.1;
                    state.dsp_load_u32.store(smoothed.to_bits(), Ordering::Relaxed);
                }
            }
        }
    }

    state.vu_meter.update(left, right, t0);

    // Read mute and loopback flags once so both output paths (this
    // function's virtual-mirror decision, and the caller's main-output
    // gate) use the exact same snapshot for this block.
    let is_muted = state.muted.load(Ordering::Relaxed);
    let is_loopback = state.loopback_enabled.load(Ordering::Relaxed);

    // ASIO: main output follows mute, virtual mirror follows loopback.
    // Non-ASIO: main output follows loopback, virtual mirror follows !mute.
    // (Matches the pre-existing manager.rs gate polarity exactly.)
    let mirror_to_virtual = if state.output_is_asio { is_loopback } else { !is_muted };
    MixerBlockResult { mirror_to_virtual, is_muted, is_loopback }
}

/// Resolves whether the main hardware-output path should currently be
/// silent. Callers combine this with `frame < frames_to_process` from
/// their own ring-buffer-drain bookkeeping.
pub fn main_output_gate_open(output_is_asio: bool, muted: bool, loopback: bool) -> bool {
    if output_is_asio { !muted } else { loopback }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn state(output_is_asio: bool) -> MixerState {
        MixerState {
            process_fn: std::sync::Arc::new(std::sync::Mutex::new(None)),
            vu_meter: std::sync::Arc::new(crate::audio::vu_meter::VUMeter::new()),
            muted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            loopback_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            dsp_load_u32: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            output_is_asio,
            underrun_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            stream_failed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    #[test]
    fn pop_frames_keeps_left_right_paired_and_counts_underruns_per_frame() {
        use ringbuf::{HeapRb, traits::{Producer, Split}};
        let (mut prod, mut cons) = HeapRb::<StereoFrame>::new(8).split();
        prod.try_push([0.1, -0.1]).unwrap();

        let (mut l, mut r) = ([9.0f32; 3], [9.0f32; 3]);
        let underruns = pop_frames(&mut cons, &mut l, &mut r);
        assert_eq!((l, r), ([0.1, 0.0, 0.0], [-0.1, 0.0, 0.0]));
        assert_eq!(underruns, 2, "one underrun per missing frame, not per sample");

        prod.try_push([0.2, -0.2]).unwrap();
        let (mut l, mut r) = ([0.0f32; 1], [0.0f32; 1]);
        assert_eq!(pop_frames(&mut cons, &mut l, &mut r), 0);
        assert_eq!((l[0], r[0]), (0.2, -0.2));
    }

    use ringbuf::{HeapRb, traits::{Observer, Producer, Split}};

    fn fill(prod: &mut ringbuf::HeapProd<StereoFrame>, n: usize, start: usize) {
        for i in start..start + n {
            prod.try_push([i as f32, -(i as f32)]).unwrap();
        }
    }

    #[test]
    fn trimmer_drops_a_steady_backlog_down_to_one_block() {
        let (mut prod, mut cons) = HeapRb::<StereoFrame>::new(4096).split();
        let mut trimmer = BacklogTrimmer::new(1000);
        let (mut l, mut r) = ([0.0f32; 100], [0.0f32; 100]);
        fill(&mut prod, 1000, 0); // 1000-frame backlog, then producer keeps pace
        let mut next = 1000;
        for _ in 0..10 {
            trimmer.before_pop(&mut cons, 100);
            pop_frames(&mut cons, &mut l, &mut r);
            fill(&mut prod, 100, next);
            next += 100;
        }
        trimmer.before_pop(&mut cons, 100);
        assert_eq!(cons.occupied_len(), 100, "excess backlog should be trimmed to one block");
    }

    #[test]
    fn trimmer_works_on_any_frame_type() {
        let (mut prod, mut cons) = HeapRb::<[f32; 4]>::new(64).split();
        for i in 0..40 {
            prod.try_push([i as f32; 4]).unwrap();
        }
        let mut trimmer = BacklogTrimmer::new(8);
        assert_eq!(trimmer.before_pop(&mut cons, 8), 32);
        assert_eq!(cons.occupied_len(), 8);
    }

    #[test]
    fn trimmer_leaves_a_bursty_producer_alone() {
        // Producer delivers 480-frame bursts, consumer pulls 64: occupancy
        // swings high but regularly drops near zero — no excess latency.
        let (mut prod, mut cons) = HeapRb::<StereoFrame>::new(4096).split();
        let mut trimmer = BacklogTrimmer::new(1000);
        let (mut l, mut r) = ([0.0f32; 64], [0.0f32; 64]);
        let (mut produced, mut consumed) = (0usize, 0usize);
        for _ in 0..200 {
            if produced < consumed + 64 {
                fill(&mut prod, 480, produced);
                produced += 480;
            }
            trimmer.before_pop(&mut cons, 64);
            assert_eq!(pop_frames(&mut cons, &mut l, &mut r), 0, "trimmer caused an underrun");
            consumed += 64;
        }
    }

    #[test]
    fn main_output_gate_polarity_matches_backend() {
        // ASIO: main output follows mute (open when NOT muted).
        assert!(main_output_gate_open(true, false, false));
        assert!(!main_output_gate_open(true, true, false));
        // Non-ASIO: main output follows loopback, independent of mute.
        assert!(main_output_gate_open(false, true, true));
        assert!(!main_output_gate_open(false, false, false));
    }

    #[test]
    fn asio_virtual_mirror_follows_loopback_not_mute() {
        let s = state(true);
        s.muted.store(true, Ordering::Relaxed);
        s.loopback_enabled.store(true, Ordering::Relaxed);
        let mut l = vec![0.5f32; 4];
        let mut r = vec![0.5f32; 4];
        let result = process_block(&mut l, &mut r, &s, 48_000.0);
        assert!(result.mirror_to_virtual, "ASIO output: virtual mirror must follow loopback flag, not mute");
        assert!(result.is_muted);
        assert!(result.is_loopback);
    }

    #[test]
    fn plugin_chain_runs_when_lock_uncontended() {
        let s = state(false);
        *s.process_fn.lock().unwrap() = Some(Box::new(|l: &mut [f32], r: &mut [f32]| {
            for s in l.iter_mut() { *s *= 0.5; }
            for s in r.iter_mut() { *s *= 0.5; }
        }));
        let mut l = vec![1.0f32; 4];
        let mut r = vec![1.0f32; 4];
        process_block(&mut l, &mut r, &s, 48_000.0);
        assert_eq!(l[0], 0.5);
        assert_eq!(r[0], 0.5);
    }
}
