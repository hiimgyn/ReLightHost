use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use crate::audio::types::AudioConfig;

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub custom_scan_paths: Vec<String>,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default = "default_true")]
    pub show_app_on_startup: bool,
    /// Experimental: load VST3 plugins in parallel during a batch restore
    /// instead of one at a time. Off by default — see
    /// PluginInstanceManager::load_plugins_parallel_results for the
    /// trade-off (faster restore, but a native crash in one plugin can now
    /// happen while others are mid-load instead of at a predictable point).
    #[serde(default)]
    pub parallel_vst3_loading: bool,
    /// Try WASAPI exclusive mode (lower latency) before shared mode. Off by
    /// default: exclusive mode takes the device away from every other app
    /// (no system/Discord/browser audio on that output, mic unavailable to
    /// others).
    #[serde(default)]
    pub wasapi_exclusive: bool,
}

/// Persisted per-session state: audio device config + mute + loopback.
/// Saved to session.json alongside config.json whenever audio settings change.
/// Plugin chain is persisted separately via the autosave preset.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub loopback_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            custom_scan_paths: Vec::new(),
            minimize_to_tray: false,
            show_app_on_startup: true,
            parallel_vst3_loading: false,
            wasapi_exclusive: false,
        }
    }
}

pub struct ConfigManager {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let config_dir = dirs::config_local_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
            .join("ReLightHost");

        fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("config.json");

        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            AppConfig::default()
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        })
    }

    pub fn get_custom_paths(&self) -> Vec<String> {
        let config = self.config.read();
        config.custom_scan_paths.clone()
    }

    pub fn add_custom_path(&self, path: String) -> Result<()> {
        let mut config = self.config.write();
        if !config.custom_scan_paths.contains(&path) {
            config.custom_scan_paths.push(path);
            self.save_config(&config)?;
        }
        Ok(())
    }

    pub fn remove_custom_path(&self, path: &str) -> Result<()> {
        let mut config = self.config.write();
        config.custom_scan_paths.retain(|p| p != path);
        self.save_config(&config)?;
        Ok(())
    }

    pub fn get_minimize_to_tray(&self) -> bool {
        self.config.read().minimize_to_tray
    }

    pub fn set_minimize_to_tray(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write();
        config.minimize_to_tray = enabled;
        self.save_config(&config)?;
        Ok(())
    }

    pub fn get_show_app_on_startup(&self) -> bool {
        self.config.read().show_app_on_startup
    }

    pub fn set_show_app_on_startup(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write();
        config.show_app_on_startup = enabled;
        self.save_config(&config)?;
        Ok(())
    }

    pub fn get_parallel_vst3_loading(&self) -> bool {
        self.config.read().parallel_vst3_loading
    }

    pub fn set_parallel_vst3_loading(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write();
        config.parallel_vst3_loading = enabled;
        self.save_config(&config)?;
        Ok(())
    }

    pub fn get_wasapi_exclusive(&self) -> bool {
        self.config.read().wasapi_exclusive
    }

    pub fn set_wasapi_exclusive(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write();
        config.wasapi_exclusive = enabled;
        self.save_config(&config)?;
        Ok(())
    }

    fn save_config(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        crate::domain::preset::write_atomic(&self.config_path, content.as_bytes())?;
        Ok(())
    }

    // ── Session persistence (audio config + mute) ──────────────────────────

    fn session_path(&self) -> PathBuf {
        self.config_path
            .parent()
            .map(|p| p.join("session.json"))
            .unwrap_or_else(|| std::env::temp_dir().join("session.json"))
    }

}

impl Default for ConfigManager {
    fn default() -> Self {
        let config = AppConfig::default();
        let config_path = std::env::temp_dir().join("relighthost_config.json");

        if let Some(parent) = config_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        }
    }
}

impl ConfigManager {
    /// Persist the current audio configuration to session.json.
    /// Called after every audio setting change so the state survives restarts.
    pub fn save_session(&self, audio: &AudioConfig, muted: bool, loopback_enabled: bool) -> Result<()> {
        let state = SessionState { audio: audio.clone(), muted, loopback_enabled };
        let content = serde_json::to_string_pretty(&state)?;
        crate::domain::preset::write_atomic(&self.session_path(), content.as_bytes())?;
        Ok(())
    }

    /// Load the last saved session state, or None if none exists yet.
    pub fn load_session(&self) -> Option<SessionState> {
        let path = self.session_path();
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch config path under the OS temp dir, unique per test run, so
    /// tests never collide with the real config.json or with each other.
    struct ScratchConfig(PathBuf);
    impl Drop for ScratchConfig {
        fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
    }
    fn scratch_manager() -> (ScratchConfig, ConfigManager) {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("relighthost_config_test_{n}.json"));
        let manager = ConfigManager {
            config: Arc::new(RwLock::new(AppConfig::default())),
            config_path: path.clone(),
        };
        (ScratchConfig(path), manager)
    }

    #[test]
    fn parallel_vst3_loading_defaults_to_false() {
        let (_scratch, manager) = scratch_manager();
        assert!(!manager.get_parallel_vst3_loading());
    }

    #[test]
    fn parallel_vst3_loading_round_trips_through_disk() {
        let (scratch, manager) = scratch_manager();
        manager.set_parallel_vst3_loading(true).expect("save should succeed");
        assert!(manager.get_parallel_vst3_loading());

        let content = fs::read_to_string(&scratch.0).expect("config file should exist");
        let reloaded: AppConfig = serde_json::from_str(&content).expect("should parse");
        assert!(reloaded.parallel_vst3_loading);
    }

    #[test]
    fn wasapi_exclusive_defaults_to_shared_and_round_trips() {
        let (scratch, manager) = scratch_manager();
        assert!(!manager.get_wasapi_exclusive());
        manager.set_wasapi_exclusive(true).expect("save should succeed");
        let content = fs::read_to_string(&scratch.0).expect("config file should exist");
        let reloaded: AppConfig = serde_json::from_str(&content).expect("should parse");
        assert!(reloaded.wasapi_exclusive);
        let old: AppConfig = serde_json::from_str(r#"{"custom_scan_paths":[]}"#).unwrap();
        assert!(!old.wasapi_exclusive, "configs saved before this setting existed stay shared");
    }
}
