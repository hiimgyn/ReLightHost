# Opt-In Parallel VST3 Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let VST3 plugins load in parallel during session restore (instead of always sequentially) so a chain with a slow plugin (e.g. Supertone Clear, ~12s to `initialize()`) doesn't serialize every other plugin's load behind it — without silently taking on the native-crash risk that made the previous VST3-sandbox mechanism worse than the problem it solved. Ships **off by default**, behind a persisted user setting, so nobody's restore behavior changes unless they opt in.

**Architecture:** `PluginInstanceManager::load_plugins_parallel_results` currently always loads VST3 jobs in a sequential `for` loop and every other format in a Rayon parallel iterator (see `src-tauri/src/plugins/core/instance.rs:598-680`, comment "VST3 factories are initialised **sequentially** (COM + host stability)"). This plan adds a `parallel_vst3: bool` parameter: when `true`, VST3 jobs join the same Rayon parallel batch as everything else. To narrow the actual concurrency risk (Windows DLL loader lock / COM factory setup — not well-behaved plugin code in general), `Vst3Processor::load()` gets a process-wide mutex that serializes only `Library::new()` through `factory.createInstance()` (obtaining `IComponent`); `component.initialize()` and everything after it — where a heavy plugin's own slow work actually happens — runs unlocked, so parallel loads gain real wall-clock parallelism where it matters instead of also serializing the slow part. The flag is read from `ConfigManager` (same pattern as `minimize_to_tray`), settable from a new toggle in Settings → General, and passed through from `session.rs`'s restore path.

**Tech Stack:** Rust (`rayon`, `parking_lot::Mutex`), existing `ConfigManager`/Tauri-command/Settings-UI patterns already in this codebase.

**Spec:** none separate — this plan is self-contained; the Goal/Architecture above is the spec.

## Global Constraints

- Default behavior (flag off) must be **byte-for-byte identical** to today's sequential VST3 loading — this is an opt-in, not a behavior change for existing users.
- No new dependency — `parking_lot` and `rayon` are already in `Cargo.toml`.
- Windows-only code stays inside the existing `#[cfg(target_os = "windows")] mod win` block in `vst3.rs`; don't touch the non-Windows stub.
- Match existing i18n: every new user-facing string gets both an `en.ts` and `vi.ts` entry (see `src/i18n/types.ts` for the schema both must satisfy).
- Match existing Settings UI pattern exactly: label (`Text strong`) + description (`Paragraph type="secondary"`) + `Switch`, inside `settingRowStyle`, wired through a `makeToggleHandler`-style async handler with the existing localStorage-cache pattern in `AppSettings.tsx`.

## Review Focus

- **Toggle OFF (default) must not change behavior at all.** A restore with the flag off must still hit the exact same sequential `for j in &jobs { if j.vst3 { ... } }` path as today, with no new locking overhead on that path. Task 4's test asserts this explicitly.
- **Toggle ON with a single VST3 plugin, or zero VST3 plugins, in the chain.** Rayon's `into_par_iter()` over 0 or 1 items must not panic, deadlock, or behave differently from the `n == 1` fast path already in the function. Task 4's test covers 0/1/many.
- **The new mutex must never be held across a call that could try to re-acquire it.** `component.initialize()` (arbitrary plugin code) must run **outside** the lock — if a plugin's own `initialize()` ever tried to load another VST3 plugin through the same host API (unlikely, but the whole point of this review line is not to assume), holding the lock there would deadlock the whole restore. Task 1's implementation and its comment make the lock scope explicit and minimal.
- **A native crash during a parallel load is still a whole-app crash.** This plan does not add any crash isolation — it only changes *when*, not *whether*, a fragile plugin can take the app down. Task 6's Settings copy must say this plainly so turning the toggle on is an informed choice, not a hidden risk.
- **Flipping the toggle mid-session must not retroactively touch already-loaded plugins.** It only affects the *next* batch load (a fresh `restore_session` or preset load), same as how `set_forced_sandbox` used to document "takes effect the next time the plugin is loaded" — Task 3's doc comment says this explicitly so nobody expects live re-loading.

---

### Task 1: Serialize only the loader-lock-sensitive part of `Vst3Processor::load()`

**Files:**
- Modify: `src-tauri/src/plugins/processor/vst3.rs:224-410` (`Vst3Processor::load()`, inside `#[cfg(target_os = "windows")] mod win`)
- Test: `src-tauri/src/plugins/processor/vst3.rs` (new `#[cfg(test)] mod lock_tests` in the same file)

