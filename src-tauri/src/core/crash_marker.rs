//! Lightweight, sandbox-free "did we crash last time" marker for VST3
//! plugins.
//!
//! A native crash (access violation, heap corruption) inside a plugin's own
//! compiled code kills the whole process before any Rust code runs to
//! record it, and ReLightHost no longer isolates VST3 plugins in a child
//! process to contain that (see git history for why that was removed — the
//! sandbox's own GUI path had an unresolved hang bug that made it worse
//! than the crash risk it was meant to prevent). This module can't stop a
//! native crash from taking the whole app down, but it can warn the user
//! about it on the *next* launch: any plugin path still marked "active"
//! here when the app starts back up means the previous run ended uncleanly
//! while that plugin was loaded, which is a strong hint for which plugin to
//! remove if the app keeps disappearing.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct MarkerFile {
    #[serde(default)]
    active: HashSet<String>,
    /// Live instance count per path (in memory only — the file just needs
    /// the set). Two instances of one plugin: removing one must not clear
    /// the marker while the other is still loaded.
    #[serde(skip)]
    counts: HashMap<String, u32>,
}

impl MarkerFile {
    /// Returns whether the persisted set changed.
    fn activate(&mut self, path: &str) -> bool {
        *self.counts.entry(path.to_string()).or_insert(0) += 1;
        self.active.insert(path.to_string())
    }

    /// Returns whether the persisted set changed.
    fn deactivate(&mut self, path: &str) -> bool {
        match self.counts.get_mut(path) {
            Some(n) if *n > 1 => {
                *n -= 1;
                false
            }
            _ => {
                self.counts.remove(path);
                self.active.remove(path)
            }
        }
    }
}

struct Marker {
    path: PathBuf,
    state: Mutex<MarkerFile>,
}

static MARKER: OnceLock<Marker> = OnceLock::new();
/// The plugins flagged as "active during an unclean exit" for *this* run,
/// captured once at startup by `take_unclean_exit_plugins()` before the
/// marker file gets cleared for the current session. The `get_startup_warning`
/// command reads this cached copy instead of re-touching the file.
static STARTUP_WARNING: OnceLock<Vec<String>> = OnceLock::new();

fn marker_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ReLightHost")
        .join("active_vst3_plugins.json")
}

fn marker() -> &'static Marker {
    MARKER.get_or_init(|| {
        let path = marker_path();
        let state = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Marker { path, state: Mutex::new(state) }
    })
}

fn save(m: &Marker, state: &MarkerFile) {
    if let Some(parent) = m.path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(state) {
        if let Err(e) = fs::write(&m.path, bytes) {
            log::warn!("Failed to persist VST3 crash marker: {e}");
        }
    }
}

/// Call once at startup, before loading any plugins. Drains whatever paths
/// were left marked active by the previous run (an unclean exit — a clean
/// shutdown always clears them via `mark_inactive`) and caches them for
/// `startup_warning()` to hand to the frontend later, once the Tauri app
/// handle exists.
pub fn take_unclean_exit_plugins() {
    let m = marker();
    let mut state = m.state.lock();
    let stale: Vec<String> = state.active.drain().collect();
    save(m, &state);
    if !stale.is_empty() {
        log::warn!("VST3 plugin(s) still marked active after an unclean exit: {stale:?}");
    }
    let _ = STARTUP_WARNING.set(stale);
}

/// The plugin paths flagged by `take_unclean_exit_plugins()` at startup, for
/// the frontend to show as a one-time warning. Empty on a clean start.
pub fn startup_warning() -> Vec<String> {
    STARTUP_WARNING.get().cloned().unwrap_or_default()
}

pub fn mark_active(plugin_path: &str) {
    let m = marker();
    let mut state = m.state.lock();
    if state.activate(plugin_path) {
        save(m, &state);
    }
}

pub fn mark_inactive(plugin_path: &str) {
    let m = marker();
    let mut state = m.state.lock();
    if state.deactivate(plugin_path) {
        save(m, &state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixes review finding "Important #2": `instance.rs` used to call
    /// `mark_active` only from the `Ok(proc) =>` arm, i.e. only *after*
    /// `Vst3Processor::load()` had already returned successfully — which a
    /// native crash *inside* that call (createInstance/initialize(), exactly
    /// the calls `parallel_vst3` runs concurrently) never reaches. This test
    /// can't simulate a real native crash, but it pins the underlying
    /// detection logic `take_unclean_exit_plugins()` relies on: an entry
    /// left in `active` (mark_active called, mark_inactive never called —
    /// what a crash mid-load now looks like, since instance.rs marks active
    /// *before* calling load()) is exactly what the next run's drain must
    /// report as stale.
    #[test]
    fn active_entry_left_by_a_crash_is_detected_on_next_drain() {
        let mut state = MarkerFile::default();
        state.active.insert("C:/Some/Plugin.vst3".to_string());

        let stale: Vec<String> = state.active.drain().collect();

        assert_eq!(stale, vec!["C:/Some/Plugin.vst3".to_string()]);
        assert!(state.active.is_empty(), "draining should reset state for the new run");
    }

    #[test]
    fn a_path_stays_active_until_every_instance_of_it_is_released() {
        let mut state = MarkerFile::default();
        assert!(state.activate("C:/P.vst3"));
        assert!(!state.activate("C:/P.vst3"), "second instance: file unchanged");
        assert!(!state.deactivate("C:/P.vst3"), "one instance still loaded");
        assert!(state.active.contains("C:/P.vst3"));
        assert!(state.deactivate("C:/P.vst3"));
        assert!(state.active.is_empty());
    }

    #[test]
    fn clean_mark_inactive_leaves_nothing_to_detect() {
        let mut state = MarkerFile::default();
        state.active.insert("C:/Some/Plugin.vst3".to_string());
        state.active.remove("C:/Some/Plugin.vst3");

        let stale: Vec<String> = state.active.drain().collect();
        assert!(stale.is_empty(), "a cleanly-unmarked plugin must not be reported as a crash");
    }
}
