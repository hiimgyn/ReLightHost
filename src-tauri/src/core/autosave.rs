use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

use crate::core::snapshot::{build_chain_preset_from_manager, preset_hash_bytes};
use crate::timing::AUTOSAVE_DEBOUNCE;

#[derive(Debug, Clone, Copy)]
enum AutosaveRequest {
    ChainChanged,
}

static AUTOSAVE_TX: OnceLock<Sender<AutosaveRequest>> = OnceLock::new();

pub fn init_autosave_worker(state: &crate::AppState) {
    if AUTOSAVE_TX.get().is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel::<AutosaveRequest>();
    let plugin_manager = Arc::clone(&state.plugin_manager);
    let preset_manager = Arc::clone(&state.preset_manager);
    let autosave_last_hash = Arc::clone(&state.autosave.last_hash);

    match std::thread::Builder::new()
        .name("autosave-worker".into())
        .spawn(move || run_autosave_worker(rx, plugin_manager, preset_manager, autosave_last_hash))
    {
        Ok(_) => {
            if AUTOSAVE_TX.set(tx).is_err() {
                log::warn!("Autosave worker started but sender was already initialized; using existing worker");
            }
        }
        Err(e) => {
            log::error!("Failed to spawn autosave worker: {e}; autosave will be disabled for this session");
        }
    }
}

pub fn request_plugin_chain_autosave() {
    if let Some(tx) = AUTOSAVE_TX.get() {
        let _ = tx.send(AutosaveRequest::ChainChanged);
    }
}

fn run_autosave_worker(
    rx: Receiver<AutosaveRequest>,
    plugin_manager: Arc<parking_lot::RwLock<crate::plugins::PluginInstanceManager>>,
    preset_manager: Arc<parking_lot::RwLock<crate::domain::preset::PresetManager>>,
    autosave_last_hash: Arc<AtomicU64>,
) {
    while let Ok(_event) = rx.recv() {
        let mut dirty = true;
        while rx.recv_timeout(AUTOSAVE_DEBOUNCE).is_ok() {
            dirty = true;
        }

        if dirty {
            save_autosave_snapshot(&plugin_manager, &preset_manager, &autosave_last_hash);
        }
    }
}

fn save_autosave_snapshot(
    plugin_manager: &Arc<parking_lot::RwLock<crate::plugins::PluginInstanceManager>>,
    preset_manager: &Arc<parking_lot::RwLock<crate::domain::preset::PresetManager>>,
    autosave_last_hash: &Arc<AtomicU64>,
) {
    let preset = build_chain_preset_from_manager(plugin_manager, "autosave");

    let skip_write = preset_hash_bytes(&preset)
        .map(|h| h == autosave_last_hash.load(Ordering::Acquire))
        .unwrap_or(false);

    if skip_write {
        return;
    }

    match preset_manager.read().save_preset(&preset) {
        Ok(_) => {
            if let Some(h) = preset_hash_bytes(&preset) {
                autosave_last_hash.store(h, Ordering::Release);
            }
        }
        Err(e) => log::warn!("Failed to auto-save plugin chain: {e}"),
    }
}
