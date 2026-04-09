mod audio;
mod plugins;
mod domain;
mod core;
mod commands;
mod bootstrap;

use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;
use crate::domain::config::ConfigManager;
use crate::domain::preset::PresetManager;
use tauri::Manager;
use audio::AudioManager;
use plugins::{PluginScanner, PluginInstanceManager};

pub(crate) use core::{app_events, session, timing};

// Global audio manager
#[derive(Clone)]
pub(crate) struct AppState {
    audio_manager: Arc<RwLock<AudioManager>>,
    plugin_scanner: Arc<RwLock<PluginScanner>>,
    plugin_manager: Arc<RwLock<PluginInstanceManager>>,
    preset_manager: Arc<RwLock<PresetManager>>,
    config_manager: Arc<RwLock<ConfigManager>>,
    sys_info: Arc<RwLock<sysinfo::System>>,
    /// Autosave-related state.
    autosave: AutosaveState,
    /// Startup/restore-related state (guard, deadlines, etc.).
    startup: StartupState,
}

/// State related to autosave behavior.
#[derive(Clone)]
struct AutosaveState {
    last_hash: Arc<AtomicU64>,
}

/// State related to startup/session restore.
#[derive(Clone)]
struct StartupState {
    session_restored: Arc<AtomicBool>,
    safe_start_deadline: Arc<RwLock<Option<Instant>>>,
    vst3_restore_ready: Arc<AtomicBool>,
}

/// Holds dynamic tray state so commands and tray events stay in sync.
pub(crate) struct TrayState {
    mute_item:             tauri::menu::MenuItem<tauri::Wry>,
    loopback_item:         tauri::menu::MenuItem<tauri::Wry>,
    audio_tray_icon:       tauri::image::Image<'static>,
    audio_tray_icon_muted: tauri::image::Image<'static>,
}

pub(crate) use crate::session::restore_session_impl;

// ── Session persistence helpers ─────────────────────────────────────────────

/// Persist current audio config + mute state to session.json.
/// Called after any audio setting change so the next app launch can restore it.
pub(crate) fn save_audio_session_to_disk(state: &AppState) {
    let config = state.audio_manager.read().get_config();
    let muted  = state.audio_manager.read().is_muted();
    let loopback_enabled = state.audio_manager.read().is_loopback_enabled();
    if let Err(e) = state.config_manager.read().save_session(&config, muted, loopback_enabled) {
        log::warn!("Failed to save audio session: {e}");
    }
}

/// Summary returned to the frontend so it can show a restore notification.
#[derive(serde::Serialize)]
pub struct SessionRestoreResult {
    audio_restored: bool,
    plugins_restored: usize,
    /// True when the output device is a Voicemeeter ASIO Insert driver.
    /// Voicemeeter needs to finish its own startup before our ASIO stream
    /// connects — the frontend schedules `toggle_monitoring(true)` after a
    /// delay rather than doing it here to keep the call on a COM-initialized
    /// Tauri command thread (raw std::thread::spawn threads crash on ASIO).
    needs_deferred_start: bool,
}

 

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let audio_manager  = Arc::new(RwLock::new(AudioManager::new()));
    let plugin_scanner = Arc::new(RwLock::new(PluginScanner::new()));
    let plugin_manager = Arc::new(RwLock::new(PluginInstanceManager::new()));
    let preset_manager = Arc::new(RwLock::new(PresetManager::default()));
    let config_manager = Arc::new(RwLock::new(
        match ConfigManager::new() {
            Ok(cm) => cm,
            Err(e) => {
                log::error!("Failed to initialize config manager: {}. Using in-memory default.", e);
                ConfigManager::default()
            }
        }
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

    let app_state = AppState {
        audio_manager: Arc::clone(&audio_manager),
        plugin_scanner: Arc::clone(&plugin_scanner),
        plugin_manager: Arc::clone(&plugin_manager),
        preset_manager: Arc::clone(&preset_manager),
        config_manager: Arc::clone(&config_manager),
        sys_info: Arc::clone(&sys_info),
        autosave: AutosaveState {
            last_hash: Arc::new(AtomicU64::new(0)),
        },
        startup: StartupState {
            session_restored: Arc::new(AtomicBool::new(false)),
            safe_start_deadline: Arc::new(RwLock::new(None)),
            vst3_restore_ready: Arc::new(AtomicBool::new(true)),
        },
    };

    crate::core::autosave::init_autosave_worker(&app_state);

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
        .manage(app_state)
        .setup(bootstrap::setup)
        .invoke_handler(tauri::generate_handler![
            commands::audio::start_audio,
            commands::audio::stop_audio,
            commands::audio::get_audio_status,
            commands::audio::list_audio_devices,
            commands::audio::get_audio_config,
            commands::audio::set_output_device,
            commands::audio::set_input_device,
            commands::audio::set_virtual_output_device,
            commands::audio::set_sample_rate,
            commands::audio::set_buffer_size,
            commands::audio::toggle_monitoring,
            commands::audio::set_muted,
            commands::audio::set_loopback,
            commands::audio::get_vu_data,
            commands::plugin::scan_plugins,
            commands::plugin::load_plugin,
            commands::plugin::remove_plugin,
            commands::plugin::get_plugin_chain,
            commands::plugin::set_plugin_bypass,
            commands::plugin::set_plugin_parameter,
            commands::plugin::reorder_plugin_chain,
            commands::plugin::swap_plugin_chain,
            commands::plugin::rename_plugin,
            commands::audio::play_test_sound,
            commands::config::get_custom_scan_paths,
            commands::config::add_custom_scan_path,
            commands::config::remove_custom_scan_path,
            commands::config::get_minimize_to_tray,
            commands::config::set_minimize_to_tray,
            commands::config::get_show_app_on_startup,
            commands::config::set_show_app_on_startup,
            commands::startup::is_startup_enabled,
            commands::startup::toggle_startup,
            commands::plugin::launch_plugin,
            commands::plugin::launch_plugins,
            commands::plugin::close_plugins,
            commands::system::get_system_stats,
            commands::plugin::get_plugin_crash_status,
            commands::plugin::get_plugin_crash_statuses,
            commands::plugin::reset_plugin_crash_protection,
            commands::plugin::get_noise_suppressor_vad,
            commands::plugin::get_plugin_parameters,
            commands::system::quit_app,
            commands::session::restore_session,
            commands::system::check_for_update,
            commands::system::install_update,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log::error!("Tauri application error: {}", e);
            std::process::exit(1);
        });
}

