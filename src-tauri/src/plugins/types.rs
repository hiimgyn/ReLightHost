use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "manufacture")]
    pub vendor: String,
    pub version: String,
    pub path: String,
    pub format: PluginFormat,
    pub category: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginFormat {
    #[serde(rename = "clap")]
    CLAP,
    #[serde(rename = "vst3")]
    VST3,
    #[serde(rename = "vst")]
    VST,
}

impl PluginFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "clap" => Some(Self::CLAP),
            "vst3" => Some(Self::VST3),
            "dll" | "vst" => Some(Self::VST), // VST2 plugins (Windows .dll)
            "dylib" => Some(Self::VST),       // VST2 plugins (macOS)
            "so" => Some(Self::VST),          // VST2 plugins (Linux)
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginParameter {
    pub id: u32,
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstanceInfo {
    pub instance_id: String,
    pub plugin_id: String,
    pub name: String,
    #[serde(rename = "manufacture")]
    pub vendor: String,
    pub version: String,
    pub path: String,
    pub format: PluginFormat,
    pub category: String,
    pub bypassed: bool,
    pub parameters: Vec<PluginParameter>,
}
