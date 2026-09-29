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

/// Runs `f` on the app's main (UI) thread. Before the app is up (tests,
/// early startup) it runs inline instead.
pub fn run_on_main_thread(f: impl FnOnce() + Send + 'static) {
    match APP_HANDLE.get() {
        Some(app) => {
            if let Err(e) = app.run_on_main_thread(f) {
                log::warn!("run_on_main_thread failed: {e}");
            }
        }
        None => f(),
    }
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

    crate::core::autosave::request_plugin_chain_autosave();
}
