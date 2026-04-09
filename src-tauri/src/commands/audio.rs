use tauri::Manager;

use crate::audio::{AudioConfig, AudioDevice, AudioDeviceInfo, AudioStatus, VUData};
use crate::AppState;

#[tauri::command]
pub fn start_audio(state: tauri::State<AppState>) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .start()
        .map_err(|e| format!("Failed to start audio: {}", e))
}

#[tauri::command]
pub fn stop_audio(state: tauri::State<AppState>) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .stop()
        .map_err(|e| format!("Failed to stop audio: {}", e))
}

#[tauri::command]
pub fn get_audio_status(state: tauri::State<AppState>) -> Result<AudioStatus, String> {
    Ok(state.audio_manager.read().get_status())
}

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    AudioDevice::list_devices().map_err(|e| format!("Failed to list audio devices: {}", e))
}

#[tauri::command]
pub fn get_audio_config(state: tauri::State<AppState>) -> Result<AudioConfig, String> {
    Ok(state.audio_manager.read().get_config())
}

#[tauri::command]
pub fn set_output_device(state: tauri::State<AppState>, device_id: Option<String>) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .set_output_device(device_id)
        .map_err(|e| format!("Failed to set output device: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn set_input_device(state: tauri::State<AppState>, device_id: Option<String>) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .set_input_device(device_id)
        .map_err(|e| format!("Failed to set input device: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn set_virtual_output_device(state: tauri::State<AppState>, device_id: Option<String>) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .set_virtual_output_device(device_id)
        .map_err(|e| format!("Failed to set virtual output device: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn set_sample_rate(state: tauri::State<AppState>, rate: u32) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .set_sample_rate(rate)
        .map_err(|e| format!("Failed to set sample rate: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn set_buffer_size(state: tauri::State<AppState>, size: u32) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .set_buffer_size(size)
        .map_err(|e| format!("Failed to set buffer size: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn toggle_monitoring(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .toggle_monitoring(enabled)
        .map_err(|e| format!("Failed to toggle monitoring: {}", e))
}

#[tauri::command]
pub fn set_muted(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<AppState>,
    muted: bool,
) -> Result<(), String> {
    state.audio_manager.read().set_muted(muted);
    let tray_state = app.state::<crate::TrayState>();
    crate::bootstrap::tray::sync_audio_tray_state(&app, &tray_state, muted);
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn set_loopback(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .audio_manager
        .read()
        .set_loopback(enabled)
        .map_err(|e| format!("Failed to set loopback: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn get_vu_data(state: tauri::State<AppState>) -> Result<VUData, String> {
    Ok(state.audio_manager.read().get_vu_data())
}

#[tauri::command]
pub fn play_test_sound() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("powershell")
            .args([
                "-WindowStyle",
                "Hidden",
                "-Command",
                "[System.Media.SystemSounds]::Beep.Play(); Start-Sleep -Milliseconds 800",
            ])
            .spawn()
            .map_err(|e| format!("Failed to play test sound: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;
        let _ = Command::new("bash")
            .args([
                "-c",
                "paplay /usr/share/sounds/freedesktop/stereo/bell.oga 2>/dev/null || afplay /System/Library/Sounds/Ping.aiff 2>/dev/null",
            ])
            .spawn();
    }
    Ok(())
}
