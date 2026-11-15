# Add-ons Split & Upgrade Implementation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the four built-in processors into self-contained add-on crates/folders behind a small API, then fix each add-on from its skeptical review — with every commit green.

**Architecture:** `src-tauri` becomes the Cargo workspace root; members `../addons/{api,noise-suppressor,deep-filter,compressor,voice}`. Host keeps one registry (`plugins/addons.rs`). GUIs move to `addons/<name>/ui`, shared widgets to `addons/ui-common`, wired by `addons/registry.ts`. Order: freeze behaviour with golden tests → adopt the API in place → pure move → lock-free boundary → frontend → per-add-on fixes.

**Tech Stack:** Rust 1.80 (Tauri 2, ringbuf 0.5, windows 0.62, nnnoiseless 0.5.2, vendored libDF + tract =0.21.4), React + TS + Vite (rolldown), Tailwind v4, Node ≥ 23.6 for `.ts` check scripts, pnpm.

**Spec:** `docs/superpowers/specs/2026-11-15-addons-split-design.md`. Reviews: `docs/superpowers/specs/addons-review/*.md`. v2 folds in five plan reviews (build, boundary, frontend, DSP, red team).

## Global Constraints

- Stored ids stay `builtin::noise_suppressor|deep_filter|compressor|voice`; `PluginInfo.path` = id; stored format `builtin`.
- No preset/session format change. Add-on version lives in the existing `plugin_version` string as a decimal integer; non-numeric (e.g. `"2.8.0"`) or missing = version 1.
- Real-time path: no allocation, no *blocking* lock (existing `try_lock`s stay), no new logging.
- Always run cargo from `src-tauri` (its `.cargo/config.toml` applies only there). Target dir stays `src-tauri/target`.
- Workspace members listed explicitly (no glob). Vendored libDF is never a workspace member: Task 1 `exclude = ["vendor"]`; Task 3 replaces that with an empty `[workspace]` table appended to `addons/deep-filter/vendor/DeepFilterNet/libDF/Cargo.toml` (local vendor patch, noted in a comment).
- mmcss stays a module named `mmcss` (test paths `mmcss::tests::*`), so `--skip mmcss` keeps matching.
- UI labels: `common.system` → "Add-ons", `library.tabBuiltin` → "Add-ons ({count})" (en and vi); search alias `'system'` → `'add-ons'`.
- Each add-on owns its strings in `addons/<name>/ui/i18n.ts` (`const en = {...}; export const vi: typeof en = {...}`); section names stay `compressor`, `voice`, `noise`, `deepFilter`.
- Add-on GUIs import host code only via `addons/ui-common/host.ts`; imports are relative (no alias).
- Sound-changing items not listed under Gate G approval are not implemented; Task 10 lists them.
- No release tag between Task 5c and Task 7.

**Per-task gate** (in `src-tauri`, then repo root):
`cargo clippy --workspace --all-targets -- -D warnings` → no `warning:`; `cargo test --workspace --lib -- --skip enum_tests --skip mmcss` → every binary `test result: ok`, filtered-out total 4; `pnpm exec tsc --noEmit` → exit 0; `node scripts/check-drop-target.ts` → `drop-target checks passed`. Baseline today: 120 passed, 4 filtered.
Tasks 6–9 may use `cargo clippy -p relight-addon-<x> --all-targets -- -D warnings` + `cargo test -p relight-addon-<x>` while iterating; the coordinator runs the full gate before each commit.

## Review Focus

- Saved presets/sessions with `builtin::*` ids load with identical param values (Task 0 characterization test, kept green to the end).
- Audio output of every add-on at default and one non-default param set is unchanged by Tasks 1–5c (Task 0 golden renders, `#[ignore]`, local gate).
- A typo in a moved param table would silently clamp saved values (Task 0 `params.json` golden, asserted in Task 2).
- A knob move never makes a block skip the add-on; un-bypass never replays stale audio (Task 4a tests).
- Unknown add-on id keeps its saved params through save/load (Task 4b test).

