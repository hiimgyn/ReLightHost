mod audio;
mod plugins;
mod preset;
mod config;

use parking_lot::RwLock;
use std::sync::Arc;
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
    if let Some(proc) = sys.process(pid) {
        let total_mem = sys.total_memory();
        let proc_mem = proc.memory();
        let ram_pct = if total_mem > 0 { (proc_mem as f32 / total_mem as f32) * 100.0 } else { 0.0 };
        Ok(SystemStats {
            cpu_percent: proc.cpu_usage(),
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
        .map_err(|e| format!("Failed to set output device: {}", e))
}

#[tauri::command]
fn set_input_device(state: tauri::State<AppState>, device_id: Option<String>) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_input_device(device_id)
        .map_err(|e| format!("Failed to set input device: {}", e))
}

#[tauri::command]
fn set_sample_rate(state: tauri::State<AppState>, sample_rate: u32) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_sample_rate(sample_rate)
        .map_err(|e| format!("Failed to set sample rate: {}", e))
}

#[tauri::command]
fn set_buffer_size(state: tauri::State<AppState>, buffer_size: u32) -> Result<(), String> {
    state.audio_manager
        .read()
        .set_buffer_size(buffer_size)
        .map_err(|e| format!("Failed to set buffer size: {}", e))
}

#[tauri::command]
fn toggle_monitoring(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state.audio_manager
        .read()
        .toggle_monitoring(enabled)
        .map_err(|e| format!("Failed to toggle monitoring: {}", e))
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
    state.plugin_manager
        .read()
        .load_plugin(plugin_info)
        .map_err(|e| format!("Failed to load plugin: {}", e))
}

#[tauri::command]
fn remove_plugin(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    state.plugin_manager
        .read()
        .remove_instance(&instance_id)
        .map_err(|e| format!("Failed to remove plugin: {}", e))
}

#[tauri::command]
fn get_plugin_chain(state: tauri::State<AppState>) -> Result<Vec<PluginInstanceInfo>, String> {
    Ok(state.plugin_manager.read().get_instances())
}

#[tauri::command]
fn set_plugin_bypass(state: tauri::State<AppState>, instance_id: String, bypassed: bool) -> Result<(), String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        instance.set_bypassed(bypassed);
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
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
fn reorder_plugin_chain(state: tauri::State<AppState>, from_index: usize, to_index: usize) -> Result<(), String> {
    state.plugin_manager
        .read()
        .reorder(from_index, to_index)
        .map_err(|e| format!("Failed to reorder plugin chain: {}", e))
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
    for plugin_preset in preset.plugin_chain {
        if plugin_preset.plugin_path.is_some() && plugin_preset.plugin_format.is_some() {
            let plugin_info = PluginInfo {
                id: plugin_preset.plugin_id,
                name: plugin_preset.plugin_name,
                vendor: plugin_preset.plugin_vendor.unwrap_or_default(),
                version: plugin_preset.plugin_version.unwrap_or_default(),
                path: plugin_preset.plugin_path.unwrap(),
                format: plugin_preset.plugin_format.unwrap(),
                category: plugin_preset.plugin_category.unwrap_or_default(),
            };

            if let Ok(instance_id) = state.plugin_manager.read().load_plugin(plugin_info) {
                if let Some(instance) = state.plugin_manager.read().get_instance(&instance_id) {
                    instance.set_bypassed(plugin_preset.bypassed);
                    for p in plugin_preset.parameters {
                        instance.set_parameter(p.id, p.value);
                    }
                }
            }
        }
    }

    log::info!("Applied preset: {}", name);
    Ok(())
}

#[tauri::command]
fn launch_plugin(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        let info = instance.get_info();
        let path = info.path.clone();
        let name = info.name.clone();
        let format = info.format;
        drop(manager); // release lock before spawning

        match format {
            plugins::PluginFormat::VST3 => {
                plugins::launch_vst3_gui(&path, &name)
                    .map_err(|e| format!("Failed to launch plugin GUI: {}", e))
            }
            plugins::PluginFormat::VST => {
                Err("VST2 plugin GUI launching is not yet supported".to_string())
            }
            plugins::PluginFormat::CLAP => {
                Err("CLAP plugin GUI launching is not yet supported".to_string())
            }
        }
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

// Preset Commands

#[tauri::command]
fn save_preset(state: tauri::State<AppState>, name: String) -> Result<String, String> {
    let plugin_chain = state.plugin_manager.read().get_instances();
    let preset = Preset::new(name, plugin_chain);
    
    state.preset_manager
        .read()
        .save_preset(&preset)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to save preset: {}", e))
}

#[tauri::command]
fn load_preset(state: tauri::State<AppState>, name: String) -> Result<Preset, String> {
    state.preset_manager
        .read()
        .load_preset(&name)
        .map_err(|e| format!("Failed to load preset: {}", e))
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
    let plugin_chain = state.plugin_manager.read().get_instances();
    state.preset_manager
        .read()
        .auto_save(plugin_chain)
        .map_err(|e| format!("Failed to auto-save: {}", e))
}

#[tauri::command]
fn has_auto_save(state: tauri::State<AppState>) -> Result<bool, String> {
    Ok(state.preset_manager.read().has_auto_save())
}

#[tauri::command]
fn restore_auto_save(state: tauri::State<AppState>) -> Result<Preset, String> {
    state.preset_manager
        .read()
        .restore_auto_save()
        .map_err(|e| format!("Failed to restore auto-save: {}", e))
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

// Startup Commands

#[tauri::command]
fn is_startup_enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("reg")
            .args(&[
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
fn toggle_startup(enable: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::env;
        
        if enable {
            let exe_path = env::current_exe()
                .map_err(|e| format!("Failed to get exe path: {}", e))?;
            
            let output = Command::new("reg")
                .args(&[
                    "add",
                    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                    "/v",
                    "ReLightHost",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{}\"", exe_path.display()),
                    "/f",
                ])
                .output()
                .map_err(|e| format!("Failed to add registry key: {}", e))?;
            
            if !output.status.success() {
                return Err("Failed to enable startup".to_string());
            }
        } else {
            let output = Command::new("reg")
                .args(&[
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
        let _ = enable;
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize managers
    let audio_manager = Arc::new(RwLock::new(AudioManager::new()));
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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            audio_manager,
            plugin_scanner,
            plugin_manager,
            preset_manager,
            config_manager,
            sys_info,
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
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
            set_sample_rate,
            set_buffer_size,
            toggle_monitoring,
            scan_plugins,
            load_plugin,
            remove_plugin,
            get_plugin_chain,
            set_plugin_bypass,
            set_plugin_parameter,
            reorder_plugin_chain,
            apply_preset,
            play_test_sound,
            save_preset,
            load_preset,
            list_presets,
            delete_preset,
            auto_save_preset,
            has_auto_save,
            restore_auto_save,
            get_custom_scan_paths,
            add_custom_scan_path,
            remove_custom_scan_path,
            is_startup_enabled,
            toggle_startup,
            launch_plugin,
            get_system_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

