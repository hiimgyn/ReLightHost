use crate::AppState;
use log::info;

#[tauri::command]
pub fn get_custom_scan_paths(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    Ok(state.config_manager.read().get_custom_paths())
}

#[tauri::command]
pub fn add_custom_scan_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state
        .config_manager
        .read()
        .add_custom_path(path)
        .map_err(|e| format!("Failed to add custom path: {}", e))
}

#[tauri::command]
pub fn remove_custom_scan_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state
        .config_manager
        .read()
        .remove_custom_path(&path)
        .map_err(|e| format!("Failed to remove custom path: {}", e))
}

#[tauri::command]
pub fn get_minimize_to_tray(state: tauri::State<AppState>) -> bool {
    state.config_manager.read().get_minimize_to_tray()
}

#[tauri::command]
pub fn set_minimize_to_tray(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .read()
        .set_minimize_to_tray(enabled)
        .map_err(|e| format!("Failed to save minimize_to_tray: {}", e))
    .map(|_| info!("Setting updated: minimize_to_tray={enabled}"))
}

#[tauri::command]
pub fn get_show_app_on_startup(state: tauri::State<AppState>) -> bool {
    state.config_manager.read().get_show_app_on_startup()
}

#[tauri::command]
pub fn set_show_app_on_startup(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .read()
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
