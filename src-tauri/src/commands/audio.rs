use tauri::Manager;
use std::sync::Mutex;
use std::process::Child;

use crate::audio::{AudioConfig, AudioDevice, AudioDeviceInfo, AudioStatus, VUData};
use crate::AppState;

static TEST_SOUND_PROCESS: Mutex<Option<Child>> = Mutex::new(None);

/// Reloads the plugin chain if the running stream's sample rate differs
/// from the one the plugins were created at (config change, or an ASIO
/// driver that refused the configured rate). Skipped while a session
/// restore is still replaying VST3 state onto the current instances.
pub(crate) fn sync_chain_to_audio_rate(state: &AppState) {
    if !state.startup.vst3_restore_ready.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    // Only against a running stream: while stopped the rate that will
    // actually run isn't known yet (an ASIO driver may override it), and
    // reloading now would just mean reloading again on start.
    let Some(rate) = state.audio_manager.running_rate() else { return };
    let block = state.audio_manager.get_config().buffer_size as usize;
    if state.plugin_manager.reprepare_if_rate_changed(rate, block) {
        crate::app_events::emit_plugin_chain_changed("reload", None);
    }
}

#[tauri::command]
pub async fn start_audio(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || start_audio_on_host(&state)).await
}

fn start_audio_on_host(state: &AppState) -> Result<(), String> {
    state
        .audio_manager
        .start()
        .map_err(|e| format!("Failed to start audio: {}", e))
}

#[tauri::command]
pub async fn stop_audio(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || stop_audio_on_host(&state)).await
}

fn stop_audio_on_host(state: &AppState) -> Result<(), String> {
    state
        .audio_manager
        .stop()
        .map_err(|e| format!("Failed to stop audio: {}", e))
}

#[tauri::command]
pub async fn get_audio_status(state: tauri::State<'_, AppState>) -> Result<AudioStatus, String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || get_audio_status_on_host(&state)).await
}

fn get_audio_status_on_host(state: &AppState) -> Result<AudioStatus, String> {
    let mut status = state.audio_manager.get_status();
    // get_status may just have rebuilt the stream after an ASIO reset at a
    // different rate.
    sync_chain_to_audio_rate(state);
    let rate = state.audio_manager.processing_rate();
    status.plugin_latency_ms = state.plugin_manager.chain_latency_samples() as f32 / rate as f32 * 1000.0;
    Ok(status)
}

/// On the plugin host thread: listing loads every ASIO driver briefly, and
/// ASIO lifecycle calls stay on the thread that runs the streams.
#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    crate::core::host_thread::run(|| {
        AudioDevice::list_devices().map_err(|e| format!("Failed to list audio devices: {}", e))
    })
    .await
}

#[tauri::command]
pub fn get_audio_config(state: tauri::State<AppState>) -> Result<AudioConfig, String> {
    Ok(state.audio_manager.get_config())
}

#[tauri::command]
pub async fn set_output_device(state: tauri::State<'_, AppState>, device_id: Option<String>) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_output_device_on_host(&state, device_id)).await
}

