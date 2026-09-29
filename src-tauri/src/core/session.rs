use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::timing::{VST3_STATE_REPLAY_DELAY, VST3_STARTUP_DELAY_MS, VOICEMEETER_STARTUP_DELAY_MS, VST3_POST_START_SETTLE_MS};

pub(crate) fn restore_session_impl(
    state: &crate::AppState,
    app: &tauri::AppHandle<tauri::Wry>,
) -> Result<crate::SessionRestoreResult, String> {
    use crate::plugins::PluginInfo;
    let restore_t0 = Instant::now();

    // Guard: React StrictMode calls effects twice in development.
    // The compare_exchange ensures only the first call does real work;
    // the second is a fast no-op that returns the same shape of result.
    if state.startup.session_restored.compare_exchange(
        false, true, Ordering::SeqCst, Ordering::SeqCst
    ).is_err() {
        log::info!(
            "{} restore_session: already restored, skipping duplicate call",
            crate::core::threading::thread_prefix("restore/guard")
        );
        let plugins_restored = state.plugin_manager.get_instances().len();
        return Ok(crate::SessionRestoreResult {
            audio_restored: state.audio_manager.get_status().is_monitoring,
            plugins_restored,
            needs_deferred_start: false,
            deferred_start_ms: 0,
        });
    }
    log::info!("{} Session restore started", crate::core::threading::thread_prefix("restore/main"));
    // Cleared below once the chain is complete — at the end of this function,
    // or by the vst3-replay thread when state replay is deferred.
    crate::core::autosave::set_restore_in_progress(true);

    let mut audio_restored: bool = false;
    let mut plugins_restored: usize = 0;
    let mut safe_delay_ms: u64 = 0;
    let mut safe_start_deadline: Option<Instant> = None;
    let mut monitoring_started_early = false;
    // ── 1. Audio config (stop stream only — do NOT restart yet) ───────────
    if let Some(session) = state.config_manager.load_session() {
        // Ensure any pre-opened stream is stopped before we swap device config.
        let _ = state.audio_manager.toggle_monitoring(false);
        state.audio_manager.restore_config(session.audio);
        state.audio_manager.set_muted(session.muted);
        let tray_state = app.state::<crate::TrayState>();
        crate::bootstrap::tray::sync_audio_tray_state(app, &tray_state, session.muted);
        let _ = state.audio_manager.set_loopback(session.loopback_enabled);
        audio_restored = true;
        log::info!("{} ✅ Audio session restored", crate::core::threading::thread_prefix("restore/main"));
    }

    // buffer handed to Voicemeeter Insert already has the full chain active.
    // Guard: do not reload if the chain already has items (StrictMode double-invoke).
    let chain_empty = state.plugin_manager.get_instances().is_empty();
    if chain_empty {
        if let Ok(preset) = state.preset_manager.restore_auto_save() {
            let plugin_restore_t0 = Instant::now();
            let config = state.audio_manager.get_config();
            let restored_has_vst3 = preset
                .plugin_chain
                .iter()
                .any(|plugin| plugin.plugin_format == Some(crate::plugins::PluginFormat::VST3));
            let is_voicemeeter = config
                .output_device_id
                .as_deref()
                .map(|id| id.to_lowercase().contains("voicemeeter"))
                .unwrap_or(false);

            state.plugin_manager.clear();

            if restored_has_vst3 {
                safe_delay_ms = safe_delay_ms.max(VST3_STARTUP_DELAY_MS);
            }
            if is_voicemeeter {
                safe_delay_ms = safe_delay_ms.max(VOICEMEETER_STARTUP_DELAY_MS);
            }

            if audio_restored && safe_delay_ms > 0 {
                safe_start_deadline = Some(Instant::now() + Duration::from_millis(safe_delay_ms));
                // Extra guard after stream start: skip VST3 process() during fragile warmup.
                // total guard = delayed-start wait + additional post-start settling window.
                let extra_post_start_ms = if restored_has_vst3 { VST3_POST_START_SETTLE_MS } else { 0 };
                crate::plugins::processor::vst3::set_global_process_block_ms(
                    safe_delay_ms.saturating_add(extra_post_start_ms)
                );
                log::info!(
                    "{} Scheduled backend safe delayed start: {} ms (vst3={}, voicemeeter={})",
                    crate::core::threading::thread_prefix("restore/main"),
                    safe_delay_ms,
                    restored_has_vst3,
                    is_voicemeeter
                );

                if restored_has_vst3 && !is_voicemeeter {
                    if let Err(e) = state.audio_manager.toggle_monitoring(true) {
                        log::warn!("{} Failed to early-start monitoring before VST3 restore: {e}", crate::core::threading::thread_prefix("restore/main"));
                    } else {
                        monitoring_started_early = true;
                        log::info!("{} Started monitoring early before VST3 restore load phase", crate::core::threading::thread_prefix("restore/main"));
                    }
                } else {
                    log::info!("{} Automatic monitoring will remain deferred until restore completion", crate::core::threading::thread_prefix("restore/main"));
                }
            } else {
                crate::plugins::processor::vst3::set_global_process_block_ms(0);
            }

            // Collect VST3 blobs + params to replay after load phase completes
            let mut vst3_replays: Vec<(String, Option<Vec<u8>>, Vec<crate::domain::preset::PresetParameter>)> = Vec::new();

            // The stream may already be running (early start) at a rate an
            // ASIO driver chose over the configured one.
            let sample_rate = state.audio_manager.processing_rate();
            let buffer_size = config.buffer_size;

            let plan = preset_load_plan(&preset);
            let infos: Vec<PluginInfo> = plan.iter().map(|(info, _)| info.clone()).collect();

            // The frontend needs the total *before* the (potentially
            // multi-second, e.g. a plugin loading its own ML model)  load
            // phase below, not after — restore_session doesn't return until
            // that phase is done, so waiting for its result to learn the
            // target would only ever show it once there's nothing left to
            // show progress for. `restore_progress` events (emitted per
            // plugin inside load_plugins_parallel_results) carry the running
            // count; this carries the denominator.
            crate::app_events::emit_plugin_chain_changed("restore_total", Some(&infos.len().to_string()));

            let parallel_vst3 = state.config_manager.get_parallel_vst3_loading();
            let results = state
                .plugin_manager
                .load_plugins_parallel_results(infos, sample_rate, buffer_size as usize, parallel_vst3);

            log::info!(
                "{} Session restore load phase completed in {} ms",
                crate::core::threading::thread_prefix("restore/load"),
                plugin_restore_t0.elapsed().as_millis()
            );

            for ((info, saved), res) in plan.iter().zip(results) {
                let instance_id = match res {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("{} Session restore — skipped plugin '{}': {e}", crate::core::threading::thread_prefix("restore/load"), info.name);
                        continue;
                    }
                };

                plugins_restored += 1;
                if let Some(instance) = state.plugin_manager.get_instance(&instance_id) {
                    instance.set_bypassed(saved.bypassed);
                    if info.format == crate::plugins::PluginFormat::VST3 {
                        vst3_replays.push((instance_id.clone(), saved.vst3_state.clone(), saved.parameters.clone()));
                        log::debug!("{} Deferred VST3 state for '{}', will replay after load", crate::core::threading::thread_prefix("restore/load"), info.name);
                    } else {
                        apply_saved_settings(&instance, saved);
                    }
                }
            }

            if plugins_restored > 0 {
                log::info!("{} ✅ Plugin chain restored: {} plugins", crate::core::threading::thread_prefix("restore/load"), plugins_restored);
            }

            // Replay VST3 binary state + parameters on a background thread
            if !vst3_replays.is_empty() {
                state.startup.vst3_restore_ready.store(false, Ordering::Release);
                let plugin_manager = Arc::clone(&state.plugin_manager);
                let vst3_restore_ready = Arc::clone(&state.startup.vst3_restore_ready);
                let vst3_restore_ready_fallback = Arc::clone(&state.startup.vst3_restore_ready);
                match std::thread::Builder::new()
                    .name("vst3-replay".to_string())
                    .spawn(move || {
                    let finished = crate::core::autosave::RestoreFinishGuard::new(vst3_restore_ready);
                    log::info!(
                        "{} VST3 replay thread started ({} item(s))",
                        crate::core::threading::thread_prefix("restore/replay"),
                        vst3_replays.len()
                    );
                    // Wait a short time to let plugin load/initialization stabilise.
                    std::thread::sleep(VST3_STATE_REPLAY_DELAY);
                    for (inst_id, opt_blob, params) in vst3_replays.into_iter() {
                        if let Some(inst) = plugin_manager.get_instance(&inst_id) {
                            if let Some(blob) = opt_blob {
                                // set_state_binary handles COM init where required.
                                inst.set_state_binary(&blob);
                            }
                            for p in params {
                                inst.set_parameter(p.id, p.value);
                            }
                        }
                    }

                    // Only emit the startup chain-changed event after VST3 replay
                    // completes so autosave cannot capture a partially restored state.
                    drop(finished);
                    log::info!("{} VST3 replay thread finished", crate::core::threading::thread_prefix("restore/replay"));
                    crate::app_events::emit_plugin_chain_changed("restore_session_vst3_replay_done", None);
                }) {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!(
                            "{} Failed to spawn VST3 replay thread: {e}; continuing without deferred replay",
                            crate::core::threading::thread_prefix("restore/replay")
                        );
                        vst3_restore_ready_fallback.store(true, Ordering::Release);
                        crate::app_events::emit_plugin_chain_changed("restore_session_vst3_replay_failed", None);
                    }
                }
            } else if plugins_restored > 0 {
                state.startup.vst3_restore_ready.store(true, Ordering::Release);
                crate::app_events::emit_plugin_chain_changed("restore_session", None);
            }
        }
    }

    // ── 3. Start stream after restore (or earlier for VST3 sessions) ─────
    //
    // ASIO COM rule: toggle_monitoring must always be called from a thread
    // that has COM initialized (i.e. a Tauri command handler thread).
    // Raw std::thread::spawn threads are NOT COM-initialized and will crash
    // with STATUS_ACCESS_VIOLATION on ASIO drivers.
    //
    // For VST3 restores, we now try to bring monitoring up before the plugin
    // load/replay phase so the startup order matches the manual add path more
    // closely. Voicemeeter still stays deferred because it needs its own warmup.
    let needs_deferred_start = audio_restored && safe_delay_ms > 0 && !monitoring_started_early;

    if audio_restored {
        if monitoring_started_early {
            log::info!("{} Monitoring already started early during VST3 restore", crate::core::threading::thread_prefix("restore/main"));
        } else if !needs_deferred_start {
            if let Err(e) = state.audio_manager.toggle_monitoring(true) {
                log::warn!("{} Failed to auto-start monitoring on session restore: {e}", crate::core::threading::thread_prefix("restore/main"));
            }
        } else {
            log::info!("{} Automatic monitoring deferred; frontend can call toggleMonitoring(true) immediately and backend will gate start", crate::core::threading::thread_prefix("restore/main"));
        }
    }

    // If VST3 replay was deferred, the background thread emits the chain event
    // after the replay completes. Otherwise, emit it immediately here.

    // No deferred VST3 replay pending (none needed, or its thread failed to
    // spawn) → the chain is complete now; save it once.
    if state.startup.vst3_restore_ready.load(Ordering::Acquire) {
        // Monitoring may have started after the plugins loaded, at a rate
        // an ASIO driver chose — bring the chain in line before saving it.
        crate::commands::audio::sync_chain_to_audio_rate(state);
        crate::core::autosave::set_restore_in_progress(false);
        crate::core::autosave::request_plugin_chain_autosave();
    }

    let total_ms = restore_t0.elapsed().as_millis();
    log::info!(
        "{} Session restore finished in {} ms (audio_restored={}, plugins_restored={})",
        crate::core::threading::thread_prefix("restore/main"),
        total_ms,
        audio_restored,
        plugins_restored
    );
    if total_ms > 5000 {
        log::warn!("{} Session restore is slow ({} ms). Check per-plugin load timings above to identify bottlenecks.", crate::core::threading::thread_prefix("restore/main"), total_ms);
    }

    let deferred_start_ms = match (needs_deferred_start, safe_start_deadline) {
        (true, Some(deadline)) => deadline.saturating_duration_since(Instant::now()).as_millis() as u64,
        _ => 0,
    };
    Ok(crate::SessionRestoreResult { audio_restored, plugins_restored, needs_deferred_start, deferred_start_ms })
}