---

### Task 0: Freeze current behaviour

**Files:** Create `src-tauri/src/plugins/builtin/golden_tests.rs` (`#[cfg(test)]`, declared in `builtin/mod.rs`), `addons/golden/*.f32`, `addons/golden/params.json`; Modify `core/session.rs` tests.

- [ ] **Step 1:** `golden_input() -> (Vec<f32>, Vec<f32>)`: 0.5 s @ 48 kHz; L = 0.3·sin(2π·220t)·(1+0.5·sin(2π·3t)) + 0.05·lcg(seed 1); R = same with 330 Hz, seed 2; samples 6000..8400 a −60→−6 dB ramp; 50 ms silence at 16000..18400. LCG: `s = s*1664525+1013904223`, value `(s>>8) as f32 / 2^24 * 2 − 1`.
- [ ] **Step 2:** Tests `#[ignore] golden_<id>_default` and `golden_<id>_custom` for the four ids (custom: noise mix 0.6/threshold 0.4/atten 0.5; compressor −30/6/5/200/6/6/0.8; voice 6/−3/4/0.5/0.4/−3; deep-filter atten 18/post 0.1/mix 0.7/gain 2). Render in 480-sample blocks via `create_builtin`. Deep-filter test lives inside `deep_filter.rs` tests (needs private fields): after each `process`, spin until `cons_from_worker.occupied_len() >= 480` (5 s timeout). With `GOLDEN_WRITE=1` write interleaved LE f32 to `addons/golden/<case>.f32`, else compare max |diff| ≤ 1e-6.
- [ ] **Step 3:** Test `builtin_param_tables_match_golden`: serialize `builtin_initial_params(id)` for all four ids to `addons/golden/params.json` (write mode) / compare field-by-field.
- [ ] **Step 4:** Characterization test `old_builtin_preset_round_trips_identically` in `core/session.rs`: preset JSON with real keys (`name`, `description`, `created_at`, `plugin_chain: [{plugin_id, plugin_name, plugin_path: "builtin::compressor", plugin_format: "builtin", plugin_version: "2.8.0", bypassed: false, parameters: [{id:0,name:"Threshold",value:-24.0},{id:1,name:"Ratio",value:6.0}]}]`) loads; instance values equal; re-snapshot yields same ids/values.
- [ ] **Step 5:** Capture: `GOLDEN_WRITE=1 cargo test --lib -- --ignored golden_ builtin_param_tables`; then run twice without `GOLDEN_WRITE` → pass both times. Gate. Commit `test(addons): golden renders before the split`.

### Task 1: Workspace + API crate

**Files:** Create `addons/api/{Cargo.toml,src/lib.rs,src/rt.rs,src/mmcss.rs}`; Modify `src-tauri/Cargo.toml` (`[workspace] members = ["../addons/api"]`, `exclude = ["vendor"]`, dep `relight-addon-api = { path = "../addons/api" }`), `src-tauri/src/audio/mixer.rs` (remove `BacklogTrimmer` + lines 220–226 helpers, `pub use relight_addon_api::rt::BacklogTrimmer;`), `src-tauri/src/audio/mmcss.rs` (becomes `pub use relight_addon_api::mmcss::*;`), `.github/workflows/check.yml:48,54` (`--workspace`; skip `mmcss`).

API `Cargo.toml`: `package.workspace = "../../src-tauri"`, deps `ringbuf = "0.5"`, `log = "0.4"`, `[target.'cfg(windows)'.dependencies] windows = { version = "0.62", features = ["Win32_System_Threading","Win32_Foundation"] }` (add any feature the moved code needs).

