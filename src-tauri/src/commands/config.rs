use crate::AppState;
use log::info;

#[tauri::command]
pub fn get_custom_scan_paths(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    Ok(state.config_manager.get_custom_paths())
}

#[tauri::command]
pub fn add_custom_scan_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state
        .config_manager
        .add_custom_path(path)
        .map_err(|e| format!("Failed to add custom path: {}", e))
}

#[tauri::command]
pub fn remove_custom_scan_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state
        .config_manager
        .remove_custom_path(&path)
        .map_err(|e| format!("Failed to remove custom path: {}", e))
}

#[tauri::command]
pub fn get_minimize_to_tray(state: tauri::State<AppState>) -> bool {
    state.config_manager.get_minimize_to_tray()
}

#[tauri::command]
pub fn set_minimize_to_tray(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .set_minimize_to_tray(enabled)
        .map_err(|e| format!("Failed to save minimize_to_tray: {}", e))
    .map(|_| info!("Setting updated: minimize_to_tray={enabled}"))
}

#[tauri::command]
pub fn get_show_app_on_startup(state: tauri::State<AppState>) -> bool {
    state.config_manager.get_show_app_on_startup()
}

#[tauri::command]
pub fn set_show_app_on_startup(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .set_show_app_on_startup(enabled)
        .map_err(|e| format!("Failed to save show_app_on_startup: {}", e))?;

    info!("Setting updated: show_app_on_startup={enabled}");

    // If startup is already enabled, rewrite the Run key immediately so the
    // next OS login uses the new visibility mode.
    if crate::commands::startup::is_startup_enabled_inner(&state)? {
        crate::commands::startup::toggle_startup_inner(true, &state)?;
    }

    Ok(())
}

/// Takes effect the next time plugins are batch-loaded (a fresh
/// `restore_session` or preset load) — not for an already-running session.
#[tauri::command]
pub fn get_parallel_vst3_loading(state: tauri::State<AppState>) -> bool {
    state.config_manager.get_parallel_vst3_loading()
}

#[tauri::command]
pub fn set_parallel_vst3_loading(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .set_parallel_vst3_loading(enabled)
        .map_err(|e| format!("Failed to save parallel_vst3_loading: {}", e))
    .map(|_| info!("Setting updated: parallel_vst3_loading={enabled}"))
}

#[tauri::command]
pub fn get_wasapi_exclusive(state: tauri::State<AppState>) -> bool {
    state.config_manager.get_wasapi_exclusive()
}

/// Restarts a running stream so the new mode applies immediately.
#[tauri::command]
pub async fn set_wasapi_exclusive(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_wasapi_exclusive_on_host(&state, enabled)).await
}

fn set_wasapi_exclusive_on_host(state: &AppState, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .set_wasapi_exclusive(enabled)
        .map_err(|e| format!("Failed to save wasapi_exclusive: {}", e))?;
    let am = &state.audio_manager;
    am.set_wasapi_exclusive(enabled);
    if am.get_status().is_monitoring {
        am.toggle_monitoring(false).map_err(|e| e.to_string())?;
        am.toggle_monitoring(true).map_err(|e| e.to_string())?;
    }
    info!("Setting updated: wasapi_exclusive={enabled}");
    Ok(())
}