/// The plugins a preset can load (entries without a path/format are
/// skipped), each paired with its saved settings.
fn preset_load_plan(preset: &crate::domain::preset::Preset) -> Vec<(crate::plugins::PluginInfo, &crate::domain::preset::PresetPlugin)> {
    preset
        .plugin_chain
        .iter()
        .filter_map(|p| {
            let (Some(path), Some(format)) = (p.plugin_path.as_ref(), p.plugin_format) else {
                return None;
            };
            let info = crate::plugins::PluginInfo {
                id:       p.plugin_id.clone(),
                name:     p.plugin_name.clone(),
                vendor:   p.plugin_vendor.clone().unwrap_or_default(),
                version:  p.plugin_version.clone().unwrap_or_default(),
                path:     path.clone(),
                format,
                category: p.plugin_category.clone().unwrap_or_default(),
            };
            Some((info, p))
        })
        .collect()
}

/// Binary state, then parameters (parameters win where both set a value).
fn apply_saved_settings(instance: &crate::plugins::core::instance::PluginInstance, saved: &crate::domain::preset::PresetPlugin) {
    if let Some(ref blob) = saved.vst3_state {
        instance.set_state_binary(blob);
    }
    for p in &saved.parameters {
        instance.set_parameter(p.id, p.value);
    }
}

