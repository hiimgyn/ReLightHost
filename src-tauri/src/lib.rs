mod audio;
mod plugins;
mod preset;
mod config;
mod app_events;

use parking_lot::RwLock;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::Manager;
use audio::{AudioManager, AudioStatus, AudioDeviceInfo, AudioDevice, AudioConfig};
use plugins::{PluginScanner, PluginInstanceManager, PluginInfo, PluginInstanceInfo};
use preset::{PresetManager, Preset};
use config::ConfigManager;

// Global audio manager
struct AppState {
    audio_manager: Arc<RwLock<AudioManager>>,
    plugin_scanner: Arc<RwLock<PluginScanner>>,
    plugin_manager: Arc<RwLock<PluginInstanceManager>>,
    preset_manager: Arc<RwLock<PresetManager>>,
    config_manager: Arc<RwLock<ConfigManager>>,
    sys_info: Arc<RwLock<sysinfo::System>>,
    /// Set to true after the first successful restore_session call.
    /// React StrictMode invokes effects twice in dev; this flag makes the
    /// second call a fast no-op so the restored ASIO stream is not torn down.
    session_restored: AtomicBool,
    /// Coalesce rapid plugin-chain mutations into a single autosave write.
    autosave_scheduled: Arc<AtomicBool>,
    /// Last serialized autosave hash to skip redundant writes.
    autosave_last_hash: Arc<AtomicU64>,
    /// Backend-orchestrated anti-crash delayed start deadline.
    /// When set, toggle_monitoring(true) will wait until this instant before
    /// opening audio streams.
    safe_start_deadline: Arc<RwLock<Option<Instant>>>,
}

/// Holds dynamic menu items so they can be updated from commands and tray events.
struct TrayState {
    mute_item:     tauri::menu::MenuItem<tauri::Wry>,
    loopback_item: tauri::menu::MenuItem<tauri::Wry>,
}

#[derive(serde::Serialize)]
struct SystemStats {
    cpu_percent: f32,
    ram_percent: f32,
    ram_used_mb: u64,
    ram_total_mb: u64,
}

#[tauri::command]
fn get_system_stats(state: tauri::State<AppState>) -> Result<SystemStats, String> {
    use sysinfo::{Pid, ProcessRefreshKind};
    let pid = Pid::from_u32(std::process::id());
    let mut sys = state.sys_info.write();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new().with_cpu().with_memory(),
    );
    let num_cpus = sys.cpus().len().max(1) as f32;
    if let Some(proc) = sys.process(pid) {
        let total_mem = sys.total_memory();
        let proc_mem = proc.memory();
        // cpu_usage() on Windows returns usage across all cores combined,
        // so divide by core count to get a 0–100% per-process percentage.
        let cpu_pct = (proc.cpu_usage() / num_cpus).min(100.0);
        let ram_pct = if total_mem > 0 { (proc_mem as f32 / total_mem as f32) * 100.0 } else { 0.0 };
        Ok(SystemStats {
            cpu_percent: cpu_pct,
            ram_percent: ram_pct,
            ram_used_mb: proc_mem / 1024 / 1024,
            ram_total_mb: total_mem / 1024 / 1024,
        })
    } else {
        Ok(SystemStats { cpu_percent: 0.0, ram_percent: 0.0, ram_used_mb: 0, ram_total_mb: 0 })
    }
}

// Audio Commands

#[tauri::command]
fn start_audio(state: tauri::State<AppState>) -> Result<(), String> {
    state.audio_manager
        .read()
        .start()
        .map_err(|e| format!("Failed to start audio: {}", e))
}

#[tauri::command]
fn stop_audio(state: tauri::State<AppState>) -> Result<(), String> {
    state.audio_manager
        .read()
        .stop()
        .map_err(|e| format!("Failed to stop audio: {}", e))
}

#[tauri::command]
fn get_audio_status(state: tauri::State<AppState>) -> Result<AudioStatus, String> {
    Ok(state.audio_manager.read().get_status())
}

#[tauri::command]
fn list_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    AudioDevice::list_devices()
        .map_err(|e| format!("Failed to list audio devices: {}", e))
}

#[tauri::command]
fn get_audio_config(state: tauri::State<AppState>) -> Result<AudioConfig, String> {
    Ok(state.audio_manager.read().get_config())
}

#[tauri::command]
fn set_output_device(state: tauri::State<AppState>, device_id: String) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_output_device(Some(device_id))
        .map_err(|e| format!("Failed to set output device: {}", e))?;
    save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
fn set_input_device(state: tauri::State<AppState>, device_id: Option<String>) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_input_device(device_id)
        .map_err(|e| format!("Failed to set input device: {}", e))?;
    save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
