use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub custom_scan_paths: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            custom_scan_paths: Vec::new(),
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

    fn save_config(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }
}