**Interfaces — Produces:**
```rust
pub enum Scale { Linear, Db, Log }
pub struct ParamSpec { pub id: u32, pub name: &'static str, pub min: f32, pub max: f32, pub default: f32,
    pub unit: &'static str, pub scale: Scale, pub step: f32, pub choices: &'static [&'static str] }
pub struct AddonDescriptor {
    pub id: &'static str, pub name: &'static str, pub category: &'static str, pub version: u32,
    pub params: &'static [ParamSpec], pub meters: &'static [&'static str],
    pub sample_rates: &'static [u32],   // empty = any
    pub create: fn(sample_rate: f32) -> Option<Box<dyn AddonProcessor>>,
    pub migrate: Option<fn(from: u32, params: &mut Vec<(u32, f32)>)>,
}
pub trait AddonProcessor: Send {
    /// Must write every meter slot each call.
    fn process(&mut self, l: &mut [f32], r: &mut [f32], meters: &mut [f32]);
    fn set_parameter(&mut self, id: u32, value: f32);
    /// Called on a control thread while bypassed, never concurrently with process; may allocate.
    fn reset(&mut self) {}
    fn latency_samples(&self) -> u32 { 0 }
}
pub const fn check_params(p: &[ParamSpec], meters: &[&str]) -> bool; // len ≤ 64, meters ≤ 8, p[i].id == i
pub mod rt { pub struct BacklogTrimmer; }       // moved verbatim
#[cfg(windows)] pub mod mmcss { pub fn boost_current_thread_to_pro_audio(); } // moved; thread_prefix → literal "[audio/mmcss]"
```

- [ ] **Step 1:** Move code; rewrite BacklogTrimmer tests to pop with a local `for _ in 0..n { cons.try_pop(); }` and `[f32; 2]` frames. Test `check_params_rejects_65_params_and_non_sequential_ids`.
- [ ] **Step 2:** Gate (passed = baseline + Task 0 + 1). Commit `refactor(addons): workspace and add-on API crate`.

### Task 2: Adopt the API in place (no file moves)

**Files:** Modify `src-tauri/src/plugins/builtin/{mod.rs,noise_suppressor.rs,deep_filter.rs,compressor.rs,voice.rs}`; Create `src-tauri/src/plugins/addons.rs`; Modify `plugins/mod.rs`, `plugins/core/instance.rs` (11, 40, 142–167, 300, 470, tests 1165–1208), `plugins/core/scanner.rs` (217, 531–578), `core/session.rs` (313–322, 381–385), `core/autosave.rs` (266–270), `commands/plugin.rs:289`.

**Interfaces:**
- Each builtin file: `impl AddonProcessor` (process writes VAD to `meters[0]` where it existed) and `pub static ADDON: AddonDescriptor`. Params copy `builtin_initial_params` rows exactly; dB params `Scale::Db,"dB"`, Attack/Release `Scale::Log,"ms"`, Ratio `Scale::Log,":1"`, else `Linear,""`; `version: 1`; `migrate: None`; `sample_rates: &[48000]` for noise/deep-filter, `&[]` otherwise; meters noise/deep-filter `["vad"]`, compressor/voice `[]`.
- `plugins/addons.rs`: `pub static ADDONS: &[&AddonDescriptor]`; `pub fn find(id: &str) -> Option<&'static AddonDescriptor>`; `pub fn initial_params(d) -> Vec<PluginParameter>`; `pub fn plugin_info(d) -> PluginInfo` (vendor `"ReLightHost"`, version `d.version.to_string()`).
- `PluginInstance` gets `meters_scratch: [f32; 8]`-backed buffer (no per-block alloc); `last_vad` ← `meters[0]` so `get_noise_suppressor_vad` keeps working.
- `preset_load_plan`: for `Builtin` with `addons::find(path)` hit, overwrite name/vendor/version/category from the descriptor.
- `create_builtin` / `builtin_initial_params` deleted; callers use the registry.

- [ ] **Step 1: Failing test** `every_addon_loads_and_processes`: for each `ADDONS` entry, `PluginInstance` at 48 kHz / 480, 200 blocks of LCG noise → all finite; `latency_samples()` = 960, 960, 0, 0 (noise, deep-filter, compressor, voice; deep-filter after its warm-up as today); meters len = descriptor.meters.len(). Plus assert `builtin_param_tables_match_golden` now reads `initial_params(d)`.
- [ ] **Step 2:** Implement. Gate + `cargo test --lib -- --ignored golden_` pass. Commit `refactor(addons): adopt add-on API and host registry`.