fn set_virtual_output_device(state: tauri::State<AppState>, device_id: Option<String>) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_virtual_output_device(device_id)
        .map_err(|e| format!("Failed to set virtual output device: {}", e))?;
    save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
fn set_sample_rate(state: tauri::State<AppState>, sample_rate: u32) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_sample_rate(sample_rate)
        .map_err(|e| format!("Failed to set sample rate: {}", e))?;
    save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
fn set_buffer_size(state: tauri::State<AppState>, buffer_size: u32) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_buffer_size(buffer_size)
        .map_err(|e| format!("Failed to set buffer size: {}", e))?;
    save_audio_session_to_disk(&state);
    Ok(())
}

#[tauri::command]
fn toggle_monitoring(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    if enabled {
        let deadline_opt = *state.safe_start_deadline.read();
        if let Some(deadline) = deadline_opt {
            let now = Instant::now();
            if deadline > now {
                let wait = deadline.duration_since(now);
                log::info!(
                    "Safe delayed start active: waiting {} ms before monitoring start",
                    wait.as_millis()
                );
                std::thread::sleep(wait);
            }
            *state.safe_start_deadline.write() = None;
        }
    }

    state.audio_manager
        .read()
        .toggle_monitoring(enabled)
        .map_err(|e| format!("Failed to toggle monitoring: {}", e))
}

#[tauri::command]
fn set_muted(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    tray_state: tauri::State<TrayState>,
    muted: bool,
) -> Result<(), String> {
    state.audio_manager.read().set_muted(muted);
    save_audio_session_to_disk(&state);
    // Sync tray menu item text and tooltip
    let new_text = if muted { "Unmute Audio" } else { "Mute Audio" };
    let _ = tray_state.mute_item.set_text(new_text);
    if let Some(tray) = app.tray_by_id("main") {
        let tooltip = if muted { "ReLightHost (Muted)" } else { "ReLightHost" };
        let _ = tray.set_tooltip(Some(tooltip));
    }
    Ok(())
}

#[tauri::command]
fn set_loopback(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    tray_state: tauri::State<TrayState>,
    enabled: bool,
) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_loopback(enabled)
        .map_err(|e| format!("Failed to set loopback: {}", e))?;
    let text = if enabled { "Disable Hardware Out" } else { "Enable Hardware Out" };
    let _ = tray_state.loopback_item.set_text(text);
    let _ = app; // keep handle in scope
    Ok(())
}

#[tauri::command]
fn get_vu_data(state: tauri::State<AppState>) -> Result<audio::VUData, String> {
    Ok(state.audio_manager.read().get_vu_data())
}

// Plugin Commands

#[tauri::command]
fn scan_plugins(state: tauri::State<AppState>) -> Result<Vec<PluginInfo>, String> {
    // Get custom paths and add them to scanner
    let custom_paths = state.config_manager.read().get_custom_paths();
    let mut scanner = state.plugin_scanner.write();
    
    // Reset scanner and recreate with default + custom paths
    *scanner = PluginScanner::new();
    for path in custom_paths {
        scanner.add_scan_path(path);
    }
    
    scanner
        .scan()
        .map_err(|e| format!("Failed to scan plugins: {}", e))
}

#[tauri::command]
fn load_plugin(state: tauri::State<AppState>, plugin_info: PluginInfo) -> Result<String, String> {
    let config = state.audio_manager.read().get_config();
    let instance_id = state.plugin_manager
        .read()
        .load_plugin(plugin_info, config.sample_rate as f64, config.buffer_size as usize)
        .map_err(|e| format!("Failed to load plugin: {}", e))?;
    auto_save_plugin_chain(&state);
    crate::app_events::emit_plugin_chain_changed("load", Some(&instance_id));
    Ok(instance_id)
}

#[tauri::command]
fn remove_plugin(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    state.plugin_manager
        .read()
        .remove_instance(&instance_id)
        .map_err(|e| format!("Failed to remove plugin: {}", e))?;
    auto_save_plugin_chain(&state);
    crate::app_events::emit_plugin_chain_changed("remove", Some(&instance_id));
    Ok(())
}

#[tauri::command]
fn get_plugin_chain(state: tauri::State<AppState>) -> Result<Vec<PluginInstanceInfo>, String> {
    Ok(state.plugin_manager.read().get_instances())
}

#[tauri::command]
fn set_plugin_bypass(state: tauri::State<AppState>, instance_id: String, bypassed: bool) -> Result<(), String> {
    {
        let manager = state.plugin_manager.read();
        if let Some(instance) = manager.get_instance(&instance_id) {
            instance.set_bypassed(bypassed);
        } else {
            return Err(format!("Plugin instance not found: {}", instance_id));
        }
    }
    auto_save_plugin_chain(&state);
    crate::app_events::emit_plugin_chain_changed("bypass", Some(&instance_id));
    Ok(())
}