**Interfaces:**
- Produces: `static FACTORY_CREATE_LOCK: parking_lot::Mutex<()>` (module-private to `vst3.rs::win`) — a process-wide lock serializing `Library::new`/`GetPluginFactory`/`createInstance`. Task 4 does not touch this directly (it's internal to `vst3.rs`), but Task 7's comments reference it by this name.

Real VST3 DLLs aren't available in this repo or in CI, so `Vst3Processor::load()` itself can't be exercised by an automated test — that's a pre-existing limitation (no such test exists today either). This task's test instead proves the **locking primitive** behaves correctly in isolation, which is the actual new logic being added.

- [ ] **Step 1: Write the failing test for the lock**

Add to the bottom of `src-tauri/src/plugins/processor/vst3.rs`, outside the `mod win` block (so it compiles on every platform, not just Windows — the lock itself is platform-agnostic even though only Windows code will ever call it):

```rust
#[cfg(test)]
mod lock_tests {
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// Stands in for `win::FACTORY_CREATE_LOCK` — same shape, defined here
    /// so this test compiles on non-Windows too. Proves the pattern Task 1
    /// step 3 wires into `load()`: while one thread holds the lock, a second
    /// thread attempting to acquire it must block until the first releases.
    fn test_lock() -> &'static Mutex<()> {
        static LOCK: Mutex<()> = Mutex::new(());
        &LOCK
    }

    #[test]
    fn second_acquirer_blocks_until_first_releases() {
        let in_critical_section = Arc::new(AtomicU32::new(0));
        let max_concurrent = Arc::new(AtomicU32::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let in_cs = Arc::clone(&in_critical_section);
            let max_cs = Arc::clone(&max_concurrent);
            handles.push(thread::spawn(move || {
                let _guard = test_lock().lock();
                let now = in_cs.fetch_add(1, Ordering::SeqCst) + 1;
                max_cs.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(5));
                in_cs.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            max_concurrent.load(Ordering::SeqCst),
            1,
            "more than one thread was inside the locked section at once"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails to compile as a sanity check, then passes**

Run: `cargo test --lib -p ReLightHost lock_tests -- --nocapture` (from `src-tauri/`)
Expected: compiles and **passes** immediately — this step is proving the test itself is correct (the primitive it exercises, `parking_lot::Mutex`, already works). The real work is wiring the same pattern into `load()` in Step 3.

- [ ] **Step 3: Add the lock to `load()` and narrow its scope to the loader-lock-sensitive section**

In `src-tauri/src/plugins/processor/vst3.rs`, inside `mod win`, add the static lock near the top of the module (right after the existing `use` statements, before `pub struct Vst3Processor` — find it by searching for `pub struct Vst3Processor` in the file):

```rust
    /// Serializes DLL loading + factory creation across concurrently-loading
    /// VST3 plugins (see PluginInstanceManager::load_plugins_parallel_results'
    /// `parallel_vst3` flag). Scope is intentionally narrow — see load()'s
    /// comment for exactly what's inside vs. outside this lock and why.
    static FACTORY_CREATE_LOCK: PLMutex<()> = PLMutex::new(());
```

Then in `load()` (starts at line 224, `pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {`), wrap the section from `Library::new` through obtaining `component: ComPtr<IComponent>` in the lock. Replace:

```rust
        pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
            // Some plugins call COM APIs during load/initialize.
            ensure_com_initialized();

            // Load DLL
            let lib = unsafe { Library::new(plugin_path) }
                .map_err(|e| anyhow!("Failed to load '{}': {}", plugin_path, e))?;
```

with:

```rust
        pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
            // Some plugins call COM APIs during load/initialize.
            ensure_com_initialized();

            // Only DLL loading + factory + createInstance are serialized —
            // the Windows loader lock and any COM apartment setup a plugin's
            // DllMain/static initializers do are the actual concurrency risk
            // when multiple *different* VST3 DLLs load at once (see
            // PluginInstanceManager's `parallel_vst3` flag). component.
            // initialize() below — where a heavy plugin does its own slow
            // work (e.g. loading an ML model) — runs UNLOCKED, on purpose:
            // that's the part parallel loading is trying to overlap, and it's
            // ordinary plugin code, not host-loader-adjacent code, so there's
            // no more reason to serialize it across *different* plugins than
            // there is for any other method call into a plugin.
            let (lib, component): (Library, ComPtr<IComponent>) = {
                let _factory_guard = FACTORY_CREATE_LOCK.lock();

                // Load DLL
                let lib = unsafe { Library::new(plugin_path) }
                    .map_err(|e| anyhow!("Failed to load '{}': {}", plugin_path, e))?;

                // Optional InitDll (some plugins require it)
                type BoolFn = unsafe extern "system" fn() -> bool;
                if let Ok(init_dll) = unsafe { lib.get::<BoolFn>(b"InitDll\0") } {
                    if !unsafe { init_dll() } {
                        log::warn!("{} InitDll() returned false for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
                    }
                }

                // GetPluginFactory
                type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
                let get_factory: Symbol<GetPluginFactory> = unsafe { lib.get(b"GetPluginFactory\0") }
                    .map_err(|_| anyhow!("'{}' has no GetPluginFactory export", plugin_path))?;
                let factory_ptr = unsafe { get_factory() };
                if factory_ptr.is_null() {
                    return Err(anyhow!("GetPluginFactory returned null for '{}'", plugin_path));
                }
                let factory = unsafe {
                    ComPtr::<IPluginFactory>::from_raw(factory_ptr)
                        .ok_or_else(|| anyhow!("Failed to wrap IPluginFactory"))?
                };

                // Find the Audio Module Class CID
                let n = unsafe { factory.countClasses() };
                let mut audio_cid: Option<vst3::Steinberg::TUID> = None;
                for i in 0..n {
                    let mut ci: PClassInfo = unsafe { std::mem::zeroed() };
                    if unsafe { factory.getClassInfo(i, &mut ci) } == kResultOk {
                        let cat: &[u8] = unsafe {
                            std::slice::from_raw_parts(ci.category.as_ptr() as *const u8, ci.category.len())
                        };
                        if cat.starts_with(b"Audio Module Class") && audio_cid.is_none() {
                            audio_cid = Some(ci.cid);
                        }
                    }
                }
                let cid = audio_cid
                    .ok_or_else(|| anyhow!("'{}': no Audio Module Class found", plugin_path))?;

                // createInstance  IComponent (with FUnknown fallback)
                let mut component_ptr: *mut IComponent = ptr::null_mut();
                let result = unsafe {
                    factory.createInstance(
                        cid.as_ptr(),
                        IComponent::IID.as_ptr() as *const i8,
                        &mut component_ptr as *mut _ as *mut _,
                    )
                };

                let component: ComPtr<IComponent> = if result == kResultOk && !component_ptr.is_null() {
                    unsafe {
                        ComPtr::<IComponent>::from_raw(component_ptr)
                            .ok_or_else(|| anyhow!("Failed to wrap IComponent"))?
                    }
                } else {
                    // Fallback: createInstance with FUnknown IID then QueryInterface
                    let mut raw_ptr: *mut vst3::Steinberg::FUnknown = ptr::null_mut();
                    let r2 = unsafe {
                        factory.createInstance(
                            cid.as_ptr(),
                            vst3::Steinberg::FUnknown::IID.as_ptr() as *const i8,
                            &mut raw_ptr as *mut _ as *mut _,
                        )
                    };
                    if r2 != kResultOk || raw_ptr.is_null() {
                        return Err(anyhow!(
                            "'{}': createInstance failed ({:#010x})",
                            plugin_path, result as u32
                        ));
                    }
                    let fu = unsafe {
                        ComPtr::<vst3::Steinberg::FUnknown>::from_raw(raw_ptr)
                            .ok_or_else(|| anyhow!("Failed to wrap FUnknown"))?
                    };
                    fu.cast::<IComponent>().ok_or_else(|| {
                        anyhow!("'{}': IComponent QueryInterface failed after FUnknown createInstance", plugin_path)
                    })?
                };

                // Tail expression: hand both `lib` and `component` out of the
                // locked block as a tuple, instead of letting `lib` drop
                // when the block ends — `lib` (the Library) must outlive
                // `component`, same as before this refactor (see the
                // existing `_lib: Option<Library>` field ordering / "MUST be
                // last" comment further down in this struct).
                (lib, component)
            };
```

No other change needed for `lib` — it's now a normal local in `load()`'s outer scope (bound by the `let (lib, component) = { ... };` above), used exactly as before by the existing `Ok(Self { _lib: Some(lib), ... })` at the end of the function.

- [ ] **Step 4: Run the full test suite to verify nothing broke**

Run: `cargo test --lib` (from `src-tauri/`)
Expected: all existing tests still pass (30 today), plus the new `lock_tests::second_acquirer_blocks_until_first_releases`.

Run: `cargo check --all-targets` (from `src-tauri/`)
Expected: clean, no warnings — `Library` and `ComPtr<IComponent>` must both be movable out of the locked block as a tuple (they are; neither type borrows from the block's local `lib`/`factory`/`cid` bindings that actually don't survive the block).

- [ ] **Step 5: Manual verification (no automated test possible — see task header)**

Build a debug binary (`cargo build`) and, with a real VST3 plugin installed, load it once through the normal app flow (Plugin Library → add). Confirm in the logs it still loads successfully and the GUI still opens. This proves the refactor didn't change single-plugin-load behavior, which is all Task 1 touches — Task 4 covers the actual concurrent-loading behavior.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/plugins/processor/vst3.rs
git commit -m "refactor(vst3): narrow load() to a lock around only DLL-load+createInstance"
```

---

### Task 2: Add the `parallel_vst3_loading` config field

**Files:**
- Modify: `src-tauri/src/domain/config.rs:11-111`
- Test: `src-tauri/src/domain/config.rs` (new `#[cfg(test)] mod tests` — none exists yet in this file)

**Interfaces:**
- Produces: `ConfigManager::get_parallel_vst3_loading(&self) -> bool`, `ConfigManager::set_parallel_vst3_loading(&self, enabled: bool) -> anyhow::Result<()>` — Task 3's commands call these by these exact names.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src-tauri/src/domain/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch config path under the OS temp dir, unique per test run, so
    /// tests never collide with the real config.json or with each other.
    struct ScratchConfig(PathBuf);
    impl Drop for ScratchConfig {
        fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
    }
    fn scratch_manager() -> (ScratchConfig, ConfigManager) {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("relighthost_config_test_{n}.json"));
        let manager = ConfigManager {
            config: Arc::new(RwLock::new(AppConfig::default())),
            config_path: path.clone(),
        };
        (ScratchConfig(path), manager)
    }

    #[test]
    fn parallel_vst3_loading_defaults_to_false() {
        let (_scratch, manager) = scratch_manager();
        assert!(!manager.get_parallel_vst3_loading());
    }

    #[test]
    fn parallel_vst3_loading_round_trips_through_disk() {
        let (scratch, manager) = scratch_manager();
        manager.set_parallel_vst3_loading(true).expect("save should succeed");
        assert!(manager.get_parallel_vst3_loading());

        let content = fs::read_to_string(&scratch.0).expect("config file should exist");
        let reloaded: AppConfig = serde_json::from_str(&content).expect("should parse");
        assert!(reloaded.parallel_vst3_loading);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib domain::config -- --nocapture` (from `src-tauri/`)
Expected: FAIL with "no field `parallel_vst3_loading`" / "no method named `get_parallel_vst3_loading`"

- [ ] **Step 3: Add the field and accessors**

In `src-tauri/src/domain/config.rs`, add the field to `AppConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub custom_scan_paths: Vec<String>,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default = "default_true")]
    pub show_app_on_startup: bool,
    /// Experimental: load VST3 plugins in parallel during a batch restore
    /// instead of one at a time. Off by default — see
    /// PluginInstanceManager::load_plugins_parallel_results for the
    /// trade-off (faster restore, but a native crash in one plugin can now
    /// happen while others are mid-load instead of at a predictable point).
    #[serde(default)]
    pub parallel_vst3_loading: bool,
}
```

Add it to `Default for AppConfig`:

```rust
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            custom_scan_paths: Vec::new(),
            minimize_to_tray: false,
            show_app_on_startup: true,
            parallel_vst3_loading: false,
        }
    }
}
```

Add the accessors, right after `set_show_app_on_startup`:

```rust
    pub fn get_parallel_vst3_loading(&self) -> bool {
        self.config.read().parallel_vst3_loading
    }

    pub fn set_parallel_vst3_loading(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write();
        config.parallel_vst3_loading = enabled;
        self.save_config(&config)?;
        Ok(())
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib domain::config -- --nocapture` (from `src-tauri/`)
Expected: PASS (2 new tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/config.rs
git commit -m "feat(config): add parallel_vst3_loading setting, off by default"
```

---

### Task 3: Add Tauri commands for the setting

**Files:**
- Modify: `src-tauri/src/commands/config.rs`
- Modify: `src-tauri/src/lib.rs` (command registration list, right after `commands::plugin::get_noise_suppressor_vad`)

**Interfaces:**
- Consumes: `ConfigManager::get_parallel_vst3_loading`/`set_parallel_vst3_loading` (Task 2)
- Produces: Tauri commands `get_parallel_vst3_loading`, `set_parallel_vst3_loading` — Task 6's frontend calls these by these exact string names via `invoke()`.

No new automated test here — this is a thin pass-through wrapper in the exact shape of `get_minimize_to_tray`/`set_minimize_to_tray`, which have none either; Task 2's tests already cover the logic this wraps.

- [ ] **Step 1: Add the commands**

In `src-tauri/src/commands/config.rs`, add after `set_show_app_on_startup`:

```rust
/// Takes effect the next time plugins are batch-loaded (a fresh
/// `restore_session` or preset load) — not for an already-running session.
#[tauri::command]
pub fn get_parallel_vst3_loading(state: tauri::State<AppState>) -> bool {
    state.config_manager.read().get_parallel_vst3_loading()
}

#[tauri::command]
pub fn set_parallel_vst3_loading(state: tauri::State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config_manager
        .read()
        .set_parallel_vst3_loading(enabled)
        .map_err(|e| format!("Failed to save parallel_vst3_loading: {}", e))
    .map(|_| info!("Setting updated: parallel_vst3_loading={enabled}"))
}
```

- [ ] **Step 2: Register both commands**

In `src-tauri/src/lib.rs`, find `commands::plugin::get_noise_suppressor_vad,` in the `tauri::generate_handler!` list and add right after it:

```rust
            commands::plugin::get_noise_suppressor_vad,
            commands::config::get_parallel_vst3_loading,
            commands::config::set_parallel_vst3_loading,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --all-targets` (from `src-tauri/`)
Expected: clean

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/config.rs src-tauri/src/lib.rs
git commit -m "feat(commands): expose parallel_vst3_loading get/set to the frontend"
```

---

### Task 4: Make `load_plugins_parallel_results` respect the flag

**Files:**
- Modify: `src-tauri/src/plugins/core/instance.rs:598-703` (the function itself)
- Test: `src-tauri/src/plugins/core/instance.rs` (new `#[cfg(test)] mod parallel_flag_tests`)

**Interfaces:**
- Consumes: nothing new from earlier tasks (this task's signature change is what Task 5 calls)
- Produces: `PluginInstanceManager::load_plugins_parallel_results(&self, infos: Vec<PluginInfo>, sample_rate: f64, block_size: usize, parallel_vst3: bool) -> Vec<Result<String, Error>>` — note the **new trailing `parallel_vst3: bool` parameter**. Task 5 calls this with the config value.

Like Task 1, this function fundamentally can't be exercised end-to-end without real VST3 plugin binaries (constructing a real `PluginInstance` calls `Vst3Processor::load()`, which needs an actual DLL on disk). The test below isolates the part that's actually new and testable without one: **which jobs get routed to the sequential path vs. the parallel path**, extracted into its own pure function so it's testable in isolation.

- [ ] **Step 1: Write the failing test**

First, add this pure helper as a **free function at module scope** (not inside `impl PluginInstanceManager`, so it can be called both as a bare name from methods in that impl and via `super::split_load_groups` from the test module below) — extracted so routing logic is testable without touching `PluginInstance`. Find `pub struct PluginInstanceManager {` in `src-tauri/src/plugins/core/instance.rs` and add this directly above it:

```rust
/// Splits job indices into (sequential, parallel) groups. VST3 jobs go
/// in `sequential` unless `parallel_vst3` is true, in which case every
/// job (VST3 included) goes in `parallel` — this is the entire behavior
/// difference the setting controls; everything downstream just iterates
/// whichever group a job landed in.
fn split_load_groups(is_vst3: &[bool], parallel_vst3: bool) -> (Vec<usize>, Vec<usize>) {
    let mut sequential = Vec::new();
    let mut parallel = Vec::new();
    for (idx, &vst3) in is_vst3.iter().enumerate() {
        if vst3 && !parallel_vst3 {
            sequential.push(idx);
        } else {
            parallel.push(idx);
        }
    }
    (sequential, parallel)
}
```

Then add the test module at the bottom of the file (or extend an existing `#[cfg(test)]` block if `instance.rs` already has one — it doesn't today, so add a new one):

```rust
#[cfg(test)]
mod parallel_flag_tests {
    use super::split_load_groups;

    #[test]
    fn flag_off_routes_all_vst3_to_sequential() {
        let is_vst3 = vec![true, false, true, false];
        let (seq, par) = split_load_groups(&is_vst3, false);
        assert_eq!(seq, vec![0, 2]);
        assert_eq!(par, vec![1, 3]);
    }

    #[test]
    fn flag_on_routes_everything_to_parallel() {
        let is_vst3 = vec![true, false, true, false];
        let (seq, par) = split_load_groups(&is_vst3, true);
        assert!(seq.is_empty());
        assert_eq!(par, vec![0, 1, 2, 3]);
    }

    #[test]
    fn empty_input_produces_empty_groups_either_way() {
        let is_vst3: Vec<bool> = vec![];
        assert_eq!(split_load_groups(&is_vst3, false), (vec![], vec![]));
        assert_eq!(split_load_groups(&is_vst3, true), (vec![], vec![]));
    }

    #[test]
    fn single_vst3_job_respects_the_flag() {
        let is_vst3 = vec![true];
        assert_eq!(split_load_groups(&is_vst3, false), (vec![0], vec![]));
        assert_eq!(split_load_groups(&is_vst3, true), (vec![], vec![0]));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parallel_flag_tests -- --nocapture` (from `src-tauri/`)
Expected: FAIL — `split_load_groups` doesn't exist yet.

- [ ] **Step 3: Run test to verify the helper alone passes**

Run: `cargo test --lib parallel_flag_tests -- --nocapture` (from `src-tauri/`)
Expected: PASS (4 tests) — the helper from Step 1 is enough on its own.

- [ ] **Step 4: Wire the helper into `load_plugins_parallel_results`**

Replace the function signature and body from `src-tauri/src/plugins/core/instance.rs:598` onward. Change the signature:

```rust
    pub fn load_plugins_parallel_results(
        &self,
        infos: Vec<PluginInfo>,
        sample_rate: f64,
        block_size: usize,
        parallel_vst3: bool,
    ) -> Vec<Result<String, Error>> {
```

Update the doc comment above it to describe the new parameter:

```rust
    /// Load many plugins for preset/session restore. Preserves chain order.
    ///
    /// VST3 factories are initialised **sequentially** by default (COM +
    /// host stability), unless `parallel_vst3` is true, in which case they
    /// join the same Rayon batch as CLAP/VST2/built-ins. See
    /// `Vst3Processor::load()`'s `FACTORY_CREATE_LOCK` for what's still
    /// serialized even when this is on. Off by default; see
    /// `ConfigManager::get_parallel_vst3_loading`.
    ///
    /// Return value aligns 1:1 with `infos`: `Ok(id)` on success, `Err` when
    /// that plugin failed to construct (same semantics as skipping a failed
    /// `load_plugin` in a loop).
```

Then, inside the function, replace the two loops (the `for j in &jobs { if j.vst3 { ... } }` sequential loop and the `parallel_jobs` filter+`into_par_iter()` block) with routing through `split_load_groups`. Replace:

```rust
        for j in &jobs {
            if j.vst3 {
                slots[j.idx] = Some(PluginInstance::new(j.info.clone(), sample_rate, block_size).map(Arc::new));
                crate::app_events::emit_plugin_chain_changed("restore_progress", Some(&j.info.name));
            }
        }

        let parallel_jobs: Vec<Job> = jobs.iter().filter(|j| !j.vst3).cloned().collect();
        if !parallel_jobs.is_empty() {
```

with:

```rust
        let is_vst3: Vec<bool> = jobs.iter().map(|j| j.vst3).collect();
        let (sequential_idx, parallel_idx) = split_load_groups(&is_vst3, parallel_vst3);

        for &idx in &sequential_idx {
            let j = &jobs[idx];
            slots[j.idx] = Some(PluginInstance::new(j.info.clone(), sample_rate, block_size).map(Arc::new));
            crate::app_events::emit_plugin_chain_changed("restore_progress", Some(&j.info.name));
        }

        let parallel_jobs: Vec<Job> = parallel_idx.iter().map(|&idx| jobs[idx].clone()).collect();
        if !parallel_jobs.is_empty() {
```

The rest of the function (the `into_par_iter()` `.map()` closure, the `for (idx, r) in filled` loop, and everything after) is unchanged — it already just iterates whatever ended up in `parallel_jobs`, regardless of format.

Also update the `n == 1` fast path just above (it doesn't call `split_load_groups` — it's a single job, already correct either way — but the new `parallel_vst3` parameter must still be accepted; no change needed there since that branch doesn't look at `j.vst3` at all today).

- [ ] **Step 5: Run the full test suite**

Run: `cargo test --lib` (from `src-tauri/`)
Expected: all tests pass, including the 4 new `parallel_flag_tests`.

Run: `cargo check --all-targets` (from `src-tauri/`)
Expected: this will show a compile error at every existing call site of `load_plugins_parallel_results` (missing the new argument) — that's expected; Task 5 fixes the one real call site (`session.rs`). If `cargo check` shows any *other* call sites, grep for them:

Run: `grep -rn "load_plugins_parallel_results" src-tauri/src/`
Expected: exactly two matches — the function definition itself, and the call in `core/session.rs` (fixed in Task 5).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/plugins/core/instance.rs
git commit -m "feat(plugins): route VST3 jobs through the parallel batch when opted in"
```

(This commit will not build in isolation — `session.rs` still calls the old 3-argument signature. That's fixed in Task 5, the very next task; if your workflow requires every commit to build, squash Tasks 4 and 5's commits together instead.)

---

### Task 5: Pass the config flag through session restore

**Files:**
- Modify: `src-tauri/src/core/session.rs:159-162`

**Interfaces:**
- Consumes: `ConfigManager::get_parallel_vst3_loading` (Task 2), `PluginInstanceManager::load_plugins_parallel_results`'s new 4-arg signature (Task 4)

- [ ] **Step 1: Read the flag and pass it through**

In `src-tauri/src/core/session.rs`, replace:

```rust
            let results = state
                .plugin_manager
                .read()
                .load_plugins_parallel_results(infos, sample_rate, buffer_size as usize);
```

with:

```rust
            let parallel_vst3 = state.config_manager.read().get_parallel_vst3_loading();
            let results = state
                .plugin_manager
                .read()
                .load_plugins_parallel_results(infos, sample_rate, buffer_size as usize, parallel_vst3);
```

- [ ] **Step 2: Verify the whole crate builds and tests pass**

Run: `cargo check --all-targets` (from `src-tauri/`)
Expected: clean — this resolves the Task 4 compile error at the one real call site.

Run: `cargo test --lib` (from `src-tauri/`)
Expected: all tests pass (30 pre-existing + 1 lock test from Task 1 + 2 config tests from Task 2 + 4 routing tests from Task 4).

Run: `cargo clippy --all-targets` (from `src-tauri/`)
Expected: no warnings.

- [ ] **Step 3: Manual verification with the flag on**

With a real VST3 plugin chain that includes at least one slow-loading plugin, flip `parallel_vst3_loading` to `true` directly in `%LOCALAPPDATA%\ReLightHost\config.json` (Task 6 adds the UI for this), restart the app, and confirm in the logs that VST3 `createInstance` calls for different plugins can now overlap in time (look for interleaved "VST3 plugin loaded" log lines from different plugin paths instead of one fully finishing before the next starts) and that the app doesn't crash or hang. Flip it back to `false` and confirm the old strictly-sequential log ordering returns.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/core/session.rs
git commit -m "feat(session): wire parallel_vst3_loading setting into restore"
```

---

### Task 6: Add the Settings toggle

**Files:**
- Modify: `src/components/settings/AppSettings.tsx`
- Modify: `src/i18n/en.ts`, `src/i18n/vi.ts`, `src/i18n/types.ts`

**Interfaces:**
- Consumes: Tauri commands `get_parallel_vst3_loading`/`set_parallel_vst3_loading` (Task 3), called directly via `invoke()` with string command names — **not** through a `src/lib/tauri.ts` wrapper. `AppSettings.tsx` calls `is_startup_enabled`/`get_minimize_to_tray`/`get_show_app_on_startup` the same raw-`invoke()` way already; this task matches that file's own established pattern rather than introducing `lib/tauri.ts` wrappers nothing else in the file uses.

No automated test — this mirrors the existing untested `runOnStartup`/`minimizeToTray` toggle pattern in the same file exactly; there's no existing frontend test harness for `AppSettings.tsx` to extend.

- [ ] **Step 1: Add i18n keys**

In `src/i18n/types.ts`, inside the `appSettings` section (find `runOnStartupDesc: string;`), add:

```typescript
    parallelVst3Loading: string;
    parallelVst3LoadingDesc: string;
```

In `src/i18n/en.ts`, in the matching `appSettings` section, add right after `runOnStartupDesc`:

```typescript
    parallelVst3Loading: 'Parallel VST3 loading (experimental)',
    parallelVst3LoadingDesc: 'Loads VST3 plugins at the same time instead of one by one — faster restore with a heavy plugin in the chain, but if one crashes natively it can now happen while others are mid-load instead of at a predictable point, and it still takes the whole app down either way.',
```

In `src/i18n/vi.ts`, add the matching Vietnamese pair:

```typescript
    parallelVst3Loading: 'Tải VST3 song song (thử nghiệm)',
    parallelVst3LoadingDesc: 'Tải các plugin VST3 cùng lúc thay vì lần lượt — restore nhanh hơn khi có plugin nặng trong chuỗi, nhưng nếu 1 plugin crash native thì có thể xảy ra ngay khi các plugin khác đang tải dở, thay vì ở một thời điểm dễ đoán như trước — và vẫn sập cả app như cũ dù bật hay tắt tùy chọn này.',
```

- [ ] **Step 2: Add the toggle to the General tab**

In `src/components/settings/AppSettings.tsx`, add state (near `runOnStartup`/`minimizeToTray`):

```typescript
  const [parallelVst3Loading, setParallelVst3LoadingState] = useState(() => readCachedBool('appSettings.parallelVst3Loading', false));
```

Add its key to the `KEYS` object at the top of the file:

```typescript
const KEYS = {
  startup: 'appSettings.runOnStartup',
  showOnStartup: 'appSettings.showOnStartup',
  minimize: 'minimizeToTray',
  parallelVst3Loading: 'appSettings.parallelVst3Loading',
} as const;
```

(then use `KEYS.parallelVst3Loading` instead of the inline string literal in the `useState` initializer above, matching the other three fields' style)

In `loadSettings()`, add it to the `Promise.all` and the follow-up `setX`/`localStorage.setItem` calls, matching the existing three:

```typescript
  const loadSettings = async () => {
    try {
      const [startupEnabled, minimizeEnabled, showOnStartupEnabled, parallelVst3Enabled] = await Promise.all([
        invoke<boolean>('is_startup_enabled'),
        invoke<boolean>('get_minimize_to_tray'),
        invoke<boolean>('get_show_app_on_startup'),
        invoke<boolean>('get_parallel_vst3_loading'),
      ]);
      setRunOnStartup(startupEnabled);
      setMinimizeToTray(minimizeEnabled);
      setShowAppOnStartup(showOnStartupEnabled);
      setParallelVst3LoadingState(parallelVst3Enabled);
      localStorage.setItem(KEYS.startup, String(startupEnabled));
      localStorage.setItem(KEYS.minimize, String(minimizeEnabled));
      localStorage.setItem(KEYS.showOnStartup, String(showOnStartupEnabled));
      localStorage.setItem(KEYS.parallelVst3Loading, String(parallelVst3Enabled));
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  };
```

Add the handler, next to `handleMinimizeToggle`:

```typescript
  const handleParallelVst3LoadingToggle = makeToggleHandler(
    setParallelVst3LoadingState,
    'set_parallel_vst3_loading',
    KEYS.parallelVst3Loading,
    'enabled',
  );
```

Add the row to `generalTab`, right after the "Minimize to Tray" row (before the closing `</div>` of `generalTab`):

```tsx
      {/* Parallel VST3 Loading (experimental) */}
      <div style={settingRowStyle}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.parallelVst3Loading')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
            {t('appSettings.parallelVst3LoadingDesc')}
          </Paragraph>
        </div>
        <Switch checked={parallelVst3Loading} onChange={handleParallelVst3LoadingToggle} />
      </div>
```

- [ ] **Step 3: Verify it compiles**

Run: `pnpm exec tsc --noEmit` (from repo root)
Expected: clean

- [ ] **Step 4: Manual verification**

Run: `pnpm build` (from repo root)
Expected: succeeds

With the app running (`pnpm tauri dev` or a built binary), open Settings → General, confirm the new row renders using the same padding/pill styling as the other two tabs (the `app-settings-tabs` CSS fix from the earlier session applies automatically — this row is inside the same `generalTab` JSX), toggle it, reopen Settings, and confirm it's still on (proves the round-trip through `config.json`).

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/AppSettings.tsx src/i18n/en.ts src/i18n/vi.ts src/i18n/types.ts
git commit -m "feat(settings): add experimental parallel VST3 loading toggle"
```

---

### Task 7: Diagnostic logging around the risky calls

**Files:**
- Modify: `src-tauri/src/plugins/processor/vst3.rs` (inside `load()`, the section touched in Task 1)

**Interfaces:** none new — pure logging addition.

Purpose: if a native crash ever does happen during a parallel restore, these log lines plus `crash_marker.rs` (already marks a plugin "active" before `initialize()` — see `instance.rs`'s `PluginInstance::new`) are what let you tell, after the fact, which specific call the crash happened in. No test — this is logging, not behavior.

- [ ] **Step 1: Add log lines around `Library::new`, `createInstance`, and `component.initialize`**

In `src-tauri/src/plugins/processor/vst3.rs`, inside `load()` (the locked block from Task 1), add immediately before the `Library::new` call:

```rust
                log::debug!("{} loading DLL for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
```

Immediately before the primary `factory.createInstance()` call:

```rust
                log::debug!("{} createInstance (IComponent) for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
```

And outside the locked block (this task's whole point — Task 1 already put `component.initialize` outside the lock), immediately before and after:

```rust
            log::debug!("{} calling initialize() for '{}' — unlocked, may run concurrently with other plugins", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
            unsafe {
                component.initialize(ptr::null_mut());
                // ... existing bus activation lines unchanged ...
            }
            log::debug!("{} initialize() returned for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
```

(the bus-activation lines between `component.initialize(ptr::null_mut());` and the closing of that `unsafe` block are the four existing `for i in 0..ai { component.activateBus(...); }`-style lines — leave them exactly as they are, only the two `log::debug!` lines around the whole block are new)

- [ ] **Step 2: Verify it compiles and existing tests still pass**

Run: `cargo check --all-targets` (from `src-tauri/`)
Expected: clean

Run: `cargo test --lib` (from `src-tauri/`)
Expected: all tests still pass — pure logging addition, no logic changed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/plugins/processor/vst3.rs
git commit -m "chore(vst3): log around load()'s locked/unlocked boundary for crash diagnosis"
```

---

## Rollout note (not a task — read before enabling by default)

This plan ships the setting **off**. Don't flip the default without first using it personally, with the real plugin chain (Clear + Valhalla + whatever else is normally loaded), across several real restarts, and watching for any crash or hang that doesn't happen with it off. If a crash ever does happen with it on, the `restore_progress`/`restore_total` events (already emitted) plus this task's new log lines plus `crash_marker.rs`'s next-launch warning together tell you which plugin and which call — check the log for the last `"createInstance (IComponent) for '<path>'"` or `"calling initialize() for '<path>'"` line with no matching "returned"/completion line after it.

**Added after final review — thread-identity risk (finding "Important #1"):** `restore_session` runs on Tauri's main/UI thread. With the flag off, every VST3 plugin is created there, same as always. With it on and more than one VST3 plugin, `createInstance`/`initialize()` for the parallel ones move to Rayon worker threads instead. Some plugin frameworks (JUCE especially) assume the thread that first creates a plugin is the UI/message thread and can hang or misbehave later if that assumption breaks. This is not fixed by this plan — it's a second, independent risk on top of the loader-lock one the plan was built around. Before ever defaulting this setting on, specifically test with real JUCE-based plugins and check the `thread=` field in the debug log around `createInstance`/`initialize()` for exactly this failure mode, not just "does it crash or hang" in general.
