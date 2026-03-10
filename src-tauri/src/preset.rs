use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;

use crate::plugins::types::{PluginInstanceInfo, PluginFormat};

const AUTO_SAVE_PRESET_NAME: &str = "__autosave__";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub plugin_chain: Vec<PresetPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetPlugin {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_vendor: Option<String>,
    pub plugin_version: Option<String>,
    pub plugin_path: Option<String>,
    pub plugin_format: Option<PluginFormat>,
    pub plugin_category: Option<String>,
    pub bypassed: bool,
    pub parameters: Vec<PresetParameter>,
    /// VST3 binary state blob (from IComponent::getState)
    /// This includes internal plugin data like sample banks, custom presets, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vst3_state: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetParameter {
    pub id: u32,
    pub name: String,
    pub value: f64,
}

impl Preset {
    pub fn new(name: String, plugin_chain: Vec<PluginInstanceInfo>) -> Self {
        let created_at = chrono::Local::now().to_rfc3339();
        
        let preset_plugins = plugin_chain
            .into_iter()
            .map(|instance| PresetPlugin {
                plugin_id: instance.plugin_id,
                plugin_name: instance.name,
                plugin_vendor: Some(instance.vendor),
                plugin_version: Some(instance.version),
                plugin_path: Some(instance.path),
                plugin_format: Some(instance.format),
                plugin_category: Some(instance.category),
                bypassed: instance.bypassed,
                parameters: instance
                    .parameters
                    .into_iter()
                    .map(|p| PresetParameter {
                        id: p.id,
                        name: p.name,
                        value: p.value,
                    })
                    .collect(),
                vst3_state: None, // Will be populated by save_preset command
            })
            .collect();

        Self {
            name,
            description: String::new(),
            created_at,
            plugin_chain: preset_plugins,
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let preset: Preset = serde_json::from_str(&json)?;
        Ok(preset)
    }
}

pub struct PresetManager {
    presets_dir: PathBuf,
}

impl PresetManager {
    pub fn new() -> Result<Self> {
        let presets_dir = Self::get_presets_directory()?;
        fs::create_dir_all(&presets_dir)?;
        
        Ok(Self { presets_dir })
    }

    fn get_presets_directory() -> Result<PathBuf> {
        let mut path = dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not locate local data directory"))?;
        
        path.push("ReLightHost");
        path.push("presets");
        
        Ok(path)
    }

    pub fn save_preset(&self, preset: &Preset) -> Result<PathBuf> {
        let filename = format!("{}.json", preset.name.replace(' ', "_"));
        let path = self.presets_dir.join(filename);
        
        preset.save_to_file(&path)?;
        log::info!("Preset saved: {}", path.display());
        
        Ok(path)
    }

    pub fn load_preset(&self, name: &str) -> Result<Preset> {
        let filename = format!("{}.json", name.replace(' ', "_"));
        let path = self.presets_dir.join(filename);
        
        if !path.exists() {
            return Err(anyhow::anyhow!("Preset not found: {}", name));
        }

        let preset = Preset::load_from_file(&path)?;
        log::info!("Preset loaded: {}", name);
        
        Ok(preset)
    }

    pub fn list_presets(&self) -> Result<Vec<String>> {
        let mut presets = Vec::new();

        if self.presets_dir.exists() {
            for entry in fs::read_dir(&self.presets_dir)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        // Exclude internal auto-save preset
                        if stem != AUTO_SAVE_PRESET_NAME {
                            presets.push(stem.replace('_', " "));
                        }
                    }
                }
            }
        }

        Ok(presets)
    }

    pub fn delete_preset(&self, name: &str) -> Result<()> {
        let filename = format!("{}.json", name.replace(' ', "_"));
        let path = self.presets_dir.join(filename);
        
        if path.exists() {
            fs::remove_file(path)?;
            log::info!("Preset deleted: {}", name);
        }

        Ok(())
    }
}

impl Default for PresetManager {
    fn default() -> Self {
        Self::new().expect("Failed to create PresetManager")
    }
}
impl PresetManager {
    /// Check if an auto-saved session exists
    pub fn has_auto_save(&self) -> bool {
        let filename = format!("{}.json", AUTO_SAVE_PRESET_NAME);
        self.presets_dir.join(filename).exists()
    }

    /// Restore the last auto-saved session
    pub fn restore_auto_save(&self) -> Result<Preset> {
        self.load_preset(AUTO_SAVE_PRESET_NAME)
    }

    /// Clear the auto-save file
    #[allow(dead_code)]
    pub fn clear_auto_save(&self) -> Result<()> {
        self.delete_preset(AUTO_SAVE_PRESET_NAME)
    }
}

// Simple chrono replacement for timestamp
mod chrono {
    use std::time::SystemTime;

    pub struct Local;

    impl Local {
        pub fn now() -> DateTime {
            DateTime
        }
    }

    pub struct DateTime;

    impl DateTime {
        pub fn to_rfc3339(&self) -> String {
            let duration = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap();
            
            format!("{}", duration.as_secs())
        }
    }
}