#[tauri::command]
fn set_plugin_parameter(state: tauri::State<AppState>, instance_id: String, param_id: u32, value: f64) -> Result<(), String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        instance.set_parameter(param_id, value);
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
fn rename_plugin(state: tauri::State<AppState>, instance_id: String, new_name: String) -> Result<(), String> {
    {
        let manager = state.plugin_manager.read();
        if let Some(instance) = manager.get_instance(&instance_id) {
            instance.rename(new_name);
        } else {
            return Err(format!("Plugin instance not found: {}", instance_id));
        }
    }
    auto_save_plugin_chain(&state);
    crate::app_events::emit_plugin_chain_changed("rename", Some(&instance_id));
    Ok(())
}

#[tauri::command]
fn reorder_plugin_chain(state: tauri::State<AppState>, from_index: usize, to_index: usize) -> Result<(), String> {
    state.plugin_manager
        .read()
        .reorder(from_index, to_index)
        .map_err(|e| format!("Failed to reorder plugin chain: {}", e))?;
    auto_save_plugin_chain(&state);
    crate::app_events::emit_plugin_chain_changed("reorder", None);
    Ok(())
}

#[tauri::command]
fn apply_preset(state: tauri::State<AppState>, name: String) -> Result<(), String> {
    use plugins::PluginInfo;
    
    let preset = state.preset_manager
        .read()
        .load_preset(&name)
        .map_err(|e| format!("Failed to load preset: {}", e))?;

    // Clear current chain
    state.plugin_manager.read().clear();

    // Reload plugins from preset
    let config = state.audio_manager.read().get_config();
    for plugin_preset in preset.plugin_chain {
        let (Some(path), Some(format)) = (plugin_preset.plugin_path, plugin_preset.plugin_format) else {
            continue;
        };
        let plugin_info = PluginInfo {
            id: plugin_preset.plugin_id.clone(),
            name: plugin_preset.plugin_name.clone(),
            vendor: plugin_preset.plugin_vendor.unwrap_or_default(),
            version: plugin_preset.plugin_version.unwrap_or_default(),
            path,
            format,
            category: plugin_preset.plugin_category.unwrap_or_default(),
        };

        if let Ok(instance_id) = state.plugin_manager.read().load_plugin(
            plugin_info,
            config.sample_rate as f64,
            config.buffer_size as usize,
        ) {
            if let Some(instance) = state.plugin_manager.read().get_instance(&instance_id) {
                instance.set_bypassed(plugin_preset.bypassed);

                // Restore VST3 binary state first (contains internal plugin data)
                if let Some(ref vst3_state) = plugin_preset.vst3_state {
                    log::debug!("🔄 Restoring VST3 state for '{}' ({} bytes)", 
                        plugin_preset.plugin_name, vst3_state.len());
                    instance.set_state_binary(vst3_state);
                }

                // Then apply parameter values (may override some state)
                for p in plugin_preset.parameters {
                    instance.set_parameter(p.id, p.value);
                }
            }
        }
    }

    log::info!("✅ Applied preset: {}", name);
    crate::app_events::emit_plugin_chain_changed("apply_preset", None);
    Ok(())
}