### Task 3: Pure move

**Files:** `git mv src-tauri/src/plugins/builtin/<x>.rs addons/<name>/src/lib.rs` (×4), `git mv src-tauri/vendor/DeepFilterNet addons/deep-filter/vendor/DeepFilterNet`, golden test module → host `plugins/addons.rs` tests (deep-filter golden → its crate tests); Create `addons/<name>/Cargo.toml` (crates `relight-addon-{noise-suppressor,deep-filter,compressor,voice}`, `package.workspace = "../../src-tauri"`), `addons/.taurignore` (`ui/`, `vendor/`); Modify `src-tauri/Cargo.toml` (members += 4, drop `exclude`, move `nnnoiseless` → noise-suppressor, `deep_filter` path dep + `tract-core/onnx/pulse/hir =0.21.4` + `ringbuf` → deep-filter, add path deps on the four crates), libDF `Cargo.toml` (append empty `[workspace]`), `plugins/addons.rs` (4 paths), `vite.config.ts` `watch.ignored` += `'**/addons/*/vendor/**','**/addons/*/src/**'`. Delete `plugins/builtin/`.

- [ ] **Step 1:** Move; only path edits inside moved files. **Step 2:** Gate + golden pass; `git log --follow --oneline addons/compressor/src/lib.rs` shows pre-split commits; `cargo tree --workspace -i deep_filter -e normal` → only `relight-addon-deep-filter`. Commit `refactor(addons): move add-on DSP into addons/`.

### Task 4a: Lock-free real-time boundary

**Files:** `plugins/core/instance.rs`, `addons/noise-suppressor/src/lib.rs` (`reset`).

**Protocol (exact):** fields `addon: Option<&'static AddonDescriptor>`, `addon_proc: Mutex<Option<Box<dyn AddonProcessor>>>`, `param_values: [AtomicU32; 64]`, `param_dirty: AtomicU64`, `meter_values: [AtomicU32; 8]`, `addon_latency: AtomicU32` (replace `builtin_processor`, `last_vad`).
1. Create: `proc.set_parameter(i, default)` directly; store defaults in `param_values`; `dirty = 0`; `addon_latency = proc.latency_samples()`.
2. `set_parameter` (any thread), under `parameters.write()`: clamp, write Vec, `param_values[i].store(bits, Relaxed)`, `param_dirty.fetch_or(1<<i, Release)`. Never touches `addon_proc`.
3. Audio thread inside successful `try_lock`: `swap(0, Acquire)` → apply set bits; seed `buf: [f32;8]` from `meter_values`; `process(l, r, &mut buf[..n])` in `protected_call`; store meters + latency Relaxed.
4. Display/snapshot from the `parameters` Vec only. `latency_samples()` = 0 if bypassed else `addon_latency`; `addon_meters() -> Vec<f32>` from `meter_values[..n]`; neither locks.
5. `set_bypassed(false)` from true: `try_lock_for(STATE_LOCK_TIMEOUT)` → `reset()`, then clear bypass (Release). No reset on bypass on. `Drop` uses `get_mut()`.
6. Sample-rate re-prepare unchanged (replay sets dirty bits).
7. Noise-suppressor `reset()`: clear + re-prime queues (out 1 frame, dry 2 frames), run 3 frames of zeros through each `DenoiseState` and discard, gate gain 1, last_vad 0.
8. `get_noise_suppressor_vad` command kept, returns `addon_meters()[0]`.

