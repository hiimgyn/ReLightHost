use crate::AppState;
use log::info;

fn create_auto_launch(show_app_on_startup: bool) -> Result<auto_launch::AutoLaunch, String> {
    use std::env;

    let exe = env::current_exe().map_err(|e| format!("Failed to get exe path: {e}"))?;
    let exe = exe
        .to_str()
        .ok_or_else(|| "Failed to convert exe path to UTF-8".to_string())?;
    let args: &[&str] = if show_app_on_startup {
        &[]
    } else {
        &["--start-hidden"]
    };

    let mut builder = auto_launch::AutoLaunchBuilder::new();
    builder
        .set_app_name("ReLightHost")
        .set_app_path(exe)
        .set_args(args);

    builder
        .build()
        .map_err(|e| format!("Failed to create auto-launch config: {e}"))
}

pub(crate) fn is_startup_enabled_inner(state: &AppState) -> Result<bool, String> {
    let show_app_on_startup = state.config_manager.read().get_show_app_on_startup();
    let auto_launch = create_auto_launch(show_app_on_startup)?;
    auto_launch
        .is_enabled()
        .map_err(|e| format!("Failed to read startup state: {e}"))
}

pub(crate) fn toggle_startup_inner(enable: bool, state: &AppState) -> Result<(), String> {
    let show_app_on_startup = state.config_manager.read().get_show_app_on_startup();
    let auto_launch = create_auto_launch(show_app_on_startup)?;

    if enable {
        auto_launch
            .enable()
            .map_err(|e| format!("Failed to enable startup: {e}"))
            .map(|_| info!("Startup setting updated: enabled=true, show_app_on_startup={show_app_on_startup}"))
    } else {
        auto_launch
            .disable()
            .map_err(|e| format!("Failed to disable startup: {e}"))
            .map(|_| info!("Startup setting updated: enabled=false, show_app_on_startup={show_app_on_startup}"))
    }
}

#[tauri::command]
pub fn is_startup_enabled(state: tauri::State<AppState>) -> Result<bool, String> {
    is_startup_enabled_inner(&state)
}

#[tauri::command]
pub fn toggle_startup(enable: bool, state: tauri::State<AppState>) -> Result<(), String> {
    toggle_startup_inner(enable, &state)
}