#[tauri::command]
fn launch_plugin(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        // Use the existing VST3 processor's GUI instead of loading a new instance
        instance.open_gui()
            .map_err(|e| format!("Failed to open plugin GUI: {}", e))?;
        crate::app_events::emit_plugin_chain_changed("gui_open", Some(&instance_id));
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[derive(serde::Serialize)]
struct PluginCrashStatusItem {
    instance_id: String,
    status: plugins::PluginStatus,
}

#[tauri::command]
fn get_plugin_crash_statuses(state: tauri::State<AppState>) -> Result<Vec<PluginCrashStatusItem>, String> {
    let manager = state.plugin_manager.read();
    Ok(manager
        .get_crash_statuses()
        .into_iter()
        .map(|(instance_id, status)| PluginCrashStatusItem { instance_id, status })
        .collect())
}

/// Return the current voice-activity probability (0.0–1.0) from the built-in
/// noise suppressor.  Used by the NoiseSuppressorGui component for the VAD meter.
#[tauri::command]
fn get_noise_suppressor_vad(state: tauri::State<AppState>, instance_id: String) -> Result<f32, String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        Ok(instance.get_builtin_vad())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
fn play_test_sound() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("powershell")
            .args(["-WindowStyle", "Hidden", "-Command",
                "[System.Media.SystemSounds]::Beep.Play(); Start-Sleep -Milliseconds 800"])
            .spawn()
            .map_err(|e| format!("Failed to play test sound: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;
        let _ = Command::new("bash")
            .args(["-c", "paplay /usr/share/sounds/freedesktop/stereo/bell.oga 2>/dev/null || afplay /System/Library/Sounds/Ping.aiff 2>/dev/null"])
            .spawn();
    }
    Ok(())
}

// Crash Protection Commands

#[tauri::command]
fn get_plugin_crash_status(state: tauri::State<AppState>, instance_id: String) -> Result<plugins::PluginStatus, String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        Ok(instance.get_crash_status())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
fn reset_plugin_crash_protection(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        instance.reset_crash_protection();
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

// Preset Commands

#[tauri::command]
fn save_preset(state: tauri::State<AppState>, name: String) -> Result<String, String> {
    let preset = build_chain_preset_with_state(&state, name);
    
    state.preset_manager
        .read()
        .save_preset(&preset)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to save preset: {}", e))
}

#[tauri::command]
fn list_presets(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    state.preset_manager
        .read()
        .list_presets()
        .map_err(|e| format!("Failed to list presets: {}", e))
}

#[tauri::command]
fn delete_preset(state: tauri::State<AppState>, name: String) -> Result<(), String> {
    state.preset_manager
        .read()
        .delete_preset(&name)
        .map_err(|e| format!("Failed to delete preset: {}", e))
}

#[tauri::command]
fn auto_save_preset(state: tauri::State<AppState>) -> Result<(), String> {
    let preset = build_chain_preset_with_state(&state, "__autosave__".to_string());
    
    state.preset_manager
        .read()
        .save_preset(&preset)
        .map(|_| ())
        .map_err(|e| format!("Failed to auto-save: {}", e))
}

// Config Commands

#[tauri::command]
fn get_custom_scan_paths(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    Ok(state.config_manager.read().get_custom_paths())
}

#[tauri::command]
fn add_custom_scan_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state.config_manager
        .read()
        .add_custom_path(path)
        .map_err(|e| format!("Failed to add custom path: {}", e))
}

#[tauri::command]
fn remove_custom_scan_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state.config_manager
        .read()
        .remove_custom_path(&path)
        .map_err(|e| format!("Failed to remove custom path: {}", e))
}

#[tauri::command]
fn get_minimize_to_tray(state: tauri::State<AppState>) -> bool {
    state.config_manager.read().get_minimize_to_tray()
}

#[tauri::command]
fn set_minimize_to_tray(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state.config_manager
        .read()
        .set_minimize_to_tray(enabled)
        .map_err(|e| format!("Failed to save minimize_to_tray: {}", e))
}

#[tauri::command]
fn get_show_app_on_startup(state: tauri::State<AppState>) -> bool {
    state.config_manager.read().get_show_app_on_startup()
}

#[tauri::command]
fn set_show_app_on_startup(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state.config_manager
        .read()
        .set_show_app_on_startup(enabled)
        .map_err(|e| format!("Failed to save show_app_on_startup: {}", e))?;

    // If startup is already enabled, rewrite the Run key immediately so the
    // next OS login uses the new visibility mode.
    if is_startup_enabled().unwrap_or(false) {
        toggle_startup(true, state)?;
    }

    Ok(())
}

// Startup Commands

#[tauri::command]
fn is_startup_enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("reg")
            .args([
                "query",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "ReLightHost",
            ])
            .output()
            .map_err(|e| format!("Failed to query registry: {}", e))?;
        
        Ok(output.status.success())
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // For macOS and Linux, we'd check LaunchAgents or autostart files
        // For now, return false as a placeholder
        Ok(false)
    }
}

#[tauri::command]
fn toggle_startup(enable: bool, state: tauri::State<AppState>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::env;
        
        if enable {
            let exe_path = env::current_exe()
                .map_err(|e| format!("Failed to get exe path: {}", e))?;
            let show_on_startup = state.config_manager.read().get_show_app_on_startup();
            let launch_cmd = if show_on_startup {
                format!("\"{}\"", exe_path.display())
            } else {
                format!("\"{}\" --start-hidden", exe_path.display())
            };
            
            let output = Command::new("reg")
                .args([
                    "add",
                    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                    "/v",
                    "ReLightHost",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &launch_cmd,
                    "/f",
                ])
                .output()
                .map_err(|e| format!("Failed to add registry key: {}", e))?;
            
            if !output.status.success() {
                return Err("Failed to enable startup".to_string());
            }
        } else {
            let output = Command::new("reg")
                .args([
                    "delete",
                    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                    "/v",
                    "ReLightHost",
                    "/f",
                ])
                .output()
                .map_err(|e| format!("Failed to delete registry key: {}", e))?;
            
            if !output.status.success() {
                // It's ok if the key doesn't exist
                return Ok(());
            }
        }
        Ok(())
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // For macOS and Linux, we'd create/remove LaunchAgents or autostart files
        // For now, return Ok as a placeholder
        let _ = (enable, state);
        Ok(())
    }
}

