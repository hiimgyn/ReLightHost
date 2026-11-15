use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

use crate::core::snapshot::build_chain_preset_from_manager;
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

/// Holds `RESTORE_IN_PROGRESS` set while alive and clears it on drop —
/// including when a panic unwinds through the restore/preset load, which
/// would otherwise disable autosave (and the exit save) for the session.
pub(crate) struct RestoreInProgress(());

impl RestoreInProgress {
    pub(crate) fn begin() -> Self {
        set_restore_in_progress(true);
        Self(())
    }
}

impl Drop for RestoreInProgress {
    fn drop(&mut self) {
        set_restore_in_progress(false);
    }
}

/// Marks the deferred VST3 replay finished when dropped — including when
/// the replay thread unwinds from a panic, which would otherwise leave
/// autosave (and GUI launch) blocked for the rest of the session.
pub(crate) struct RestoreFinishGuard {
    vst3_restore_ready: Arc<std::sync::atomic::AtomicBool>,
}

impl RestoreFinishGuard {
    pub(crate) fn new(vst3_restore_ready: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { vst3_restore_ready }
    }
}

impl Drop for RestoreFinishGuard {
    fn drop(&mut self) {
        self.vst3_restore_ready.store(true, Ordering::Release);
        set_restore_in_progress(false);
    }
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

/// Exit path: close every plugin GUI (so VST3 state is readable and the
/// latest GUI edits are in it), write the snapshot synchronously — a change
/// still inside the worker's debounce window would otherwise be lost — then
/// stop the worker.
pub(crate) fn flush_on_exit(
    plugin_manager: &Arc<crate::plugins::PluginInstanceManager>,
    preset_manager: &Arc<crate::domain::preset::PresetManager>,
    autosave_last_hash: &Arc<AtomicU64>,
) {
    for instance in plugin_manager.get_instances_arc() {
        instance.request_close_gui(crate::timing::GUI_CLOSE_TIMEOUT);
    }
    save_autosave_snapshot(plugin_manager, preset_manager, autosave_last_hash);
    shutdown_autosave_worker();
}

fn run_autosave_worker(
    rx: Receiver<AutosaveRequest>,
    plugin_manager: Arc<crate::plugins::PluginInstanceManager>,
    preset_manager: Arc<crate::domain::preset::PresetManager>,
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
    preset_manager: &Arc<crate::domain::preset::PresetManager>,
    autosave_last_hash: &Arc<AtomicU64>,
) {
    save_snapshot_with(|| build_chain_preset_from_manager(plugin_manager, "autosave"), preset_manager, autosave_last_hash);
}

/// Serialises the hash check + write (the worker and the exit flush share a
/// tmp file) and holds the sequence number of the last snapshot written.
/// Never held while a snapshot is built: building reads plugin state on the
/// plugin host thread, and the exit flush saves *from* that thread — holding
/// it across the build deadlocked exit (see core::host_thread).
static SAVE_LOCK: parking_lot::Mutex<u64> = parking_lot::Mutex::new(0);
/// Order in which snapshots started building; a later one wins.
static SNAPSHOT_SEQ: AtomicU64 = AtomicU64::new(0);

fn save_snapshot_with(
    build: impl FnOnce() -> crate::domain::preset::Preset,
    preset_manager: &Arc<crate::domain::preset::PresetManager>,
    autosave_last_hash: &Arc<AtomicU64>,
) {
    if RESTORE_IN_PROGRESS.load(Ordering::Acquire) {
        return;
    }
    let seq = SNAPSHOT_SEQ.fetch_add(1, Ordering::AcqRel) + 1;
    let mut preset = build();
    // Hash without the timestamp — it changes on every snapshot, so the
    // dedupe would never match. Serialized again with it only on a change.
    let created_at = std::mem::take(&mut preset.created_at);
    let hash = match preset.to_json() {
        Ok(j) => {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            j.hash(&mut h);
            h.finish()
        }
        Err(e) => {
            log::warn!("Failed to serialize auto-save: {e}");
            return;
        }
    };
    preset.created_at = created_at;
    let json = match preset.to_json() {
        Ok(j) => j,
        Err(e) => {
            log::warn!("Failed to serialize auto-save: {e}");
            return;
        }
    };

    let mut last_written = SAVE_LOCK.lock();
    if seq < *last_written {
        return; // a newer snapshot is already on disk
    }
    if hash == autosave_last_hash.load(Ordering::Acquire) {
        *last_written = seq;
        return;
    }
    match preset_manager.save_preset_json(&preset.name, &json) {
        Ok(_) => {
            *last_written = seq;
            autosave_last_hash.store(hash, Ordering::Release);
        }
        Err(e) => log::warn!("Failed to auto-save plugin chain: {e}"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Tests that touch the process-wide RESTORE_IN_PROGRESS flag (also
    /// taken by core::session's preset-load test).
    pub(crate) static FLAG_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn snapshot_is_skipped_while_restore_is_in_progress() {
        let _flag = FLAG_LOCK.lock();
        let dir = std::env::temp_dir().join(format!("rh-autosave-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("autosave.json");
        let _ = std::fs::remove_file(&file);
        let presets = Arc::new(crate::domain::preset::PresetManager::with_dir(dir.clone()));
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

    #[test]
    fn restore_is_marked_finished_even_if_the_replay_thread_panics() {
        let _flag = FLAG_LOCK.lock();
        let ready = Arc::new(AtomicBool::new(false));
        set_restore_in_progress(true);
        let guard = RestoreFinishGuard::new(Arc::clone(&ready));
        let _ = std::thread::spawn(move || {
            let _guard = guard;
            panic!("plugin replay blew up");
        })
        .join();
        assert!(ready.load(Ordering::Acquire));
        assert!(!RESTORE_IN_PROGRESS.load(Ordering::Acquire), "autosave would stay disabled");
    }

    #[test]
    fn restore_flag_is_cleared_when_a_restore_panics() {
        let _flag = FLAG_LOCK.lock();
        let _ = std::panic::catch_unwind(|| {
            let _restoring = RestoreInProgress::begin();
            assert!(RESTORE_IN_PROGRESS.load(Ordering::Acquire));
            panic!("plugin load blew up");
        });
        assert!(!RESTORE_IN_PROGRESS.load(Ordering::Acquire), "autosave would stay disabled");
    }

    #[test]
    fn unchanged_chain_is_not_rewritten() {
        let _flag = FLAG_LOCK.lock();
        let dir = std::env::temp_dir().join(format!("rh-autosave-dedupe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("autosave.json");
        let presets = Arc::new(crate::domain::preset::PresetManager::with_dir(dir.clone()));
        let plugins = Arc::new(crate::plugins::PluginInstanceManager::new());
        let hash = Arc::new(AtomicU64::new(0));
        save_autosave_snapshot(&plugins, &presets, &hash);
        std::fs::remove_file(&file).unwrap();
        save_autosave_snapshot(&plugins, &presets, &hash);
        assert!(!file.exists(), "identical chain rewritten (timestamp in the hash?)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The exit deadlock: the worker builds a snapshot (reading plugin state
    /// on the host thread) while the host thread runs the exit flush, which
    /// saves too. Neither may wait on the other.
    #[test]
    fn a_save_on_the_host_thread_never_waits_on_a_save_that_is_building() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let _flag = FLAG_LOCK.lock();
        let dir = std::env::temp_dir().join(format!("rh-autosave-deadlock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let presets = Arc::new(crate::domain::preset::PresetManager::with_dir(dir.clone()));
        let hash = Arc::new(AtomicU64::new(0));
        let snapshot = || crate::core::snapshot::build_chain_preset_from_manager(&Arc::new(crate::plugins::PluginInstanceManager::new()), "autosave");

        let (building_tx, building_rx) = channel();
        let (host_done_tx, host_done_rx) = channel();
        // Host job: waits until the worker is mid-build, then saves (exit flush).
        {
            let (presets, hash) = (Arc::clone(&presets), Arc::clone(&hash));
            crate::core::host_thread::post(move || {
                let _ = building_rx.recv_timeout(Duration::from_secs(5));
                save_snapshot_with(snapshot, &presets, &hash);
                let _ = host_done_tx.send(());
            });
        }
        // Worker: its build reads plugin state on the (busy) host thread.
        let (worker_done_tx, worker_done_rx) = channel();
        {
            let (presets, hash) = (Arc::clone(&presets), Arc::clone(&hash));
            std::thread::spawn(move || {
                save_snapshot_with(
                    || {
                        building_tx.send(()).unwrap();
                        crate::core::host_thread::run_blocking(|| ());
                        snapshot()
                    },
                    &presets,
                    &hash,
                );
                let _ = worker_done_tx.send(());
            });
        }
        assert!(host_done_rx.recv_timeout(Duration::from_secs(10)).is_ok(), "exit save deadlocked against the worker");
        assert!(worker_done_rx.recv_timeout(Duration::from_secs(10)).is_ok(), "worker save deadlocked");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_snapshot_finished_after_a_newer_one_is_discarded() {
        let _flag = FLAG_LOCK.lock();
        let dir = std::env::temp_dir().join(format!("rh-autosave-order-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let presets = Arc::new(crate::domain::preset::PresetManager::with_dir(dir.clone()));
        let hash = Arc::new(AtomicU64::new(0));
        let snapshot = |d: &str| {
            let mut p = crate::core::snapshot::build_chain_preset_from_manager(&Arc::new(crate::plugins::PluginInstanceManager::new()), "autosave");
            p.description = d.into();
            p
        };
        // The older snapshot starts building; a newer one is built and written meanwhile.
        save_snapshot_with(
            || {
                save_snapshot_with(|| snapshot("newer"), &presets, &hash);
                snapshot("older")
            },
            &presets,
            &hash,
        );
        let saved = std::fs::read_to_string(dir.join("autosave.json")).unwrap();
        assert!(saved.contains("newer"), "stale snapshot overwrote a newer one: {saved}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flush_on_exit_writes_the_current_chain_immediately() {
        let _flag = FLAG_LOCK.lock();
        let dir = std::env::temp_dir().join(format!("rh-autosave-flush-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("autosave.json");
        let _ = std::fs::remove_file(&file);
        let presets = Arc::new(crate::domain::preset::PresetManager::with_dir(dir.clone()));
        let plugins = Arc::new(crate::plugins::PluginInstanceManager::new());
        let info = crate::plugins::PluginInfo {
            id: "builtin::compressor".into(),
            name: "Compressor".into(),
            vendor: String::new(),
            version: String::new(),
            path: relight_addon_compressor::ID.into(),
            format: crate::plugins::PluginFormat::Builtin,
            category: String::new(),
            sub_index: 0,
        };
        plugins.load_plugin(info, 48_000.0, 512).unwrap();

        flush_on_exit(&plugins, &presets, &Arc::new(AtomicU64::new(0)));

        let saved = std::fs::read_to_string(&file).expect("no autosave written on exit");
        assert!(saved.contains("builtin::compressor"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