- [ ] **Step 1: Failing tests:** `set_parameter_never_needs_the_processor_lock` (hold `addon_proc.lock()`, `set_parameter(4, 30.0)`, drop guard, process 64×0.001 → `l[63] > 0.02`); `latency_and_meters_read_without_the_lock` (noise; hold lock; latency 960, meters len 1); `knob_moves_never_skip_an_addon_block` (compressor makeup 12, 100 ms warm-up; thread A 10 000× makeup ∈ [6, 30] dB; 2 000 blocks of 64×0.1 behind a `Barrier`; every sample ∈ [0.1·10^(6/20), 0.1·10^(30/20)]); `unbypass_does_not_replay_stale_audio` (noise mix 0; 4×256 of 0.5; bypass on/off; 4×256 zeros → all 1024 L and R exactly 0.0). Replace `set_parameter_reaches_processor_despite_brief_lock_contention`.
- [ ] **Step 2:** Implement; gate + golden. Commit `feat(addons): lock-free params, meters, latency and reset`.

### Task 4b: Saved-param restore, migration, meters command

**Files:** `plugins/core/instance.rs`, `core/session.rs` (`apply_saved_settings`), `commands/plugin.rs`, `lib.rs`, `src/lib/tauri.ts`.

- `PluginInstance::restore_parameters(&self, saved_version: Option<&str>, saved: &[PresetParameter])` called from `apply_saved_settings` (covers preset, session, autosave). Known add-on: `from = v.parse::<u32>().ok().unwrap_or(1)`; if `from < d.version` and `migrate` → run; then `set_parameter` each. `Builtin` without descriptor: keep params verbatim (`min = max = default = value`, name kept), keep saved version string, status Error `add-on '<id>' is not available in this version`. Other formats: unchanged loop.
- `#[tauri::command(async)] get_addon_meters(instance_ids: Vec<String>) -> Vec<(String, Vec<f32>)>`; `getAddonMeters` in `tauri.ts`.