// ── Session persistence helpers ─────────────────────────────────────────────

/// Persist current audio config + mute state to session.json.
/// Called after any audio setting change so the next app launch can restore it.
fn save_audio_session_to_disk(state: &AppState) {
    let config = state.audio_manager.read().get_config();
    let muted  = state.audio_manager.read().is_muted();
    if let Err(e) = state.config_manager.read().save_session(&config, muted) {
        log::warn!("Failed to save audio session: {e}");
    }
}

/// Snapshot the current plugin chain to the __autosave__ preset.
/// Called after any structural change to the chain (add/remove/reorder/bypass/rename).
fn auto_save_plugin_chain(state: &AppState) {
    schedule_auto_save_plugin_chain(state);
}

fn build_chain_preset_with_state(state: &AppState, name: String) -> Preset {
    let chain = state.plugin_manager.read().get_instances();
    let mut preset = Preset::new(name, chain.clone());
    {
        let manager = state.plugin_manager.read();
        for (preset_plugin, info) in preset.plugin_chain.iter_mut().zip(chain.iter()) {
            if let Some(instance) = manager.get_instance(&info.instance_id) {
                let blob = instance.get_state_binary();
                if !blob.is_empty() {
                    preset_plugin.vst3_state = Some(blob);
                }
            }
        }
    }
    preset
}

fn preset_hash(preset: &Preset) -> Option<u64> {
    let bytes = serde_json::to_vec(preset).ok()?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}

fn schedule_auto_save_plugin_chain(state: &AppState) {
    if state.autosave_scheduled.swap(true, Ordering::AcqRel) {
        return;
    }

    let plugin_manager = Arc::clone(&state.plugin_manager);
    let preset_manager = Arc::clone(&state.preset_manager);
    let autosave_scheduled = Arc::clone(&state.autosave_scheduled);
    let autosave_last_hash = Arc::clone(&state.autosave_last_hash);

    std::thread::spawn(move || {
        // Debounce bursty chain updates from UI operations.
        std::thread::sleep(Duration::from_millis(200));

        let chain = plugin_manager.read().get_instances();
        let mut preset = Preset::new("__autosave__".to_string(), chain.clone());
        {
            let manager = plugin_manager.read();
            for (preset_plugin, info) in preset.plugin_chain.iter_mut().zip(chain.iter()) {
                if let Some(instance) = manager.get_instance(&info.instance_id) {
                    let blob = instance.get_state_binary();
                    if !blob.is_empty() {
                        preset_plugin.vst3_state = Some(blob);
                    }
                }
            }
        }

        let skip_write = preset_hash(&preset)
            .map(|h| h == autosave_last_hash.load(Ordering::Acquire))
            .unwrap_or(false);

        if !skip_write {
            match preset_manager.read().save_preset(&preset) {
                Ok(_) => {
                    if let Some(h) = preset_hash(&preset) {
                        autosave_last_hash.store(h, Ordering::Release);
                    }
                }
                Err(e) => log::warn!("Failed to auto-save plugin chain: {e}"),
            }
        }

        autosave_scheduled.store(false, Ordering::Release);
    });
}

/// Summary returned to the frontend so it can show a restore notification.
#[derive(serde::Serialize)]
struct SessionRestoreResult {
    audio_restored: bool,
    plugins_restored: usize,
    /// True when the output device is a Voicemeeter ASIO Insert driver.
    /// Voicemeeter needs to finish its own startup before our ASIO stream
    /// connects — the frontend schedules `toggle_monitoring(true)` after a
    /// delay rather than doing it here to keep the call on a COM-initialized
    /// Tauri command thread (raw std::thread::spawn threads crash on ASIO).
    needs_deferred_start: bool,
}

