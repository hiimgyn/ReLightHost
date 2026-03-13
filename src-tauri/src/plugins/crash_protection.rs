use std::panic;
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Plugin status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum PluginStatus {
    Ok,
    Crashed(String),
    Timeout(Duration),
    Error(String),
}

/// Crash protection state
#[derive(Debug, Clone, Serialize)]
pub struct CrashProtection {
    pub status: PluginStatus,
    pub crash_count: usize,
    #[serde(skip)]
    pub last_crash_time: Option<Instant>,
}

impl CrashProtection {
    pub fn new() -> Self {
        Self {
            status: PluginStatus::Ok,
            crash_count: 0,
            last_crash_time: None,
        }
    }

    pub fn mark_crashed(&mut self, reason: String) {
        log::error!("💥 Plugin crashed: {}", reason);
        self.status = PluginStatus::Crashed(reason);
        self.crash_count += 1;
        self.last_crash_time = Some(Instant::now());
    }

    pub fn reset(&mut self) {
        log::info!("✅ Plugin status reset");
        self.status = PluginStatus::Ok;
        // Keep crash_count for statistics
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self.status, PluginStatus::Ok)
    }

    /// Attempt automatic recovery after a crash with cooldown/rate limiting.
    /// Returns true when recovery succeeded and processing may resume.
    pub fn try_auto_recover(&mut self) -> bool {
        // Do not enter an infinite crash loop for unstable plugins.
        if !self.should_auto_restart() {
            return false;
        }

        // Give plugin internals time to settle before re-enabling processing.
        const RECOVERY_COOLDOWN: Duration = Duration::from_secs(2);
        if let Some(last_crash) = self.last_crash_time {
            if last_crash.elapsed() < RECOVERY_COOLDOWN {
                return false;
            }
        }

        if !self.is_healthy() {
            log::warn!("Attempting automatic plugin recovery after crash");
            self.status = PluginStatus::Ok;
            return true;
        }

        true
    }

    #[allow(dead_code)]
    pub fn should_auto_restart(&self) -> bool {
        // Auto-restart if crashed less than 3 times
        self.crash_count < 3
    }
}

impl Default for CrashProtection {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrap unsafe plugin operations with panic catching
pub fn protected_call<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce() -> R + panic::UnwindSafe,
{
    match panic::catch_unwind(f) {
        Ok(result) => Ok(result),
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            Err(format!("Plugin panicked: {}", msg))
        }
    }
}

/// Shared crash protection state
pub type SharedCrashProtection = Arc<Mutex<CrashProtection>>;

/// Create a new shared crash protection instance
pub fn create_shared() -> SharedCrashProtection {
    Arc::new(Mutex::new(CrashProtection::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_protection_lifecycle() {
        let mut protection = CrashProtection::new();
        assert!(protection.is_healthy());
        assert_eq!(protection.crash_count, 0);

        protection.mark_crashed("Test crash".to_string());
        assert!(!protection.is_healthy());
        assert_eq!(protection.crash_count, 1);

        protection.reset();
        assert!(protection.is_healthy());
        assert_eq!(protection.crash_count, 1); // Preserved for statistics
    }

    #[test]
    fn test_protected_call_success() {
        let result = protected_call(|| {
            42
        });
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_protected_call_panic() {
        let result = protected_call(|| {
            panic!("Test panic");
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Test panic"));
    }

    #[test]
    fn test_auto_restart_threshold() {
        let mut protection = CrashProtection::new();
        
        protection.mark_crashed("Crash 1".to_string());
        assert!(protection.should_auto_restart());
        
        protection.mark_crashed("Crash 2".to_string());
        assert!(protection.should_auto_restart());
        
        protection.mark_crashed("Crash 3".to_string());
        assert!(!protection.should_auto_restart()); // Too many crashes
    }

    #[test]
    fn test_auto_recover_after_cooldown() {
        let mut protection = CrashProtection::new();
        protection.mark_crashed("Crash".to_string());

        // Immediate recovery should be blocked by cooldown.
        assert!(!protection.try_auto_recover());

        // Simulate cooldown elapsed.
        protection.last_crash_time = Some(Instant::now() - Duration::from_secs(3));
        assert!(protection.try_auto_recover());
        assert!(protection.is_healthy());
    }
}
