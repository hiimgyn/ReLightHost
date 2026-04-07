use crate::AppState;

pub(crate) fn is_startup_enabled_inner() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::{enums::*, RegKey};
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_READ) {
            Ok(run_key) => match run_key.get_value::<String, _>("ReLightHost") {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            },
            Err(e) => Err(format!("Failed to open registry key: {e}")),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

pub(crate) fn toggle_startup_inner(enable: bool, state: &AppState) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::env;
        use winreg::{enums::*, RegKey};

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                KEY_SET_VALUE | KEY_READ,
            )
            .map_err(|e| format!("Failed to open registry key: {e}"))?;

        if enable {
            let exe = env::current_exe().map_err(|e| format!("Failed to get exe path: {e}"))?;
            let show = state.config_manager.read().get_show_app_on_startup();
            let cmd = if show {
                format!("\"{}\"", exe.display())
            } else {
                format!("\"{}\" --start-hidden", exe.display())
            };
            run_key
                .set_value("ReLightHost", &cmd)
                .map_err(|e| format!("Failed to set registry value: {e}"))
        } else {
            let _ = run_key.delete_value("ReLightHost");
            Ok(())
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (enable, state);
        Ok(())
    }
}

#[tauri::command]
pub fn is_startup_enabled() -> Result<bool, String> {
    is_startup_enabled_inner()
}

#[tauri::command]
pub fn toggle_startup(enable: bool, state: tauri::State<AppState>) -> Result<(), String> {
    toggle_startup_inner(enable, &state)
}
