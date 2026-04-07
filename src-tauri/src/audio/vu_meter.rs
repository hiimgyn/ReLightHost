use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

#[inline(always)]
fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

#[inline(always)]
fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

/// VU Meter data for a single channel
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VUChannel {
    /// Current peak level (0.0 - 1.0)
    pub peak: f32,
    /// Peak hold value (0.0 - 1.0)
    pub peak_hold: f32,
    /// RMS (Root Mean Square) level for average loudness
    pub rms: f32,
}

impl Default for VUChannel {
    fn default() -> Self {
        Self {
            peak: 0.0,
            peak_hold: 0.0,
            rms: 0.0,
        }
    }
}

/// VU Meter state for stereo audio
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VUData {
    pub left: VUChannel,
    pub right: VUChannel,
}

/// Lock-free VU meter — safe to call from the realtime audio callback.
///
/// All writable fields are stored as atomic bitwise f32 (`AtomicU32`) so
/// the audio thread never blocks. Peak hold timestamps use `AtomicI64`.
pub struct VUMeter {
    // peaks (written by audio thread, read by UI thread)
    left_peak:  AtomicU32,
    right_peak: AtomicU32,
    // smoothed RMS
    left_rms:   AtomicU32,
    right_rms:  AtomicU32,
    // peak hold value + timestamp (nanos, relative to meter creation)
    left_hold_val:  AtomicU32,
    right_hold_val: AtomicU32,
    left_hold_ns:   AtomicI64,
    right_hold_ns:  AtomicI64,

    created_at:       Instant,
    hold_duration_ns: i64, // nanos
    decay_rate:       f32,
}

impl VUMeter {
    pub fn new() -> Self {
        Self {
            left_peak:      AtomicU32::new(0.0f32.to_bits()),
            right_peak:     AtomicU32::new(0.0f32.to_bits()),
            left_rms:       AtomicU32::new(0.0f32.to_bits()),
            right_rms:      AtomicU32::new(0.0f32.to_bits()),
            left_hold_val:  AtomicU32::new(0.0f32.to_bits()),
            right_hold_val: AtomicU32::new(0.0f32.to_bits()),
            left_hold_ns:   AtomicI64::new(0),
            right_hold_ns:  AtomicI64::new(0),
            created_at:     Instant::now(),
            hold_duration_ns: Duration::from_secs(3).as_nanos() as i64,
            decay_rate:     0.95,
        }
    }

    /// Called from the realtime audio callback — zero locks, zero allocations.
    pub fn update(&self, left: &[f32], right: &[f32]) {
        // Peak
        let peak_l = left.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let peak_r = right.iter().map(|s| s.abs()).fold(0.0f32, f32::max);

        // Decay then clamp new peak up
        let decayed_l = (load_f32(&self.left_peak) * self.decay_rate).max(peak_l);
        let decayed_r = (load_f32(&self.right_peak) * self.decay_rate).max(peak_r);
        store_f32(&self.left_peak, decayed_l);
        store_f32(&self.right_peak, decayed_r);

        // RMS smoothing
        const RMS_SMOOTH: f32 = 0.8;
        let rms_l = if !left.is_empty() {
            (left.iter().map(|s| s * s).sum::<f32>() / left.len() as f32).sqrt()
        } else { 0.0 };
        let rms_r = if !right.is_empty() {
            (right.iter().map(|s| s * s).sum::<f32>() / right.len() as f32).sqrt()
        } else { 0.0 };
        store_f32(&self.left_rms,  load_f32(&self.left_rms)  * RMS_SMOOTH + rms_l * (1.0 - RMS_SMOOTH));
        store_f32(&self.right_rms, load_f32(&self.right_rms) * RMS_SMOOTH + rms_r * (1.0 - RMS_SMOOTH));

        // Peak hold using elapsed nanos since creation (single Instant::now() cost avoided)
        let now_ns = self.created_at.elapsed().as_nanos() as i64;

        let update_hold = |peak: f32, hold_val: &AtomicU32, hold_ns: &AtomicI64| {
            let elapsed = now_ns - hold_ns.load(Ordering::Relaxed);
            if peak > load_f32(hold_val) || elapsed > self.hold_duration_ns {
                store_f32(hold_val, peak);
                hold_ns.store(now_ns, Ordering::Relaxed);
            }
        };
        update_hold(peak_l, &self.left_hold_val,  &self.left_hold_ns);
        update_hold(peak_r, &self.right_hold_val, &self.right_hold_ns);
    }

    /// Called from the UI thread — snapshot of current values, never blocks audio.
    pub fn get_data(&self) -> VUData {
        VUData {
            left: VUChannel {
                peak:      load_f32(&self.left_peak),
                peak_hold: load_f32(&self.left_hold_val),
                rms:       load_f32(&self.left_rms),
            },
            right: VUChannel {
                peak:      load_f32(&self.right_peak),
                peak_hold: load_f32(&self.right_hold_val),
                rms:       load_f32(&self.right_rms),
            },
        }
    }
}

impl Default for VUMeter {
    fn default() -> Self { Self::new() }
}

/// Convert linear amplitude to decibels
///
/// # Arguments
/// * `linear` - Linear amplitude (0.0 - 1.0)
///
/// # Returns
/// Decibels (-∞ to 0 dB), clamped to -60 dB minimum
#[allow(dead_code)]
pub fn to_db(linear: f32) -> f32 {
    const SILENCE_THRESHOLD: f32 = 0.00001; // -100 dB
    if linear > SILENCE_THRESHOLD {
        (20.0 * linear.log10()).max(-60.0)
    } else {
        f32::NEG_INFINITY
    }
}

/// Convert decibels to linear amplitude
#[allow(dead_code)]
pub fn from_db(db: f32) -> f32 {
    if db.is_finite() {
        10.0f32.powf(db / 20.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vu_meter_peak_detection() {
        let vu = VUMeter::new();
        
        // Silence
        let silence = vec![0.0; 512];
        vu.update(&silence, &silence);
        let data = vu.get_data();
        assert!(data.left.peak < 0.001);
        assert!(data.right.peak < 0.001);
        
        // Full scale
        let full = vec![1.0; 512];
        vu.update(&full, &full);
        let data = vu.get_data();
        assert!(data.left.peak > 0.99);
        assert!(data.right.peak > 0.99);
    }
    
    #[test]
    fn test_db_conversion() {
        assert_eq!(to_db(1.0), 0.0); // Full scale = 0 dB
        assert!(to_db(0.5) > -7.0 && to_db(0.5) < -5.0); // ~-6 dB
        assert!(to_db(0.0).is_infinite() && to_db(0.0).is_sign_negative());
        
        assert_eq!(from_db(0.0), 1.0);
        assert!((from_db(-6.0) - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_peak_hold() {
        let vu = VUMeter::new();
        
        // Send peak
        let peak = vec![0.8; 256];
        vu.update(&peak, &peak);
        let data = vu.get_data();
        assert!(data.left.peak_hold > 0.75);
        
        // Send silence - peak hold should remain
        let silence = vec![0.0; 256];
        vu.update(&silence, &silence);
        let data = vu.get_data();
        assert!(data.left.peak_hold > 0.75, "Peak hold should persist");
    }
}
