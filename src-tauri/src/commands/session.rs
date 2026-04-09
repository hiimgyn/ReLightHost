use crate::{restore_session_impl, AppState, SessionRestoreResult};

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