/// Replaces the whole chain with `preset`'s plugins and applies their
/// saved bypass, state and parameters right away (user-initiated preset
/// load; session restore defers VST3 state instead). Returns how many
/// plugins loaded.
// ponytail: clears first, so audio passes through dry while the preset
// loads; build-then-swap if that gap turns out to matter.
pub(crate) fn load_preset_into_chain(
    plugin_manager: &crate::plugins::PluginInstanceManager,
    preset: &crate::domain::preset::Preset,
    sample_rate: f64,
    block_size: usize,
    parallel_vst3: bool,
) -> usize {
    let plan = preset_load_plan(preset);
    let infos = plan.iter().map(|(info, _)| info.clone()).collect();
    plugin_manager.clear();
    let results = plugin_manager.load_plugins_parallel_results(infos, sample_rate, block_size, parallel_vst3);
    let mut loaded = 0;
    for ((info, saved), res) in plan.iter().zip(results) {
        match res.ok().and_then(|id| plugin_manager.get_instance(&id)) {
            Some(instance) => {
                instance.set_bypassed(saved.bypassed);
                apply_saved_settings(&instance, saved);
                loaded += 1;
            }
            None => log::warn!("Preset load skipped plugin '{}'", info.name),
        }
    }
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{PluginFormat, PluginInfo, PluginInstanceManager};

    fn compressor() -> PluginInfo {
        PluginInfo {
            id: "builtin::compressor".into(),
            name: "Compressor".into(),
            vendor: String::new(),
            version: String::new(),
            path: crate::plugins::builtin::compressor::ID.into(),
            format: PluginFormat::Builtin,
            category: String::new(),
        }
    }

    #[test]
    fn loading_a_preset_replaces_the_chain_and_applies_its_settings() {
        let source = Arc::new(PluginInstanceManager::new());
        let id = source.load_plugin(compressor(), 48_000.0, 512).unwrap();
        let inst = source.get_instance(&id).unwrap();
        inst.set_parameter(4, 12.0);
        inst.set_bypassed(true);
        let preset = crate::core::snapshot::build_chain_preset_from_manager(&source, "p");

        let target = PluginInstanceManager::new();
        target.load_plugin(compressor(), 48_000.0, 512).unwrap();
        target.load_plugin(compressor(), 48_000.0, 512).unwrap();

        assert_eq!(load_preset_into_chain(&target, &preset, 48_000.0, 512, false), 1);
        let chain = target.get_instances();
        assert_eq!(chain.len(), 1);
        assert!(chain[0].bypassed);
        assert_eq!(chain[0].parameters.iter().find(|p| p.id == 4).unwrap().value, 12.0);
    }
}
