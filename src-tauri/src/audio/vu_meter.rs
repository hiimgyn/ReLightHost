use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

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

/// Peak hold with timestamp
#[derive(Debug, Clone, Copy)]
struct PeakHold {
    value: f32,
    timestamp: Instant,
}

/// VU Meter with peak and RMS detection
///
/// Inspired by rust-vst3-host's metering system:
/// - Peak detection for transients
/// - Peak hold with 3-second decay
/// - RMS for average loudness perception
pub struct VUMeter {
    left_peak: Arc<Mutex<f32>>,
    right_peak: Arc<Mutex<f32>>,
    left_rms: Arc<Mutex<f32>>,
    right_rms: Arc<Mutex<f32>>,
    left_hold: Arc<Mutex<PeakHold>>,
    right_hold: Arc<Mutex<PeakHold>>,
    
    /// Peak hold duration (default: 3 seconds)
    hold_duration: Duration,
    /// Peak decay rate per update (0.0 - 1.0)
    /// 0.95 = 5% decay per frame
    decay_rate: f32,
}

impl VUMeter {
    pub fn new() -> Self {
        Self {
            left_peak: Arc::new(Mutex::new(0.0)),
            right_peak: Arc::new(Mutex::new(0.0)),
            left_rms: Arc::new(Mutex::new(0.0)),
            right_rms: Arc::new(Mutex::new(0.0)),
            left_hold: Arc::new(Mutex::new(PeakHold {
                value: 0.0,
                timestamp: Instant::now(),
            })),
            right_hold: Arc::new(Mutex::new(PeakHold {
                value: 0.0,
                timestamp: Instant::now(),
            })),
            hold_duration: Duration::from_secs(3),
            decay_rate: 0.95,
        }
    }
    
    /// Update VU meter with audio buffer (non-interleaved stereo)
    ///
    /// # Arguments
    /// * `left` - Left channel samples
    /// * `right` - Right channel samples
    pub fn update(&self, left: &[f32], right: &[f32]) {
        // Calculate peak values
        let peak_l = left.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let peak_r = right.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        
        // Calculate RMS values (average energy)
        let rms_l = if !left.is_empty() {
            let sum_squares: f32 = left.iter().map(|s| s * s).sum();
            (sum_squares / left.len() as f32).sqrt()
        } else {
            0.0
        };
        
        let rms_r = if !right.is_empty() {
            let sum_squares: f32 = right.iter().map(|s| s * s).sum();
            (sum_squares / right.len() as f32).sqrt()
        } else {
            0.0
        };
        
        // Update peaks with decay
        let mut left_peak = self.left_peak.lock();
        let mut right_peak = self.right_peak.lock();
        *left_peak = (*left_peak * self.decay_rate).max(peak_l);
        *right_peak = (*right_peak * self.decay_rate).max(peak_r);
        
        // Update RMS with smoothing
        const RMS_SMOOTHING: f32 = 0.8; // Slower response for RMS
        let mut left_rms = self.left_rms.lock();
        let mut right_rms = self.right_rms.lock();
        *left_rms = (*left_rms * RMS_SMOOTHING) + (rms_l * (1.0 - RMS_SMOOTHING));
        *right_rms = (*right_rms * RMS_SMOOTHING) + (rms_r * (1.0 - RMS_SMOOTHING));
        
        // Update peak hold
        let now = Instant::now();
        
        let mut left_hold = self.left_hold.lock();
        if peak_l > left_hold.value || now.duration_since(left_hold.timestamp) > self.hold_duration {
            *left_hold = PeakHold { value: peak_l, timestamp: now };
        }
        
        let mut right_hold = self.right_hold.lock();
        if peak_r > right_hold.value || now.duration_since(right_hold.timestamp) > self.hold_duration {
            *right_hold = PeakHold { value: peak_r, timestamp: now };
        }
    }
    
    /// Get current VU meter data
    pub fn get_data(&self) -> VUData {
        VUData {
            left: VUChannel {
                peak: *self.left_peak.lock(),
                peak_hold: self.left_hold.lock().value,
                rms: *self.left_rms.lock(),
            },
            right: VUChannel {
                peak: *self.right_peak.lock(),
                peak_hold: self.right_hold.lock().value,
                rms: *self.right_rms.lock(),
            },
        }
    }
    
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
