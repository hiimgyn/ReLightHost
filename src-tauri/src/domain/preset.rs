use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;

use crate::plugins::types::{PluginInstanceInfo, PluginFormat};

const AUTO_SAVE_PRESET_NAME: &str = "autosave";

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
    /// Serialized as base64 (older versions wrote a JSON number array —
    /// still accepted on load).
    #[serde(default, skip_serializing_if = "Option::is_none", with = "state_blob")]
    pub vst3_state: Option<Vec<u8>>,
}

/// Plugin state blobs as base64 strings: a JSON number array costs up to
/// 4 bytes of text per byte (8+ pretty-printed, one number per line).
mod state_blob {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(bytes) => s.serialize_str(&STANDARD.encode(bytes)),
            None => s.serialize_none(),
        }
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Blob {
        Base64(String),
        Legacy(Vec<u8>),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        match Option::<Blob>::deserialize(d)? {
            None => Ok(None),
            Some(Blob::Legacy(bytes)) => Ok(Some(bytes)),
            Some(Blob::Base64(text)) => STANDARD.decode(text).map(Some).map_err(serde::de::Error::custom),
        }
    }
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

    /// Compact JSON — what gets written to disk (and hashed by autosave).
    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let preset: Preset = serde_json::from_str(&json)?;
        Ok(preset)
    }
}

/// Write to a temp file then rename over the target — rename is atomic on
/// the same volume, so a crash/power-loss mid-write can never leave a
/// truncated/corrupt file behind.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
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

    /// Manager rooted at an explicit directory (tests).
    #[cfg(test)]
    pub(crate) fn with_dir(presets_dir: PathBuf) -> Self {
        Self { presets_dir }
    }

    fn get_presets_directory() -> Result<PathBuf> {
        let mut path = dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not locate local data directory"))?;
        
        path.push("ReLightHost");
        path.push("presets");
        
        Ok(path)
    }

    /// Writes an already-serialized preset (see `Preset::to_json`).
    pub fn save_preset_json(&self, name: &str, json: &[u8]) -> Result<PathBuf> {
        let path = self.presets_dir.join(format!("{}.json", name.replace(' ', "_")));
        write_atomic(&path, json)?;
        Ok(path)
    }

    pub fn load_preset(&self, name: &str) -> Result<Preset> {
        let filename = format!("{}.json", name.replace(' ', "_"));
        let path = self.presets_dir.join(filename);
        
        if !path.exists() {
            return Err(anyhow::anyhow!("Preset not found: {}", name));
        }

        let preset = Preset::load_from_file(&path)?;
        log::info!("{} Preset loaded: {}", crate::core::threading::thread_prefix("preset/load"), name);
        
        Ok(preset)
    }

}

impl Default for PresetManager {
    fn default() -> Self {
        match Self::new() {
            Ok(m) => m,
            Err(e) => {
                log::error!("Failed to create PresetManager: {}. Falling back to current directory.", e);
                let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                Self { presets_dir: dir }
            }
        }
    }
}
impl PresetManager {
    /// Restore the last auto-saved session
    pub fn restore_auto_save(&self) -> Result<Preset> {
        self.load_preset(AUTO_SAVE_PRESET_NAME)
    }
}

// Simple chrono replacement for timestamp
mod chrono {
    use std::time::{Duration, SystemTime};

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
                .unwrap_or(Duration::ZERO);

            format!("{}", duration.as_secs())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_with_state(state: Option<Vec<u8>>) -> PresetPlugin {
        PresetPlugin {
            plugin_id: "vst3::x".into(),
            plugin_name: "X".into(),
            plugin_vendor: None,
            plugin_version: None,
            plugin_path: None,
            plugin_format: None,
            plugin_category: None,
            bypassed: false,
            parameters: vec![],
            vst3_state: state,
        }
    }

    #[test]
    fn preset_files_are_written_compact() {
        let dir = std::env::temp_dir().join(format!("rh-preset-compact-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut preset = Preset::new("compact".into(), vec![]);
        preset.plugin_chain.push(plugin_with_state(Some(vec![1; 64])));
        let path = PresetManager::with_dir(dir.clone()).save_preset_json(&preset.name, &preset.to_json().unwrap()).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains('\n'), "pretty-printed preset: {} bytes", text.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_is_stored_as_a_compact_base64_string() {
        let json = serde_json::to_string(&plugin_with_state(Some(vec![0, 1, 2, 255]))).unwrap();
        assert!(json.contains(r#""vst3_state":"AAEC/w==""#), "{json}");
    }

    #[test]
    fn state_saved_by_older_versions_as_a_number_array_still_loads() {
        let mut json = serde_json::to_value(plugin_with_state(None)).unwrap();
        json["vst3_state"] = serde_json::json!([0, 1, 2, 255]);
        let p: PresetPlugin = serde_json::from_value(json).unwrap();
        assert_eq!(p.vst3_state, Some(vec![0, 1, 2, 255]));
    }

    #[test]
    fn base64_state_round_trips_and_absent_state_stays_absent() {
        let p = plugin_with_state(Some(vec![9; 1000]));
        let back: PresetPlugin = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.vst3_state, p.vst3_state);
        let none: PresetPlugin = serde_json::from_str(&serde_json::to_string(&plugin_with_state(None)).unwrap()).unwrap();
        assert_eq!(none.vst3_state, None);
    }
}
