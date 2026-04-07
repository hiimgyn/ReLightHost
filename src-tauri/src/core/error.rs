use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Preset error: {0}")]
    Preset(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
