use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use crate::audio::types::AudioConfig;

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub custom_scan_paths: Vec<String>,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default = "default_true")]
    pub show_app_on_startup: bool,
}

/// Persisted per-session state: audio device config + mute.
/// Saved to session.json alongside config.json whenever audio settings change.
/// Plugin chain is persisted separately via the __autosave__ preset.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub muted: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            custom_scan_paths: Vec::new(),
            minimize_to_tray: false,
            show_app_on_startup: true,
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
        let config = self.config.read().unwrap();
        config.custom_scan_paths.clone()
    }

    pub fn add_custom_path(&self, path: String) -> Result<()> {
        let mut config = self.config.write().unwrap();
        if !config.custom_scan_paths.contains(&path) {
            config.custom_scan_paths.push(path);
            self.save_config(&config)?;
        }
        Ok(())
    }

    pub fn remove_custom_path(&self, path: &str) -> Result<()> {
        let mut config = self.config.write().unwrap();
        config.custom_scan_paths.retain(|p| p != path);
        self.save_config(&config)?;
        Ok(())
    }

    pub fn get_minimize_to_tray(&self) -> bool {
        self.config.read().unwrap().minimize_to_tray
    }

    pub fn set_minimize_to_tray(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write().unwrap();
        config.minimize_to_tray = enabled;
        self.save_config(&config)?;
        Ok(())
    }

    pub fn get_show_app_on_startup(&self) -> bool {
        self.config.read().unwrap().show_app_on_startup
    }

    pub fn set_show_app_on_startup(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write().unwrap();
        config.show_app_on_startup = enabled;
        self.save_config(&config)?;
        Ok(())
    }

    fn save_config(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }

    // ── Session persistence (audio config + mute) ──────────────────────────

    fn session_path(&self) -> PathBuf {
        self.config_path
            .parent()
            .expect("config path must have a parent directory")
            .join("session.json")
    }

    /// Persist the current audio configuration to session.json.
    /// Called after every audio setting change so the state survives restarts.
    pub fn save_session(&self, audio: &AudioConfig, muted: bool) -> Result<()> {
        let state = SessionState { audio: audio.clone(), muted };
        let content = serde_json::to_string_pretty(&state)?;
        fs::write(self.session_path(), content)?;
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