fn set_output_device_on_host(state: &AppState, device_id: Option<String>) -> Result<(), String> {
    state
        .audio_manager
        .set_output_device(device_id)
        .map_err(|e| format!("Failed to set output device: {}", e))?;
    sync_chain_to_audio_rate(state);
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn set_input_device(state: tauri::State<'_, AppState>, device_id: Option<String>) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_input_device_on_host(&state, device_id)).await
}

fn set_input_device_on_host(state: &AppState, device_id: Option<String>) -> Result<(), String> {
    state
        .audio_manager
        .set_input_device(device_id)
        .map_err(|e| format!("Failed to set input device: {}", e))?;
    sync_chain_to_audio_rate(state);
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn set_virtual_output_device(state: tauri::State<'_, AppState>, device_id: Option<String>) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_virtual_output_device_on_host(&state, device_id)).await
}

fn set_virtual_output_device_on_host(state: &AppState, device_id: Option<String>) -> Result<(), String> {
    state
        .audio_manager
        .set_virtual_output_device(device_id)
        .map_err(|e| format!("Failed to set virtual output device: {}", e))?;
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn set_input_channel_offset(state: tauri::State<'_, AppState>, offset: usize) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_input_channel_offset_on_host(&state, offset)).await
}

fn set_input_channel_offset_on_host(state: &AppState, offset: usize) -> Result<(), String> {
    state
        .audio_manager
        .set_input_channel_offset(offset)
        .map_err(|e| format!("Failed to set input channel: {}", e))?;
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn set_output_channel_offset(state: tauri::State<'_, AppState>, offset: usize) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_output_channel_offset_on_host(&state, offset)).await
}

fn set_output_channel_offset_on_host(state: &AppState, offset: usize) -> Result<(), String> {
    state
        .audio_manager
        .set_output_channel_offset(offset)
        .map_err(|e| format!("Failed to set output channel: {}", e))?;
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn set_sample_rate(state: tauri::State<'_, AppState>, rate: u32) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_sample_rate_on_host(&state, rate)).await
}

fn set_sample_rate_on_host(state: &AppState, rate: u32) -> Result<(), String> {
    state
        .audio_manager
        .set_sample_rate(rate)
        .map_err(|e| format!("Failed to set sample rate: {}", e))?;
    sync_chain_to_audio_rate(state);
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn set_buffer_size(state: tauri::State<'_, AppState>, size: u32) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || set_buffer_size_on_host(&state, size)).await
}

fn set_buffer_size_on_host(state: &AppState, size: u32) -> Result<(), String> {
    state
        .audio_manager
        .set_buffer_size(size)
        .map_err(|e| format!("Failed to set buffer size: {}", e))?;
    crate::save_audio_session_to_disk(state);
    Ok(())
}

#[tauri::command]
pub async fn toggle_monitoring(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let state = state.inner().clone();
    crate::core::host_thread::run(move || toggle_monitoring_on_host(&state, enabled)).await
}

fn toggle_monitoring_on_host(state: &AppState, enabled: bool) -> Result<(), String> {
    state
        .audio_manager
        .toggle_monitoring(enabled)
        .map_err(|e| format!("Failed to toggle monitoring: {}", e))?;
    if enabled {
        sync_chain_to_audio_rate(state);
    }
    Ok(())
}

#[tauri::command]
pub fn set_muted(
    app: tauri::AppHandle<tauri::Wry>,
    state: tauri::State<AppState>,
    muted: bool,
) -> Result<(), String> {
    state.audio_manager.set_muted(muted);
    let tray_state = app.state::<crate::TrayState>();
    crate::bootstrap::tray::sync_audio_tray_state(&app, &tray_state, muted);
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn set_loopback(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .audio_manager
        .set_loopback(enabled)
        .map_err(|e| format!("Failed to set loopback: {}", e))?;
    crate::save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn get_vu_data(state: tauri::State<AppState>) -> Result<VUData, String> {
    Ok(state.vu_meter.get_data())
}

#[tauri::command]
pub fn play_test_sound() -> Result<(), String> {
    let mut guard = TEST_SOUND_PROCESS.lock().unwrap_or_else(|e| e.into_inner());
    // Kill any previous test-sound process before spawning a new one.
    if let Some(ref mut prev) = *guard {
        let _ = prev.kill();
    }
    *guard = None;

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let child = Command::new("powershell")
            .args([
                "-WindowStyle",
                "Hidden",
                "-Command",
                "[System.Media.SystemSounds]::Beep.Play(); Start-Sleep -Milliseconds 800",
            ])
            .spawn()
            .map_err(|e| format!("Failed to play test sound: {}", e))?;
        *guard = Some(child);
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;
        if let Ok(child) = Command::new("bash")
            .args([
                "-c",
                "paplay /usr/share/sounds/freedesktop/stereo/bell.oga 2>/dev/null || afplay /System/Library/Sounds/Ping.aiff 2>/dev/null",
            ])
            .spawn()
        {
            *guard = Some(child);
        }
    }
    Ok(())
}
