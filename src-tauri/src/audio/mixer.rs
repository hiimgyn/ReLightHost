use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use crate::audio::vu_meter::VUMeter;

pub struct MixerState {
    pub process_fn: Arc<Mutex<Option<Box<dyn Fn(&mut [f32], &mut [f32]) + Send + 'static>>>>,
    pub vu_meter: Arc<VUMeter>,
    pub muted: Arc<std::sync::atomic::AtomicBool>,
    pub loopback_enabled: Arc<std::sync::atomic::AtomicBool>,
    pub dsp_load_u32: Arc<std::sync::atomic::AtomicU32>,
    pub output_is_asio: bool,
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
            let block_ns = left.len() as f64 / sample_rate_hz * 1_000_000_000.0;
            let measured = ((dsp_ns / block_ns) * 100.0).clamp(0.0, 100.0) as f32;
            let old = f32::from_bits(state.dsp_load_u32.load(Ordering::Relaxed));
            let smoothed = old * 0.9 + measured * 0.1;
            state.dsp_load_u32.store(smoothed.to_bits(), Ordering::Relaxed);
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
