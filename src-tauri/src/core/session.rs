use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::timing::{VST3_STATE_REPLAY_DELAY, VST3_STARTUP_DELAY_MS, VOICEMEETER_STARTUP_DELAY_MS, VST3_POST_START_SETTLE_MS};

pub fn restore_session_impl(state: &crate::AppState) -> Result<crate::SessionRestoreResult, String> {
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
        let plugins_restored = state.plugin_manager.read().get_instances().len();
        return Ok(crate::SessionRestoreResult {
            audio_restored: state.audio_manager.read().get_status().is_monitoring,
            plugins_restored,
            needs_deferred_start: false,
        });
    }
    log::info!("{} Session restore started", crate::core::threading::thread_prefix("restore/main"));

    let mut audio_restored: bool = false;
    let mut plugins_restored: usize = 0;
    let mut safe_delay_ms: u64 = 0;
    let mut monitoring_started_early = false;
    // ── 1. Audio config (stop stream only — do NOT restart yet) ───────────
    if let Some(session) = state.config_manager.read().load_session() {
        // Ensure any pre-opened stream is stopped before we swap device config.
        let _ = state.audio_manager.read().toggle_monitoring(false);
        state.audio_manager.read().restore_config(session.audio);
        state.audio_manager.read().set_muted(session.muted);
        let _ = state.audio_manager.read().set_loopback(session.loopback_enabled);
        audio_restored = true;
        log::info!("{} ✅ Audio session restored", crate::core::threading::thread_prefix("restore/main"));
    }

    // buffer handed to Voicemeeter Insert already has the full chain active.
    // Guard: do not reload if the chain already has items (StrictMode double-invoke).
    let chain_empty = state.plugin_manager.read().get_instances().is_empty();
    if chain_empty {
        if let Ok(preset) = state.preset_manager.read().restore_auto_save() {
            let plugin_restore_t0 = Instant::now();
            let config = state.audio_manager.read().get_config();
            let restored_has_vst3 = preset
                .plugin_chain
                .iter()
                .any(|plugin| plugin.plugin_format == Some(crate::plugins::PluginFormat::VST3));
            let is_voicemeeter = config
                .output_device_id
                .as_deref()
                .map(|id| id.to_lowercase().contains("voicemeeter"))
                .unwrap_or(false);

            state.plugin_manager.read().clear();

            if restored_has_vst3 {
                safe_delay_ms = safe_delay_ms.max(VST3_STARTUP_DELAY_MS);
            }
            if is_voicemeeter {
                safe_delay_ms = safe_delay_ms.max(VOICEMEETER_STARTUP_DELAY_MS);
            }

            if audio_restored && safe_delay_ms > 0 {
                *state.startup.safe_start_deadline.write() = Some(Instant::now() + Duration::from_millis(safe_delay_ms));
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
                    if let Err(e) = state.audio_manager.read().toggle_monitoring(true) {
                        log::warn!("{} Failed to early-start monitoring before VST3 restore: {e}", crate::core::threading::thread_prefix("restore/main"));
                    } else {
                        monitoring_started_early = true;
                        log::info!("{} Started monitoring early before VST3 restore load phase", crate::core::threading::thread_prefix("restore/main"));
                    }
                } else {
                    log::info!("{} Automatic monitoring will remain deferred until restore completion", crate::core::threading::thread_prefix("restore/main"));
                }
            } else {
                *state.startup.safe_start_deadline.write() = None;
                crate::plugins::processor::vst3::set_global_process_block_ms(0);
            }

            // Collect VST3 blobs + params to replay after load phase completes
            let mut vst3_replays: Vec<(String, Option<Vec<u8>>, Vec<crate::domain::preset::PresetParameter>)> = Vec::new();

            let sample_rate = config.sample_rate as f64;
            let buffer_size = config.buffer_size;

            let mut infos: Vec<PluginInfo> = Vec::new();
            let mut restore_rows: Vec<(
                bool,
                crate::plugins::PluginFormat,
                String,
                Option<Vec<u8>>,
                Vec<crate::domain::preset::PresetParameter>,
            )> = Vec::new();

            for plugin_preset in &preset.plugin_chain {
                let (Some(path), Some(format)) =
                    (plugin_preset.plugin_path.as_ref(), plugin_preset.plugin_format)
                else {
                    continue;
                };
                infos.push(PluginInfo {
                    id:       plugin_preset.plugin_id.clone(),
                    name:     plugin_preset.plugin_name.clone(),
                    vendor:   plugin_preset.plugin_vendor.clone().unwrap_or_default(),
                    version:  plugin_preset.plugin_version.clone().unwrap_or_default(),
                    path:     path.clone(),
                    format,
                    category: plugin_preset.plugin_category.clone().unwrap_or_default(),
                });
                restore_rows.push((
                    plugin_preset.bypassed,
                    format,
                    plugin_preset.plugin_name.clone(),
                    plugin_preset.vst3_state.clone(),
                    plugin_preset.parameters.clone(),
                ));
            }

            let results = state
                .plugin_manager
                .read()
                .load_plugins_parallel_results(infos, sample_rate, buffer_size as usize);

            log::info!(
                "{} Session restore load phase completed in {} ms",
                crate::core::threading::thread_prefix("restore/load"),
                plugin_restore_t0.elapsed().as_millis()
            );

            for ((bypassed, format, plugin_name, vst3_state, parameters), res) in
                restore_rows.into_iter().zip(results)
            {
                let instance_id = match res {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("{} Session restore — skipped plugin '{plugin_name}': {e}", crate::core::threading::thread_prefix("restore/load"));
                        continue;
                    }
                };

                plugins_restored += 1;
                let restoring_vst3 = format == crate::plugins::PluginFormat::VST3;

                if let Some(instance) = state.plugin_manager.read().get_instance(&instance_id) {
                    instance.set_bypassed(bypassed);

                    if restoring_vst3 {
                        vst3_replays.push((
                            instance_id.clone(),
                            vst3_state.clone(),
                            parameters.clone(),
                        ));
                        log::debug!("{} Deferred VST3 state for '{}', will replay after load", crate::core::threading::thread_prefix("restore/load"), plugin_name);
                    } else {
                        if let Some(ref blob) = vst3_state {
                            instance.set_state_binary(blob);
                        }
                        for p in &parameters {
                            instance.set_parameter(p.id, p.value);
                        }
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
                std::thread::Builder::new()
                    .name("vst3-replay".to_string())
                    .spawn(move || {
                    log::info!(
                        "{} VST3 replay thread started ({} item(s))",
                        crate::core::threading::thread_prefix("restore/replay"),
                        vst3_replays.len()
                    );
                    // Wait a short time to let plugin load/initialization stabilise.
                    std::thread::sleep(VST3_STATE_REPLAY_DELAY);
                    for (inst_id, opt_blob, params) in vst3_replays.into_iter() {
                        if let Some(inst) = plugin_manager.read().get_instance(&inst_id) {
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
                    vst3_restore_ready.store(true, Ordering::Release);
                    log::info!("{} VST3 replay thread finished", crate::core::threading::thread_prefix("restore/replay"));
                    crate::app_events::emit_plugin_chain_changed("restore_session_vst3_replay_done", None);
                }).expect("failed to spawn VST3 replay thread");
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
            if let Err(e) = state.audio_manager.read().toggle_monitoring(true) {
                log::warn!("{} Failed to auto-start monitoring on session restore: {e}", crate::core::threading::thread_prefix("restore/main"));
            }
        } else {
            log::info!("{} Automatic monitoring deferred; frontend can call toggleMonitoring(true) immediately and backend will gate start", crate::core::threading::thread_prefix("restore/main"));
        }
    }

    // If VST3 replay was deferred, the background thread emits the chain event
    // after the replay completes. Otherwise, emit it immediately here.

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

    Ok(crate::SessionRestoreResult { audio_restored, plugins_restored, needs_deferred_start })
}