- [ ] **Step 1: Failing tests:** `unknown_addon_keeps_saved_params` (`builtin::gone`, version "7", `[{0,"X",3.0}]` → load → snapshot → load → snapshot: same params, version "7", status Error); `resave_writes_current_addon_version` ("2.8.0" compressor → snapshot version "1"); `migrate_runs_only_from_older_versions` (test descriptor v2 doubling param 0: "2.8.0"/"1"/none migrate; "2"/"3" don't).
- [ ] **Step 2:** Implement; gate. Commit `feat(addons): versioned param restore and meters command`.

### Task 5a: Move GUIs (pure)

**Files:** `git mv` `src/components/plugin-gui/{NoiseSuppressor,DeepFilterNet,Compressor,Voice}Gui.tsx` → `addons/{noise-suppressor,deep-filter,compressor,voice}/ui/`; `TransferCurveCanvas.tsx` → `addons/compressor/ui/`; `BodePlotCanvas.tsx` → `addons/voice/ui/`; rest of `common/` → `addons/ui-common/`. Create `addons/ui-common/host.ts` (re-exports: `* as tauri` and `getPluginScope` from `src/lib/tauri`, type `PluginInstanceInfo`, `useTranslation`, `useThemeStore`, `useAudioStore`, `usePluginStore`, `useWindowVisibility`, `useVisibleInterval`), `addons/registry.ts` (`AddonGuiProps`, `ADDON_GUIS` with one static `lazy(() => import('./x/ui/XGui'))` per id). Modify `BuiltinPluginGuiSwitch.tsx` (use `ADDON_GUIS`), `tsconfig.json` `include: ["src","addons"]`, `vite.config.ts` (delete only the `/src/components/plugin-gui/` manualChunks line).

- [ ] **Step 1:** Move + rewire imports only. **Step 2:** tsc 0; `pnpm build 2>&1 | grep -cE "(NoiseSuppressor|DeepFilterNet|Compressor|Voice)Gui-|ui-common-"` → `5`. Commit `refactor(addons): move add-on GUIs into addons/`. (May run in parallel with Tasks 1–3: no shared files.)

### Task 5b: Per-add-on strings and labels

**Files:** Create `addons/<name>/ui/i18n.ts` ×4 (26/24/22/12 keys per locale for compressor/voice/noise/deepFilter, moved verbatim + the hard-coded GUI strings "48 kHz required, current: {rate} Hz", "VAD CONFIDENCE", "Tube Saturation"→ key); Modify `addons/registry.ts` (`ADDON_STRINGS = { en: {...}, vi: {...} }`), `src/i18n/types.ts` (drop 4 sections; `HostSchema`; `export type TranslationSchema = HostSchema & typeof ADDON_STRINGS.en`), `en.ts`/`vi.ts` (spread `ADDON_STRINGS`; labels per Global Constraints; delete unused `common.builtin`), `usePluginLibraryFilters.ts:26` alias.

- [ ] **Step 1:** Implement; a deliberately missing vi key must fail tsc (try once, revert). **Step 2:** Gate. Commit `refactor(addons): add-on owned strings, "Add-ons" labels`.

### Task 5c: Shared hooks and GUI behaviour

**Files:** Create `addons/ui-common/{throttle.ts,useParams.ts,useMeters.ts}`, `scripts/check-addon-ui.ts`; Modify the 4 GUIs, `WaveformDualCanvas`, `NeuralVADOrb`, `src/lib/tauri.ts` (delete `getNoiseSuppressorVad`), `commands/plugin.rs` + `lib.rs:219` (delete `get_noise_suppressor_vad`), `src/lib/types.ts` if needed.

- `createThrottledSender(send, ms = 40) → { set(id, v), dispose() }`: first change sent immediately and not queued; changes within 40 ms coalesced per id; `dispose()` flushes pending.
- `useParams(plugin, isOpen, defaults: Record<number, number>) → { values, set(id, v), resetAll() }`: initial from `plugin.parameters` with fallback; one `getPluginParameters` on open.
- `useMeters(instanceId, enabled) → readonly number[]`: `useVisibleInterval(tick, 33, enabled, [instanceId])`, in-flight guard, `getAddonMeters([instanceId])`; `[]` when disabled. GUIs pass `enabled = isOpen`.
- Noise/DeepFilter: VAD from `meters[0]`; 48 kHz alert when `crashStatusByInstanceId[id]?.type === 'Error'`. `WaveformDualCanvas`/`NeuralVADOrb` take `active={isOpen}` and stop polling/animating when false. Compressor keeps its scope-derived dot/GR until Task 7.

- [ ] **Step 1:** `scripts/check-addon-ui.ts` asserts: one change → one send; leading send immediate; trailing send carries latest value per id; `dispose` flushes. Prints `addon-ui checks ok`.
- [ ] **Step 2:** Implement; gate + `node scripts/check-addon-ui.ts` + golden. Commit `feat(addons): shared param/meter hooks for add-on GUIs`.

### Gate G: sound-change approval (coordinator asks the user before Tasks 6–9)

Items that change sound to fix a defect: noise gate timings (only presets with gate on); voice width formula (only width > 0); voice drive pre-gain 7.0 → 1.2 (THD at full drive on −6 dBFS 30% → 7.8%); deep-filter post-filter default 0.2 → 0.02 with range 0–0.1 + migration v1→v2 (`v/10`) — alternative: change only the default. Implement only approved items; the rest go to Task 10.

### Tasks 6–9: Per-add-on fixes (parallel, each confined to `addons/<name>/`; regenerate only its own golden files and say so in the commit; delete `addons/golden` after Task 9)

**Task 6 — noise-suppressor.** VAD = `vad_l.max(vad_r)`. Sanitize input before scaling (non-finite → 0, clamp ±1000; dry uses sanitized). Gate per frame aligned to output (frame k's VAD drives frame k's output; k+1 as lookahead allowed). Gate open 3 ms / close 150 ms / hold 200 ms; open at VAD ≥ threshold, close below threshold − 0.1. 10 ms one-pole on mix and output gain, snap within 1e-6. Meters `["vad","gate_db"]`. Fix comments `:77`, `:166`; delete `unsafe impl Send`.
Tests (helper `gliding_voice(n, onset)`: harmonic stack, f0 = 140 ± 40 Hz at 3 Hz, scale 0.25, −50 dBFS noise floor): `silent_right_channel_does_not_cap_vad` (L voice, R 0; mean vad frames 20–60 ≥ 0.8); `gate_opens_within_5ms_of_speech_onset` (onset 50·480+470, block 256, threshold 0.5, atten 1, mix 1, vs gate-off: RMS ratio over [s+1200, s+1440) ≥ 0.708, over [s−2400, s) ≤ 0.05); `non_finite_input_never_reaches_rnnoise` (NaN/+inf at 5000 → bit-identical to 0.0 run; 1e30 → finite); `wet_lag_is_960_with_gliding_pitch` (blocks 256/512/1024/100, lag ∈ 960 ± 4; replaces the modulo test); `reset_flushes_queues` (50 frames voice, reset, zeros → max |out| < 1e-3).

**Task 7 — compressor.** 10 ms one-pole on makeup (linear) and mix only; snap 1e-6. Meters `["in_db","gr_db","out_db"]` (block peaks; GR positive dB). Detector unchanged. GUI: live dot + GR from `useMeters`; delete its `getPluginScope` poll. `TransferCurveCanvas`: no clamp; Y max `max(0, ceil6(out(0 dBFS)))`; 0 dBFS line; curve above it re-stroked in `--rh-error`.
Tests: `gr_meter_reports_applied_reduction` (−6 dBFS 1 kHz, defaults, 1 s: |gr_db − 8.13| < 0.1, |out_db − (in_db − gr_db)| < 0.1); `mix_zero_is_identity` (after 100 ms bit-exact); `ratio_one_is_makeup_only` (ratio 1, makeup 12, after 100 ms gain 10^(12/20) ± 1e-5); `nan_resets_envelope`; `makeup_jump_is_click_free` (DC 0.01, makeup +30 → 0: max |Δy| < 1e-3).

**Task 8a — voice DSP.** Delete `< 0.05 dB` shortcut (`:57,:77,:97`) and `set_identity`. 20 ms one-pole on all six params; recompute EQ coefficients every sample only while a band is moving (snap 1e-3 dB). Width (if approved): `L = L_in + w·D`, `R = R_in − w·D`, D = mid delayed 12 ms. Drive (if approved): pre-gain `1 + 1.2·drive`. Meter `["limiter_gr_db"]`.
Tests: `band_to_zero_db_does_not_click` (each band, 100 Hz sine 0.25, +12 dB for 0.5 s → 0 and → 0.04: max |Δy| next 100 ms ≤ 1.5× previous 100 ms); `mono_sum_is_flat_at_full_width` (noise ±0.25: |(L+R)−(L₀+R₀)| < 1e-6 vs width 0); `full_drive_loses_at_most_3db_on_minus6_sine` (peak ≥ input − 3 dB, THD > 5%); Rust `low_shelf_100hz_matches_ui` (Low +12 at 100 Hz = 11.1028 ± 0.01).

**Task 8b — voice UI.** `addons/voice/ui/eqResponse.ts` (no imports): `lowShelf/highShelf(gainDb, f0, sr)`, `peak(gainDb, f0, q, sr)`, `magnitudeDb(h, f, sr)`, `voiceEqDb(low, mid, high, f, sr)` (200 Hz shelf, 2 kHz peak Q 0.7, 8 kHz shelf). `BodePlotCanvas` uses it with `sampleRate` from `useAudioStore`. Check `addons/voice/ui/bode.check.ts` (run by `node`): `voiceEqDb(12,0,0,100,48000)` = 11.1028 ± 0.01; Low +12 at 200 Hz = 6.00 ± 0.05; High +12 at 4 kHz = 0.69 ± 0.05. "Saturation" label via i18n.

**Task 9 — deep-filter.** Block-drop cap `dry_l.len() > n + 960 + n + 3·HOP`. Worker `set_atten_lim(atten.max(0.1))`. First `process(n)`: pre-push e = 480 + n − gcd(n,480) − ⌈480/n⌉·n zeros into `to_worker` and `dry_*`; on underrun resume skip `dry_ahead` from both queues with 2 ms crossfade. `reset()`: set `flush` atomic, re-seed `dry_*`; while flushing push nothing, output silence, drain `cons_from_worker`; worker drains input, zeroes `prev_noisy`, `model.init()`, clears flag; drain once more. `latency_samples()` = `dry_l.len() − dry_ahead` stored at end of `process`. Worker `StopOnDrop` guard sets `running = false` on panic. Inference error → `emit_hop(prod, None, noisy, prev)` emits `prev`. Non-finite input → 0. Post-filter (if approved): default 0.02, range 0–0.1, name "Post-filter Strength", `version: 2`, migrate id 1 `v → clamp(v/10, 0, 0.1)` when `from < 2`. Meter `["vad"]`; fix "no heap allocation" comment.
Tests (fake worker = previous hop): `large_blocks_do_not_drop_hops` (ramp input, n ∈ {256, 441, 2048, 4096}, output +1 per sample after start); `startup_never_underruns` (n ∈ {128,256,512,1024}); `underrun_resume_neither_repeats_nor_adds_latency` (stall 3 blocks); `latency_reports_pipeline_length` (impulse delay = `latency_samples()` = 960 + n − gcd(n,480)); `reset_drains_pipeline`; `inference_error_emits_previous_hop`; `worker_panic_falls_back_to_dry`; `non_finite_input_never_reaches_worker`; `migrate_v1_post_filter` (0.2→0.02, 1.0→0.1). Real model: `zero_attenuation_keeps_hop_alignment` (atten 0, mix 1, 256 blocks real-time-paced 2 s, xcorr lag = `latency_samples()` ± 4).

### Task 9b: Add-on error status (after 9)

API: `fn status(&self) -> AddonStatus { Ok, Loading, Error(&'static str) }` default `Ok`; host maps to `PluginStatus` after each block (no alloc: static str). Deep-filter reports `Loading` until the model is ready and `Error("model failed to load")` / worker panic. Test `deep_filter_worker_panic_shows_error`.

### Task 10: Options for the user

`docs/superpowers/specs/addons-review/options.md`: compressor sidechain HPF, auto-makeup, RMS mode, lookahead limiter, log-domain detector (measured: ripple worse); noise floor-in-dB, mono-sum; voice HPF, de-esser, presets, true-peak limiter, output gain, ADAA/2× oversampled drive, heavy-drive restore; deep-filter thresholds −15/35/35, fold Mix into attenuation, resampling 44.1/96 kHz, mono/linked mode, warm-up crossfade, controls layout, lighter model; un-bypass crossfade. Each with value/risk/size. Present; implement nothing.

## Happy case (final acceptance)

1. `cargo clippy --workspace --all-targets -- -D warnings` → exit 0, no `warning:`.
2. `cargo test --workspace --lib -- --skip enum_tests --skip mmcss` → all `ok`, filtered total 4.
3. Golden tests pass through Task 5c (then per-add-on regenerated in 6–9, deleted after 9).
4. `cargo tree --workspace -i deep_filter -e normal` → only `relight-addon-deep-filter`; features include `default-model-ll`.
5. `git grep -nE "plugins::builtin|create_builtin|builtin_initial_params" src-tauri/src` → empty; `src-tauri/src/plugins/builtin`, `src-tauri/vendor`, `src/components/plugin-gui` absent.
6. `pnpm exec tsc --noEmit` → 0; `node scripts/check-drop-target.ts`, `node scripts/check-addon-ui.ts`, `node addons/voice/ui/bode.check.ts` → pass.
7. `pnpm build` → exit 0 with separate `*Gui-*` chunks.
8. After Task 3 and at the end: `pnpm tauri build --no-bundle` → "Finished `release` profile"; exe ≈ 99.5 MB ± 2 MB; do not launch. (If asio-sys rebuilds: pip libclang workaround from memory.)
9. (Only when the user asks to push) open a PR; `gh pr checks` green.

Rollback: Phase 1 (Tasks 0–5a) lands as its own PR with a merge commit → `git revert -m 1 <merge>`.