/// Restore the last saved session:
///   1. Audio config (session.json) → AudioManager → start monitoring
///   2. Plugin chain (__autosave__ preset) → PluginInstanceManager
///
/// Called once by the frontend on mount.  A guard prevents double-restore if
/// the chain is already populated (e.g. React StrictMode double-effect).
#[tauri::command]
fn restore_session(state: tauri::State<AppState>) -> Result<SessionRestoreResult, String> {
    use plugins::PluginInfo;

    // Guard: React StrictMode calls effects twice in development.
    // The compare_exchange ensures only the first call does real work;
    // the second is a fast no-op that returns the same shape of result.
    if state.session_restored.compare_exchange(
        false, true, Ordering::SeqCst, Ordering::SeqCst
    ).is_err() {
        log::info!("restore_session: already restored, skipping duplicate call");
        let plugins_restored = state.plugin_manager.read().get_instances().len();
        crate::app_events::emit_plugin_chain_changed("restore_session_skip", None);
        return Ok(SessionRestoreResult {
            audio_restored: state.audio_manager.read().get_status().is_monitoring,
            plugins_restored,
            needs_deferred_start: false,
        });
    }

    let mut audio_restored   = false;
    let mut plugins_restored = 0usize;

    // ── 1. Audio config (stop stream only — do NOT restart yet) ───────────
    if let Some(session) = state.config_manager.read().load_session() {
        // React child effects (Layout) fire before parent effects (App), so
        // Layout's toggleMonitoring(true) may have already opened a stream
        // using the default config.  Stop it unconditionally so we can swap
        // in the saved config.
        let _ = state.audio_manager.read().toggle_monitoring(false);

        state.audio_manager.read().restore_config(session.audio);
        state.audio_manager.read().set_muted(session.muted);
        audio_restored = true;
        log::info!("✅ Audio session restored");
    }

    // ── 2. Plugin chain (loaded while stream is stopped) ──────────────────
    // Plugins are loaded BEFORE the audio stream starts so the very first
    // buffer handed to Voicemeeter Insert already has the full chain active.
    // Guard: do not reload if the chain already has items (StrictMode double-invoke).
    let chain_empty = state.plugin_manager.read().get_instances().is_empty();
    if chain_empty {
        if let Ok(preset) = state.preset_manager.read().restore_auto_save() {
            let config = state.audio_manager.read().get_config();
            state.plugin_manager.read().clear();

            for plugin_preset in &preset.plugin_chain {
                let (Some(path), Some(format)) =
                    (plugin_preset.plugin_path.as_ref(), plugin_preset.plugin_format)
                else {
                    continue;
                };
                let plugin_info = PluginInfo {
                    id:       plugin_preset.plugin_id.clone(),
                    name:     plugin_preset.plugin_name.clone(),
                    vendor:   plugin_preset.plugin_vendor.clone().unwrap_or_default(),
                    version:  plugin_preset.plugin_version.clone().unwrap_or_default(),
                    path:     path.clone(),
                    format,
                    category: plugin_preset.plugin_category.clone().unwrap_or_default(),
                };

                if let Ok(instance_id) = state.plugin_manager.read().load_plugin(
                    plugin_info,
                    config.sample_rate as f64,
                    config.buffer_size as usize,
                ) {
                    let restoring_vst3 = format == crate::plugins::types::PluginFormat::VST3;
                    if let Some(instance) = state.plugin_manager.read().get_instance(&instance_id) {
                        instance.set_bypassed(plugin_preset.bypassed);
                        if restoring_vst3 {
                            // Startup-safe mode for fragile VST3 plugins:
                            // defer state/parameter replay to avoid heap corruption
                            // during immediate post-load initialization.
                            log::warn!(
                                "Skipping startup state replay for VST3 '{}' to improve launch stability",
                                plugin_preset.plugin_name
                            );
                        } else {
                            if let Some(ref vst3_state) = plugin_preset.vst3_state {
                                instance.set_state_binary(vst3_state);
                            }
                            for p in &plugin_preset.parameters {
                                instance.set_parameter(p.id, p.value);
                            }
                        }
                    }
                    plugins_restored += 1;
                }
            }

            if plugins_restored > 0 {
                log::info!("✅ Plugin chain restored: {} plugins", plugins_restored);
            }
        }
    }

    // ── 3. Start stream (plugin chain is fully ready) ─────────────────────
    //
    // ASIO COM rule: toggle_monitoring must always be called from a thread
    // that has COM initialized (i.e. a Tauri command handler thread).
    // Raw std::thread::spawn threads are NOT COM-initialized and will crash
    // with STATUS_ACCESS_VIOLATION on ASIO drivers.
    //
    // For Voicemeeter Insert ASIO, Voicemeeter must finish its own startup
    // before our ASIO stream connects.  We signal the frontend to call
    // toggle_monitoring(true) after a 2-second delay, keeping the call on
    // the proper Tauri command thread.
    // Runtime check is more reliable than preset metadata when deciding whether
    // startup monitoring should be deferred for fragile plugin hosts.
    let restored_has_vst3 = state
        .plugin_manager
        .read()
        .get_instances()
        .iter()
        .any(|p| p.format == crate::plugins::types::PluginFormat::VST3);

    let is_voicemeeter = state
        .audio_manager
        .read()
        .get_config()
        .output_device_id
        .as_deref()
        .map(|id| id.to_lowercase().contains("voicemeeter"))
        .unwrap_or(false);

    let mut safe_delay_ms = 0u64;
    if restored_has_vst3 {
        safe_delay_ms = safe_delay_ms.max(4000);
    }
    if is_voicemeeter {
        safe_delay_ms = safe_delay_ms.max(2000);
    }

    let needs_deferred_start = audio_restored && safe_delay_ms > 0;

    if needs_deferred_start {
        *state.safe_start_deadline.write() = Some(Instant::now() + Duration::from_millis(safe_delay_ms));
        // Extra guard after stream start: skip VST3 process() during fragile warmup.
        // total guard = delayed-start wait + additional post-start settling window.
        let extra_post_start_ms = if restored_has_vst3 { 8000 } else { 0 };
        crate::plugins::vst3_processor::set_global_process_block_ms(
            safe_delay_ms.saturating_add(extra_post_start_ms)
        );
        log::info!(
            "Scheduled backend safe delayed start: {} ms (vst3={}, voicemeeter={})",
            safe_delay_ms,
            restored_has_vst3,
            is_voicemeeter
        );
    } else {
        *state.safe_start_deadline.write() = None;
        crate::plugins::vst3_processor::set_global_process_block_ms(0);
    }

    if audio_restored {
        if !needs_deferred_start {
            if let Err(e) = state.audio_manager.read().toggle_monitoring(true) {
                log::warn!("Failed to auto-start monitoring on session restore: {e}");
            }
        } else {
            log::info!(
                "Automatic monitoring deferred; frontend can call toggleMonitoring(true) immediately and backend will gate start"
            );
        }
    }

    // Notify frontend that startup restore finished and chain snapshot is ready.
    crate::app_events::emit_plugin_chain_changed("restore_session", None);

    Ok(SessionRestoreResult { audio_restored, plugins_restored, needs_deferred_start })
}

