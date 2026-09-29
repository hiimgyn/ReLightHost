use crate::{restore_session_impl, AppState, SessionRestoreResult};

/// VST3 plugin paths (file system paths, not instance ids — instances don't
/// exist yet) that were still active when the previous run ended uncleanly.
/// Empty on a clean start. The frontend calls this once after mount to show
/// a one-time "this plugin may have crashed the app last time" warning.
#[tauri::command]
pub fn get_startup_crash_warning() -> Vec<String> {
    crate::core::crash_marker::startup_warning()
}

#[tauri::command]
pub fn restore_session(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<'_, AppState>,
) -> Result<SessionRestoreResult, String> {
    let app_state = state.inner().clone();
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
