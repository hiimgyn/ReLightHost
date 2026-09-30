use crate::{restore_session_impl, AppState, SessionRestoreResult};

/// VST3 plugin paths (file system paths, not instance ids — instances don't
/// exist yet) that were still active when the previous run ended uncleanly.
/// Empty on a clean start. The frontend calls this once after mount to show
/// a one-time "this plugin may have crashed the app last time" warning.
#[tauri::command]
pub fn get_startup_crash_warning() -> Vec<String> {
    crate::core::crash_marker::startup_warning()
}

/// On the plugin host thread (see core::host_thread): a restore can take
/// many seconds (plugins loading their own models) and used to freeze the
/// window for all of it.
#[tauri::command]
pub async fn restore_session(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<'_, AppState>,
) -> Result<SessionRestoreResult, String> {
    let app_state = state.inner().clone();
    crate::core::host_thread::run(move || restore_session_on_host(app_state, app)).await
}

fn restore_session_on_host(
    app_state: AppState,
    app: tauri::AppHandle<tauri::Wry>,
) -> Result<SessionRestoreResult, String> {
    log::info!(
        "{} restore_session command invoked",
        crate::core::threading::thread_prefix("cmd/session")
    );
    let result = restore_session_impl(&app_state, &app);
    log::info!(
        "{} restore_session command finished",
        crate::core::threading::thread_prefix("cmd/session")
    );
    result
}

// ── Named presets ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_presets(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.preset_manager.list_user_presets()
}

/// Saves the current chain (plugins, bypass, parameters, state) under `name`,
/// overwriting a preset of the same name.
#[tauri::command]
pub async fn save_preset(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || save_preset_on_host(&state, name)).await
}

fn save_preset_on_host(state: &AppState, name: String) -> Result<(), String> {
    crate::domain::preset::PresetManager::validate_name(&name).map_err(|e| e.to_string())?;
    let preset = crate::core::snapshot::build_chain_preset_from_manager(&state.plugin_manager, name.trim());
    let json = preset.to_json().map_err(|e| e.to_string())?;
    state
        .preset_manager
        .save_preset_json(name.trim(), &json)
        .map_err(|e| format!("Failed to save preset: {e}"))?;
    Ok(())
}

/// Replaces the chain with the preset's plugins. Returns how many loaded.
/// Runs on the plugin host thread like `load_plugin`, so plugins are always
/// created on the same thread.
#[tauri::command]
pub async fn load_preset(state: tauri::State<'_, AppState>, name: String) -> Result<usize, String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || load_preset_on_host(&state, name)).await
}

fn load_preset_on_host(state: &AppState, name: String) -> Result<usize, String> {
    crate::domain::preset::PresetManager::validate_name(&name).map_err(|e| e.to_string())?;
    if !state.startup.vst3_restore_ready.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Session restore is still finishing — try again in a moment".into());
    }
    let preset = state.preset_manager.load_preset(name.trim()).map_err(|e| e.to_string())?;
    let rate = state.audio_manager.processing_rate();
    let block = state.audio_manager.get_config().buffer_size as usize;
    let parallel = state.config_manager.get_parallel_vst3_loading();
    let loaded = crate::core::session::load_preset_into_chain(&state.plugin_manager, &preset, rate, block, parallel);
    crate::app_events::emit_plugin_chain_changed("preset_load", None);
    Ok(loaded)
}

#[tauri::command]
pub fn delete_preset(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    state
        .preset_manager
        .delete_preset(&name)
        .map_err(|e| format!("Failed to delete preset: {e}"))
}
