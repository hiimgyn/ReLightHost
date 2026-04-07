use crate::plugins::{PluginFormat, PluginInfo, PluginInstanceInfo};
use crate::AppState;
use crate::timing::GUI_CLOSE_TIMEOUT;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(serde::Serialize)]
pub struct LaunchPluginsResult {
    ok_count: usize,
    skipped_count: usize,
    errors: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct PluginCrashStatusItem {
    instance_id: String,
    status: crate::plugins::PluginStatus,
}

fn wait_for_vst3_restore_ready(state: &AppState) {
    if state.startup.vst3_restore_ready.load(Ordering::Acquire) {
        return;
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if state.startup.vst3_restore_ready.load(Ordering::Acquire) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    log::warn!("VST3 restore replay did not finish within the wait timeout; opening GUI anyway");
}

#[tauri::command]
pub fn scan_plugins(state: tauri::State<AppState>) -> Result<Vec<PluginInfo>, String> {
    let custom_paths = state.config_manager.read().get_custom_paths();
    state
        .plugin_scanner
        .read()
        .scan(&custom_paths)
        .map_err(|e| format!("Failed to scan plugins: {}", e))
}

#[tauri::command]
pub fn load_plugin(state: tauri::State<AppState>, info: PluginInfo) -> Result<String, String> {
    let config = state.audio_manager.read().get_config();
    let id = state
        .plugin_manager
        .read()
        .load_plugin(info, config.sample_rate as f64, config.buffer_size as usize)
        .map_err(|e| format!("Failed to load plugin: {}", e))?;
    crate::app_events::emit_plugin_chain_changed("load", Some(&id));
    Ok(id)
}

#[tauri::command]
pub fn remove_plugin(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .read()
        .remove_instance(&instance_id)
        .map_err(|e| format!("Failed to remove plugin: {}", e))?;
    crate::app_events::emit_plugin_chain_changed("remove", Some(&instance_id));
    Ok(())
}

#[tauri::command]
pub fn get_plugin_chain(state: tauri::State<AppState>) -> Result<Vec<PluginInstanceInfo>, String> {
    Ok(state.plugin_manager.read().get_instances())
}

#[tauri::command]
pub fn set_plugin_bypass(state: tauri::State<AppState>, instance_id: String, bypass: bool) -> Result<(), String> {
    if let Some(inst) = state.plugin_manager.read().get_instance(&instance_id) {
        inst.set_bypassed(bypass);
        crate::app_events::emit_plugin_chain_changed("bypass", Some(&instance_id));
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn set_plugin_parameter(
    state: tauri::State<AppState>,
    instance_id: String,
    param_id: u32,
    value: f64,
) -> Result<(), String> {
    if let Some(inst) = state.plugin_manager.read().get_instance(&instance_id) {
        inst.set_parameter(param_id, value);
        crate::app_events::emit_plugin_chain_changed("parameter", Some(&instance_id));
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn rename_plugin(state: tauri::State<AppState>, instance_id: String, new_name: String) -> Result<(), String> {
    if let Some(inst) = state.plugin_manager.read().get_instance(&instance_id) {
        inst.rename(new_name);
        crate::app_events::emit_plugin_chain_changed("rename", Some(&instance_id));
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn get_plugin_parameters(
    state: tauri::State<AppState>,
    instance_id: String,
) -> Result<Vec<crate::plugins::types::PluginParameter>, String> {
    if let Some(inst) = state.plugin_manager.read().get_instance(&instance_id) {
        Ok(inst.get_info().parameters)
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn reorder_plugin_chain(state: tauri::State<AppState>, from_index: usize, to_index: usize) -> Result<(), String> {
    state
        .plugin_manager
        .read()
        .reorder(from_index, to_index)
        .map_err(|e| format!("Failed to reorder plugin chain: {}", e))?;
    crate::app_events::emit_plugin_chain_changed("reorder", None);
    Ok(())
}

#[tauri::command]
pub fn launch_plugin(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    let instance_opt = {
        let manager = state.plugin_manager.read();
        manager.get_instance(&instance_id)
    };
    if let Some(instance) = instance_opt {
        let info = instance.get_info();
        if info.format == PluginFormat::VST3 {
            wait_for_vst3_restore_ready(&*state);
        }
        log::info!(
            "launch_plugin requested: id={}, name='{}', gui_open={} format={:?}",
            instance_id,
            info.name,
            info.gui_open,
            info.format
        );
        instance
            .open_gui()
            .map_err(|e| format!("Failed to open plugin GUI: {}", e))?;
        crate::app_events::emit_plugin_chain_changed("gui_open", Some(&instance_id));
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn launch_plugins(
    state: tauri::State<AppState>,
    instance_ids: Option<Vec<String>>,
) -> Result<LaunchPluginsResult, String> {
    let ids: Vec<String> = match instance_ids {
        Some(v) if !v.is_empty() => v,
        _ => state
            .plugin_manager
            .read()
            .get_instances()
            .into_iter()
            .filter(|p| !matches!(p.format, PluginFormat::Builtin) && !p.gui_open)
            .map(|p| p.instance_id)
            .collect(),
    };

    if ids.is_empty() {
        return Ok(LaunchPluginsResult {
            ok_count: 0,
            skipped_count: 0,
            errors: vec![],
        });
    }

    let mut ok_count = 0usize;
    let mut skipped_count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    let has_vst3 = ids.iter().any(|id| {
        state
            .plugin_manager
            .read()
            .get_instance(id)
            .map(|instance| instance.get_info().format == PluginFormat::VST3)
            .unwrap_or(false)
    });

    if has_vst3 {
        wait_for_vst3_restore_ready(&*state);
    }

    for id in ids {
        let instance_opt = {
            let manager = state.plugin_manager.read();
            manager.get_instance(&id)
        };
        let Some(instance) = instance_opt else {
            errors.push(format!("Unknown instance: {}", id));
            continue;
        };
        let info = instance.get_info();
        if matches!(info.format, PluginFormat::Builtin) {
            skipped_count += 1;
            continue;
        }
        if info.gui_open {
            log::debug!("launch_plugins skip already-open GUI: id={}, name='{}'", id, info.name);
            skipped_count += 1;
            continue;
        }

        log::info!(
            "launch_plugins opening GUI: id={}, name='{}' format={:?}",
            id,
            info.name,
            info.format
        );
        match instance.open_gui() {
            Ok(()) => {
                ok_count += 1;
                crate::app_events::emit_plugin_chain_changed("gui_open", Some(&id));
            }
            Err(e) => errors.push(format!("{}: {}", info.name, e)),
        }
    }

    Ok(LaunchPluginsResult {
        ok_count,
        skipped_count,
        errors,
    })
}

#[tauri::command]
pub fn close_plugins(
    state: tauri::State<AppState>,
    instance_ids: Option<Vec<String>>,
) -> Result<LaunchPluginsResult, String> {
    let ids: Vec<String> = match instance_ids {
        Some(v) if !v.is_empty() => v,
        _ => state
            .plugin_manager
            .read()
            .get_instances()
            .into_iter()
            .filter(|p| p.gui_open)
            .map(|p| p.instance_id)
            .collect(),
    };

    if ids.is_empty() {
        return Ok(LaunchPluginsResult {
            ok_count: 0,
            skipped_count: 0,
            errors: vec![],
        });
    }

    let mut ok_count = 0usize;
    let mut skipped_count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for id in ids {
        let instance_opt = {
            let manager = state.plugin_manager.read();
            manager.get_instance(&id)
        };
        let Some(instance) = instance_opt else {
            errors.push(format!("Unknown instance: {}", id));
            continue;
        };
        let info = instance.get_info();
        if !info.gui_open {
            skipped_count += 1;
            continue;
        }

        if instance.request_close_gui(GUI_CLOSE_TIMEOUT) {
            ok_count += 1;
        } else {
            errors.push(format!("Failed to close GUI for {}", info.name));
        }

        crate::app_events::emit_plugin_chain_changed("gui_open", Some(&id));
    }

    Ok(LaunchPluginsResult {
        ok_count,
        skipped_count,
        errors,
    })
}

#[tauri::command]
pub fn get_plugin_crash_statuses(state: tauri::State<AppState>) -> Result<Vec<PluginCrashStatusItem>, String> {
    let manager = state.plugin_manager.read();
    Ok(manager
        .get_crash_statuses()
        .into_iter()
        .map(|(instance_id, status)| PluginCrashStatusItem { instance_id, status })
        .collect())
}

#[tauri::command]
pub fn get_noise_suppressor_vad(state: tauri::State<AppState>, instance_id: String) -> Result<f32, String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        Ok(instance.get_builtin_vad())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn get_plugin_crash_status(
    state: tauri::State<AppState>,
    instance_id: String,
) -> Result<crate::plugins::PluginStatus, String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        Ok(instance.get_crash_status())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}

#[tauri::command]
pub fn reset_plugin_crash_protection(state: tauri::State<AppState>, instance_id: String) -> Result<(), String> {
    let manager = state.plugin_manager.read();
    if let Some(instance) = manager.get_instance(&instance_id) {
        instance.reset_crash_protection();
        Ok(())
    } else {
        Err(format!("Plugin instance not found: {}", instance_id))
    }
}
