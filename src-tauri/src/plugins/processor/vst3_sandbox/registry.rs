//! Persisted crash attribution for VST3 plugins, across app restarts.
//!
//! A native (SEH) crash inside an in-process plugin kills the whole app
//! before any Rust code can run to record it — `crash_protection.rs`'s
//! `catch_unwind`-based counter only ever sees Rust panics, never this class
//! of crash. So attribution has to happen from the *outside*: on every
//! startup, before doing anything else, we check whether the *previous* run
//! shut down uncleanly while a given plugin was active, and if so count that
//! as one crash for it, persisted here. Once a plugin accumulates
//! `SANDBOX_THRESHOLD` crashes it is loaded inside `vst3_sandbox_host.exe`
//! instead of in-process from then on (see `super::SandboxedVst3Processor`).
//!
//! ponytail: when more than one VST3 plugin is active at once, an unclean
//! exit is attributed to ALL of them (we can't tell which one actually
//! faulted from outside the crashed process). This over-sandboxes innocent
//! plugins that happen to share a chain with a fragile one, rather than
//! under-detecting — errs toward the safer direction. Upgrade path: narrow
//! the "active" marker to the moment of an actual `process()`/`attached()`
//! call (not the whole load-to-unload lifetime) if false positives matter.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use parking_lot::Mutex;

/// Crashes accumulated before a plugin is promoted to sandboxed hosting.
/// Matches `crash_protection::CrashProtection::should_auto_restart`'s
/// in-process threshold, so both paths give up at the same point.
pub const SANDBOX_THRESHOLD: u32 = 3;

#[derive(Default, Serialize, Deserialize)]
struct RegistryFile {
    /// Plugin path -> accumulated crash count.
    #[serde(default)]
    crash_counts: HashMap<String, u32>,
    /// Plugin paths currently loaded in-process. Written when a plugin
    /// finishes loading, removed on clean shutdown/unload. Any path still
    /// present when the app starts up again means last exit was unclean
    /// while that plugin was active.
    #[serde(default)]
    active_in_process: Vec<String>,
}

struct Registry {
    path: PathBuf,
    state: Mutex<RegistryFile>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ReLightHost")
        .join("sandboxed_plugins.json")
}

fn load_from_disk(path: &PathBuf) -> RegistryFile {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_to_disk(path: &PathBuf, file: &RegistryFile) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(file) {
        if let Err(e) = fs::write(path, bytes) {
            log::warn!("Failed to persist VST3 sandbox registry: {e}");
        }
    }
}

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let path = registry_path();
        let state = load_from_disk(&path);
        Registry { path, state: Mutex::new(state) }
    })
}

/// Call once at startup, before loading any plugins. Any plugin path still
/// marked `active_in_process` from the previous run gets one crash credited
/// (see module docs), then the marker list is cleared for this fresh run.
pub fn attribute_crashes_from_unclean_exit() {
    let reg = registry();
    let mut state = reg.state.lock();
    let stale: Vec<String> = std::mem::take(&mut state.active_in_process);
    if stale.is_empty() {
        return;
    }
    for path in &stale {
        let count = state.crash_counts.entry(path.clone()).or_insert(0);
        *count += 1;
        log::warn!(
            "VST3 plugin '{}' was active during an unclean exit — crash count now {} (sandbox threshold {})",
            path, *count, SANDBOX_THRESHOLD
        );
    }
    save_to_disk(&reg.path, &state);
}

/// Whether `plugin_path` has crashed enough times to load sandboxed.
pub fn should_sandbox(plugin_path: &str) -> bool {
    let reg = registry();
    let state = reg.state.lock();
    state.crash_counts.get(plugin_path).copied().unwrap_or(0) >= SANDBOX_THRESHOLD
}

/// Mark `plugin_path` as currently loaded in-process (in-process path only —
/// a sandboxed load can never itself take the whole app down, so it doesn't
/// need this marker).
pub fn mark_active_in_process(plugin_path: &str) {
    let reg = registry();
    let mut state = reg.state.lock();
    if !state.active_in_process.iter().any(|p| p == plugin_path) {
        state.active_in_process.push(plugin_path.to_string());
        save_to_disk(&reg.path, &state);
    }
}

/// Clear the "active in-process" marker on clean unload/shutdown.
pub fn mark_inactive(plugin_path: &str) {
    let reg = registry();
    let mut state = reg.state.lock();
    let before = state.active_in_process.len();
    state.active_in_process.retain(|p| p != plugin_path);
    if state.active_in_process.len() != before {
        save_to_disk(&reg.path, &state);
    }
}

/// Record a crash detected directly (a sandboxed child dying is observable
/// in-process, unlike a native in-process crash) — used by
/// `SandboxedVst3Processor` so its own crash counter also feeds this
/// registry, in case the plugin is later demoted back to in-process by a
/// user action.
pub fn record_crash(plugin_path: &str) {
    let reg = registry();
    let mut state = reg.state.lock();
    let count = state.crash_counts.entry(plugin_path.to_string()).or_insert(0);
    *count += 1;
    save_to_disk(&reg.path, &state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch registry file under the OS temp dir, unique per test run
    /// (avoids adding a tempdir crate dependency just for this) and removed
    /// after the test.
    struct ScratchFile(PathBuf);
    impl Drop for ScratchFile {
        fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
    }

    fn scratch_path() -> ScratchFile {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        ScratchFile(std::env::temp_dir().join(format!("relighthost_sandbox_registry_test_{n}.json")))
    }

    #[test]
    fn fresh_plugin_is_not_sandboxed() {
        let scratch = scratch_path();
        let state = load_from_disk(&scratch.0);
        assert_eq!(state.crash_counts.get("C:/x.vst3").copied().unwrap_or(0), 0);
    }

    #[test]
    fn crash_count_persists_across_reload() {
        let scratch = scratch_path();
        let mut state = load_from_disk(&scratch.0);
        *state.crash_counts.entry("C:/x.vst3".into()).or_insert(0) += 1;
        save_to_disk(&scratch.0, &state);

        let reloaded = load_from_disk(&scratch.0);
        assert_eq!(reloaded.crash_counts.get("C:/x.vst3").copied(), Some(1));
    }

    #[test]
    fn threshold_promotes_to_sandbox() {
        let scratch = scratch_path();
        let mut state = load_from_disk(&scratch.0);
        state.crash_counts.insert("C:/fragile.vst3".into(), SANDBOX_THRESHOLD);
        save_to_disk(&scratch.0, &state);

        let reloaded = load_from_disk(&scratch.0);
        assert!(reloaded.crash_counts.get("C:/fragile.vst3").copied().unwrap_or(0) >= SANDBOX_THRESHOLD);
    }

    #[test]
    fn active_marker_survives_and_clears() {
        let scratch = scratch_path();
        let mut state = load_from_disk(&scratch.0);
        state.active_in_process.push("C:/x.vst3".into());
        save_to_disk(&scratch.0, &state);
        assert_eq!(load_from_disk(&scratch.0).active_in_process, vec!["C:/x.vst3".to_string()]);

        let mut state = load_from_disk(&scratch.0);
        state.active_in_process.retain(|p| p != "C:/x.vst3");
        save_to_disk(&scratch.0, &state);
        assert!(load_from_disk(&scratch.0).active_in_process.is_empty());
    }
}
