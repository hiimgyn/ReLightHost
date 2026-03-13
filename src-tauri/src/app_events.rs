use std::sync::OnceLock;

use tauri::{AppHandle, Emitter, Wry};

static APP_HANDLE: OnceLock<AppHandle<Wry>> = OnceLock::new();

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginChainEvent {
    pub reason: String,
    pub instance_id: Option<String>,
}

pub fn init_app_handle(app: AppHandle<Wry>) {
    let _ = APP_HANDLE.set(app);
}

pub fn emit_plugin_chain_changed(reason: &str, instance_id: Option<&str>) {
    let payload = PluginChainEvent {
        reason: reason.to_string(),
        instance_id: instance_id.map(|s| s.to_string()),
    };

    if let Some(app) = APP_HANDLE.get() {
        if let Err(e) = app.emit("plugin-chain-changed", payload) {
            log::warn!("Failed to emit plugin-chain-changed event: {}", e);
        }
    }
}
