use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

use crate::core::snapshot::{build_chain_preset_from_manager, preset_hash_bytes};
use crate::timing::AUTOSAVE_DEBOUNCE;

#[derive(Debug, Clone, Copy)]
enum AutosaveRequest {
    ChainChanged,
    Shutdown,
}

static AUTOSAVE_TX: OnceLock<Sender<AutosaveRequest>> = OnceLock::new();

pub(crate) fn init_autosave_worker(state: &crate::AppState) {
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

/// True while `restore_session_impl` (and its deferred VST3 replay) is
/// rebuilding the chain. Snapshots taken then would capture an empty or
/// default-state chain and overwrite the real autosave.
static RESTORE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub fn set_restore_in_progress(active: bool) {
    RESTORE_IN_PROGRESS.store(active, Ordering::Release);
}

pub fn request_plugin_chain_autosave() {
    if let Some(tx) = AUTOSAVE_TX.get() {
        let _ = tx.send(AutosaveRequest::ChainChanged);
    }
}

pub fn shutdown_autosave_worker() {
    if let Some(tx) = AUTOSAVE_TX.get() {
        let _ = tx.send(AutosaveRequest::Shutdown);
    }
}

fn run_autosave_worker(
    rx: Receiver<AutosaveRequest>,
    plugin_manager: Arc<crate::plugins::PluginInstanceManager>,
    preset_manager: Arc<parking_lot::RwLock<crate::domain::preset::PresetManager>>,
    autosave_last_hash: Arc<AtomicU64>,
) {
    loop {
        match rx.recv() {
            Ok(AutosaveRequest::Shutdown) | Err(_) => break,
            Ok(AutosaveRequest::ChainChanged) => {
                // Debounce: drain any additional requests within the window.
                loop {
                    match rx.recv_timeout(AUTOSAVE_DEBOUNCE) {
                        Ok(AutosaveRequest::Shutdown) => return,
                        Ok(AutosaveRequest::ChainChanged) => continue,
                        Err(_) => break,
                    }
                }
                save_autosave_snapshot(&plugin_manager, &preset_manager, &autosave_last_hash);
            }
        }
    }
}

fn save_autosave_snapshot(
    plugin_manager: &Arc<crate::plugins::PluginInstanceManager>,
    preset_manager: &Arc<parking_lot::RwLock<crate::domain::preset::PresetManager>>,
    autosave_last_hash: &Arc<AtomicU64>,
) {
    if RESTORE_IN_PROGRESS.load(Ordering::Acquire) {
        return;
    }
    let preset = build_chain_preset_from_manager(plugin_manager, "autosave");
    let hash = preset_hash_bytes(&preset);

    let skip_write = hash
        .map(|h| h == autosave_last_hash.load(Ordering::Acquire))
        .unwrap_or(false);

    if skip_write {
        return;
    }

    match preset_manager.read().save_preset(&preset) {
        Ok(_) => {
            if let Some(h) = hash {
                autosave_last_hash.store(h, Ordering::Release);
            }
        }
        Err(e) => log::warn!("Failed to auto-save plugin chain: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_skipped_while_restore_is_in_progress() {
        let dir = std::env::temp_dir().join(format!("rh-autosave-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("autosave.json");
        let _ = std::fs::remove_file(&file);
        let presets = Arc::new(parking_lot::RwLock::new(crate::domain::preset::PresetManager::with_dir(dir.clone())));
        let plugins = Arc::new(crate::plugins::PluginInstanceManager::new());
        let hash = Arc::new(AtomicU64::new(0));

        set_restore_in_progress(true);
        save_autosave_snapshot(&plugins, &presets, &hash);
        assert!(!file.exists(), "autosave wrote a snapshot mid-restore");

        set_restore_in_progress(false);
        save_autosave_snapshot(&plugins, &presets, &hash);
        assert!(file.exists(), "autosave did not resume after restore finished");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