// ── Auto-update commands ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct UpdateInfo {
    available: bool,
    version: Option<String>,
    notes: Option<String>,
}

#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateInfo { available: false, version: None, notes: None }),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let audio_manager  = Arc::new(RwLock::new(AudioManager::new()));
    let plugin_scanner = Arc::new(RwLock::new(PluginScanner::new()));
    let plugin_manager = Arc::new(RwLock::new(PluginInstanceManager::new()));
    let preset_manager = Arc::new(RwLock::new(PresetManager::default()));
    let config_manager = Arc::new(RwLock::new(
        ConfigManager::new().expect("Failed to initialize config manager")
    ));
    let sys_info = Arc::new(RwLock::new({
        use sysinfo::{System, RefreshKind, CpuRefreshKind, MemoryRefreshKind};
        let mut s = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        s.refresh_all();
        s
    }));

    // Wire the plugin chain into the audio manager.
    // When monitoring is active, the CPAL output callback calls this closure
    // once per block — exactly like LightHost's AudioProcessorGraph routing:
    //   INPUT node → plugin1 → plugin2 → ... → OUTPUT node
    {
        let pm = Arc::clone(&plugin_manager);
        audio_manager.read().set_process_callback(move |left, right| {
            // try_read is non-blocking — safe for real-time audio thread.
            if let Some(guard) = pm.try_read() {
                guard.process_chain_stereo(left, right);
            }
        });
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Second instance launched — focus the existing window instead
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            audio_manager,
            plugin_scanner,
            plugin_manager,
            preset_manager,
            config_manager,
            sys_info,
            session_restored: AtomicBool::new(false),
            autosave_scheduled: Arc::new(AtomicBool::new(false)),
            autosave_last_hash: Arc::new(AtomicU64::new(0)),
            safe_start_deadline: Arc::new(RwLock::new(None)),
        })
        .setup(|app| {
            crate::app_events::init_app_handle(app.handle().clone());

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // System tray icon with context menu.
            {
                use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
                use tauri::Manager;

                let show_item      = MenuItem::with_id(app, "show",            "Show ReLightHost",      true, None::<&str>)?;
                let mute_item      = MenuItem::with_id(app, "toggle_mute",     "Mute Audio",            true, None::<&str>)?;
                let loopback_item  = MenuItem::with_id(app, "toggle_loopback", "Enable Hardware Out",   true, None::<&str>)?;
                let audio_item     = MenuItem::with_id(app, "audio_settings",  "Audio Settings…",       true, None::<&str>)?;
                let app_item       = MenuItem::with_id(app, "app_settings",    "Application Settings…", true, None::<&str>)?;
                let quit_item      = MenuItem::with_id(app, "quit",            "Exit",                  true, None::<&str>)?;
                let sep1 = PredefinedMenuItem::separator(app)?;
                let sep2 = PredefinedMenuItem::separator(app)?;
                let sep3 = PredefinedMenuItem::separator(app)?;

                let menu = Menu::with_items(app, &[
                    &show_item,
                    &sep1,
                    &mute_item,
                    &loopback_item,
                    &sep2,
                    &audio_item,
                    &app_item,
                    &sep3,
                    &quit_item,
                ])?;

                let tray = TrayIconBuilder::with_id("main")
                    .tooltip("ReLightHost")
                    .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
                        tauri::image::Image::new(&[] as &[u8], 0, 0)
                    }))
                    .menu(&menu)
                    .on_menu_event(|app: &tauri::AppHandle<tauri::Wry>, event: tauri::menu::MenuEvent| {
                        use tauri::Emitter;
                        match event.id.as_ref() {
                            "show" => {
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.show();
                                    let _ = win.unminimize();
                                    let _ = win.set_focus();
                                }
                            }
                            "toggle_mute" => {
                                // Toggle mute on the audio engine and sync the frontend
                                let state = app.state::<AppState>();
                                let manager = state.audio_manager.read();
                                let new_muted = !manager.is_muted();
                                manager.set_muted(new_muted);
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.emit("tray-mute-changed", new_muted);
                                }
                                // Sync tray menu item text and tooltip
                                let tray_state = app.state::<TrayState>();
                                let new_text = if new_muted { "Unmute Audio" } else { "Mute Audio" };
                                let _ = tray_state.mute_item.set_text(new_text);
                                if let Some(tray) = app.tray_by_id("main") {
                                    let tooltip = if new_muted { "ReLightHost (Muted)" } else { "ReLightHost" };
                                    let _ = tray.set_tooltip(Some(tooltip));
                                }
                            }
                            "toggle_loopback" => {
                                let state = app.state::<AppState>();
                                let manager = state.audio_manager.read();
                                let new_enabled = !manager.is_loopback_enabled();
                                let _ = manager.set_loopback(new_enabled);
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.emit("tray-loopback-changed", new_enabled);
                                }
                                let tray_state = app.state::<TrayState>();
                                let new_text = if new_enabled { "Disable Hardware Out" } else { "Enable Hardware Out" };
                                let _ = tray_state.loopback_item.set_text(new_text);
                            }
                            "audio_settings" => {
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.show();
                                    let _ = win.unminimize();
                                    let _ = win.set_focus();
                                    let _ = win.emit("tray-open-audio-settings", ());
                                }
                            }
                            "app_settings" => {
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.show();
                                    let _ = win.unminimize();
                                    let _ = win.set_focus();
                                    let _ = win.emit("tray-open-app-settings", ());
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    })
                    .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event: TrayIconEvent| {
                        // Left-click: show/restore window
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event {
                            if let Some(window) = tray.app_handle().get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;
                let _ = tray; // keep alive
                app.manage(TrayState { mute_item, loopback_item });
            }

            // Set initial window size proportional to the primary monitor,
            // keeping the ~11:7 aspect ratio and respecting the 800×520 minimum.
            const RATIO: f64 = 860.0 / 560.0;
            const MIN_W: f64 = 800.0;
            const MIN_H: f64 = 520.0;

            if let Some(window) = app.get_webview_window("main") {
                let start_hidden = std::env::args().any(|arg| arg == "--start-hidden");
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let monitor: tauri::Monitor = monitor;
                    let scale: f64 = monitor.scale_factor();
                    let logical_w = monitor.size().width  as f64 / scale;
                    let logical_h = monitor.size().height as f64 / scale;

                    // Use 65% of the screen width to derive the window size.
                    let from_w = (logical_w * 0.65).round();
                    let from_h = (from_w / RATIO).round();
                    let (mut win_w, mut win_h): (f64, f64) = if from_h <= logical_h * 0.9 {
                        (from_w, from_h)
                    } else {
                        let h = (logical_h * 0.9).round();
                        (( h * RATIO).round(), h)
                    };

                    win_w = win_w.max(MIN_W);
                    win_h = win_h.max(MIN_H);

                    let _ = window.set_size(tauri::LogicalSize::new(win_w, win_h));
                }

                if start_hidden {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_audio,
            stop_audio,
            get_audio_status,
            list_audio_devices,
            get_audio_config,
            set_output_device,
            set_input_device,
            set_virtual_output_device,
            set_sample_rate,
            set_buffer_size,
            toggle_monitoring,
            set_muted,
            set_loopback,
            get_vu_data,
            scan_plugins,
            load_plugin,
            remove_plugin,
            get_plugin_chain,
            set_plugin_bypass,
            set_plugin_parameter,
            reorder_plugin_chain,
            rename_plugin,
            apply_preset,
            play_test_sound,
            save_preset,
            list_presets,
            delete_preset,
            auto_save_preset,
            get_custom_scan_paths,
            add_custom_scan_path,
            remove_custom_scan_path,
            get_minimize_to_tray,
            set_minimize_to_tray,
            get_show_app_on_startup,
            set_show_app_on_startup,
            is_startup_enabled,
            toggle_startup,
            launch_plugin,
            get_system_stats,
            get_plugin_crash_status,
            get_plugin_crash_statuses,
            reset_plugin_crash_protection,
            get_noise_suppressor_vad,
            restore_session,
            check_for_update,
            install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

