# Add-ons Split & Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the four built-in processors into self-contained add-on crates/folders behind a small API, then fix and upgrade each add-on from its skeptical review.

**Architecture:** `src-tauri` becomes a Cargo workspace root whose members are `../addons/{api,noise-suppressor,deep-filter,compressor,voice}`; the host keeps one registry (`plugins/addons.rs`). Frontend GUIs move to `addons/<name>/ui`, shared widgets to `addons/ui-common`, wired by `addons/registry.ts`. Phase 1 is a behavior-preserving move; phase 2 makes the host lock-free toward add-ons; phase 3 upgrades each add-on in parallel inside its own folder.

**Tech Stack:** Rust 1.80 (Tauri 2, ringbuf 0.5, nnnoiseless 0.5, vendored libDF/tract 0.21.4), React + TypeScript + Vite, pnpm.

**Spec:** `docs/superpowers/specs/2026-11-15-addons-split-design.md`; reviews in `docs/superpowers/specs/addons-review/*.md`.

## Global Constraints

- Stored ids stay `builtin::noise_suppressor`, `builtin::deep_filter`, `builtin::compressor`, `builtin::voice`; `PluginInfo.path` = id; stored `format` stays `builtin`.
- No preset/session file-format change. Add-on version is stored in the existing `plugin_version` field as a decimal integer string; non-numeric (e.g. `"2.8.0"`) means version 1.
- Real-time path: no allocation, no lock other than the audio thread's own `try_lock` of the processor, no logging.
- Target dir stays `src-tauri/target`; workspace members are listed explicitly (no glob — `addons/ui-common` has no Cargo.toml).
- `addons/deep-filter/vendor/DeepFilterNet` is moved as one unit (libDF `include_bytes!("../../models/...")`); libDF is a path dependency, not a workspace member.
- UI labels: format tag and library tab read "Add-ons" (en and vi); add-on vendor string "ReLightHost".
- Each add-on owns its strings: `addons/<name>/ui/i18n.ts` exports `{ en, vi }`, merged into `src/i18n`.
- Frontend imports from add-on folders are relative; add-on GUIs reach host code only through `addons/ui-common/host.ts`.
- Changes that alter sound character in ways the reviews call optional, or add parameters, are NOT implemented; Task 10 lists them for the user.
- Verification gate for every task: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace --lib -- --skip enum_tests --skip mmcss` (in `src-tauri`), `pnpm exec tsc --noEmit`, `node scripts/check-drop-target.ts` (repo root).

## Review Focus

- Old presets/sessions/autosave with `builtin::*` ids and params by id must load with identical values after the move (Task 3 test).
- A preset referencing an unknown add-on id must keep its saved params through an autosave round-trip (Task 4 test).
- Un-bypass must not replay stale audio from before the bypass (Task 4 test with noise-suppressor).
- A knob move while audio runs must never make a block skip the add-on (Task 4 test: concurrent set_parameter vs process, every block processed).
- Sample rates other than 48 kHz: neural add-ons still report Error and pass audio through (existing test kept green in Task 3).

---

### Task 1: Workspace + API crate

**Files:**
- Create: `addons/api/Cargo.toml` (package `relight-addon-api`, `package.workspace = "../../src-tauri"`, deps: `ringbuf = "0.5"`, windows target dep `windows` with features already used by `audio/mmcss.rs`)
- Create: `addons/api/src/lib.rs`, `addons/api/src/rt.rs`
- Modify: `src-tauri/Cargo.toml` (add `[workspace] members = [...]`, dependency `relight-addon-api = { path = "../addons/api" }`)
- Modify: `src-tauri/src/audio/mixer.rs` (remove `BacklogTrimmer`, `pub use relight_addon_api::rt::BacklogTrimmer;`), `src-tauri/src/audio/mmcss.rs` (body moves to `rt`, keep `pub use` so callers compile)
- Modify: `.github/workflows/check.yml:48,54` → `--workspace` forms above

**Interfaces — Produces:**
```rust
pub enum Scale { Linear, Db, Log }
pub struct ParamSpec { pub id: u32, pub name: &'static str, pub min: f32, pub max: f32, pub default: f32,
    pub unit: &'static str, pub scale: Scale, pub step: f32, pub choices: &'static [&'static str] }
pub struct AddonDescriptor {
    pub id: &'static str, pub name: &'static str, pub category: &'static str, pub version: u32,
    pub params: &'static [ParamSpec],            // ≤ 64
    pub meters: &'static [&'static str],         // slot names; process() fills meters[i]
    pub sample_rates: &'static [u32],            // empty = any
    pub create: fn(sample_rate: f32, max_block: usize) -> Option<Box<dyn AddonProcessor>>,
    pub migrate: Option<fn(from: u32, params: &mut Vec<(u32, f32)>)>,
}
pub trait AddonProcessor: Send {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], meters: &mut [f32]);
    fn set_parameter(&mut self, id: u32, value: f32);
    fn reset(&mut self) {}
    fn latency_samples(&self) -> u32 { 0 }
}
// rt.rs
pub struct BacklogTrimmer { /* moved verbatim from audio/mixer.rs */ }
pub fn boost_current_thread_to_pro_audio() -> /* existing return type */;
```

- [ ] **Step 1:** Move `BacklogTrimmer` + its tests and `mmcss` + its tests into `addons/api/src/rt.rs` with `git mv`-equivalent content moves; add host re-exports.
- [ ] **Step 2:** Add the types above to `lib.rs`, plus test `descriptor_param_count_fits_dirty_mask` asserting a 65-param slice is rejected by `pub const fn check_params(p: &[ParamSpec]) -> bool` (returns `p.len() <= 64`).
- [ ] **Step 3:** Run the verification gate. Expected: all green, test count = previous 124 + 1.
- [ ] **Step 4:** Commit `refactor(addons): workspace and add-on API crate`.

### Task 2: Move the four DSP crates (behavior-preserving)

**Files:**
- Create via move: `addons/{noise-suppressor,deep-filter,compressor,voice}/{Cargo.toml,src/lib.rs}` from `src-tauri/src/plugins/builtin/*.rs`; `git mv src-tauri/vendor/DeepFilterNet addons/deep-filter/vendor/DeepFilterNet`
- Move deps: `nnnoiseless` → noise-suppressor; `deep_filter` (path `vendor/DeepFilterNet/libDF`, same features), `tract-*` pins → deep-filter. Remove them from `src-tauri/Cargo.toml`.

**Interfaces:**
- Consumes: Task 1 API.
- Produces: each crate exports `pub static ADDON: relight_addon_api::AddonDescriptor`. Crate names: `relight-addon-noise-suppressor`, `relight-addon-deep-filter`, `relight-addon-compressor`, `relight-addon-voice`. Meter slots: noise-suppressor `["vad"]`, deep-filter `["vad"]`, compressor `[]`, voice `[]` (phase 3 adds more).
- `params` copy the exact rows of today's `builtin_initial_params` (names, min/max/default); scale/unit: dB params `Scale::Db, "dB"`; Attack/Release `Scale::Log, "ms"`; Ratio `Scale::Log, ":1"`; rest `Linear, ""`. `version: 1`, `migrate: None`. `sample_rates`: `[48000]` for the neural two, `[]` otherwise.

- [ ] **Step 1:** Move files; adapt `process_stereo` → `process(l, r, meters)` writing VAD into `meters[0]` where it existed; `create` wraps today's `new(sample_rate)`.
- [ ] **Step 2:** Unit tests move with their DSP; host-integration tests stay in `instance.rs`/`session.rs`/`autosave.rs` and now use `relight_addon_compressor::ADDON.id` etc.
- [ ] **Step 3:** Verification gate; same test total as after Task 1.
- [ ] **Step 4:** Commit `refactor(addons): move built-in DSP into add-on crates`.

### Task 3: Host registry replaces `plugins/builtin`

**Files:**
- Create: `src-tauri/src/plugins/addons.rs`
- Delete: `src-tauri/src/plugins/builtin/`
- Modify: `plugins/mod.rs`, `plugins/core/instance.rs` (creation/params), `plugins/core/scanner.rs:531-570` (`builtin_plugins` → registry, vendor `"ReLightHost"`, `plugin_version` = descriptor version as string)

**Interfaces — Produces:**
```rust
pub static ADDONS: &[&AddonDescriptor];
pub fn find(id: &str) -> Option<&'static AddonDescriptor>;
pub fn initial_params(d: &AddonDescriptor) -> Vec<PluginParameter>;
pub fn plugin_info(d: &AddonDescriptor) -> PluginInfo;
```

- [ ] **Step 1: Write failing test** `old_builtin_preset_round_trips_identically` in `core/session.rs` tests: a preset JSON literal with `"path":"builtin::compressor"`, `"plugin_version":"2.8.0"`, params `[{id:0,value:-24.0},{id:1,value:6.0}]` loads; instance params equal those values; re-snapshot yields the same ids/values.
- [ ] **Step 2:** Implement registry and switch callers; run test → PASS; existing `builtin_that_cannot_run_at_this_rate_reports_error_and_passes_audio_through` stays green.
- [ ] **Step 3:** Verification gate. Commit `refactor(addons): host add-on registry`.

### Task 4: Lock-free host ↔ add-on boundary

**Files:** Modify `plugins/core/instance.rs`, `commands/plugin.rs`, `core/session.rs` (migration), `src/lib/tauri.ts`.

**Interfaces — Produces:**
- `PluginInstance::set_parameter` for add-ons writes `param_values: [AtomicU32; 64]` + `param_dirty: AtomicU64`; the audio thread, after acquiring the processor, applies `dirty.swap(0)` bits.
- `needs_reset: AtomicBool` set on un-bypass; audio thread calls `reset()` before the next `process`.
- After `process`, audio thread stores meters into `meter_values: [AtomicU32; 8]` and latency into `latency: AtomicU32`; `latency_samples()` and meter reads never lock.
- Command `get_addon_meters(instance_ids: Vec<String>) -> Vec<(String, Vec<f32>)>` (not on the plugin-host thread); `get_builtin_vad` removed (frontend switches in Task 5).
- Unknown add-on id: instance keeps saved params verbatim (stored as-is, re-emitted by snapshot), status Error "add-on '<id>' is not available in this version".
- Migration: on load, `from = plugin_version.parse::<u32>().unwrap_or(1)`; if `< d.version` and `d.migrate` is Some, run it; instance version = `d.version`.

- [ ] **Step 1: Failing tests** in `instance.rs` tests:
  - `knob_moves_never_skip_an_addon_block`: compressor at makeup 12 dB; thread A calls `set_parameter(4, x)` 10 000 times; thread B processes 2 000 blocks of 0.1; every output block ≈ 0.1·10^(makeup/20) for some makeup in range (none equals 0.1 exactly).
  - `unbypass_does_not_replay_stale_audio`: noise-suppressor, mix 0; process 4 blocks of 0.5, bypass, unbypass, process zeros: first 960 output samples are exactly 0.0.
  - `unknown_addon_keeps_saved_params`: instance for `builtin::gone` with params `[(0, 3.0)]` → snapshot returns `[(0, 3.0)]`, status Error.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS + verification gate.
- [ ] **Step 5:** Commit `feat(addons): lock-free params, meters and reset`.

### Task 5: Frontend move, registry, shared hooks, labels

**Files:**
- Move: `src/components/plugin-gui/{NoiseSuppressor,DeepFilterNet,Compressor,Voice}Gui.tsx` → `addons/<name>/ui/`; `src/components/plugin-gui/common/*` → `addons/ui-common/`
- Create: `addons/registry.ts` (`export const ADDON_GUIS: Record<string, LazyExoticComponent<...>>` with one static `lazy(() => import('./<name>/ui/<X>Gui'))` per id), `addons/ui-common/host.ts` (re-exports used host modules), `addons/ui-common/useParam.ts` (`useParam(instanceId: string, paramId: number, initial: number): [number, (v: number) => void]` — trailing throttle 40 ms, sends the first value once), `addons/ui-common/useMeters.ts` (`useMeters(instanceId: string, enabled: boolean): number[]` — one shared 30 Hz poller batching all subscribed ids via `getAddonMeters`, paused when window hidden), `addons/<name>/ui/i18n.ts`
- Modify: `src/components/chain/BuiltinPluginGuiSwitch.tsx` (uses `ADDON_GUIS`), `src/i18n/{en,vi,types}.ts` (merge add-on strings; `builtin`/`system` labels → "Add-ons"), `tsconfig.json` include `["src","addons"]`, `vite.config.ts` (drop the `plugin-gui` manualChunks rule; add `**/addons/*/vendor/**` to watch.ignored), `src/lib/tauri.ts` (`getAddonMeters`)
- Delete: `src/components/plugin-gui/`

- [ ] **Step 1:** Move + rewire; GUIs use `useParam`/`useMeters`; NoiseSuppressor/DeepFilter GUIs read VAD from meters and show the 48 kHz warning from instance status (not the audio store rate); compressor GUI shows GR only from its meter (until Task 7 adds `gr_db`, hide the GR meter and live dot).
- [ ] **Step 2:** `pnpm exec tsc --noEmit` → 0; `pnpm build` → succeeds and emits separate chunks per add-on GUI.
- [ ] **Step 3:** Commit `refactor(addons): move add-on GUIs and strings into addons/`.

### Tasks 6–9: Per-add-on fixes (parallel; each confined to `addons/<name>/`)

Each task: write failing tests named below → implement → green → verification gate → commit `fix(<name>): ...`. Bump `version` only when a param's range/meaning changes (none below do).

**Task 6 — noise-suppressor** (review: `addons-review/noise-suppressor.md`): VAD = `max(l, r)`; per-frame gate decision; gate open 3 ms / close 150 ms / hold 200 ms / hysteresis 0.1 (fixed, no new params); NaN input → 0 and rebuild state on non-finite output; `reset()` clears queues and re-primes; per-sample ramp on mix/output gain; meters `["vad","gate_db"]`; fix stale comments, delete `unsafe impl Send`.
Tests: `silent_right_channel_does_not_cap_vad`, `gate_opens_within_5ms_of_speech_onset`, `nan_input_recovers_within_two_frames`, `wet_lag_is_960_with_gliding_pitch` (replaces the modulo test; asserts `|lag-960| <= 4`), `reset_flushes_queues`.

**Task 7 — compressor** (review: `addons-review/compressor.md`): one-pole ~10 ms smoother on final gain factor; log-domain decoupled peak detector on GR so steady-state matches the static curve; meters `["in_db","gr_db","out_db"]`; GUI live dot + GR meter from meters; TransferCurveCanvas draws > 0 dBFS in red instead of clamping.
Tests: `steady_state_matches_static_curve` (−6 dBFS sine, defaults → GR within 0.3 dB of 9.0), `mix_zero_is_identity`, `ratio_one_is_makeup_only`, `nan_resets_envelope`, `makeup_jump_is_click_free` (max sample step after a +30→0 dB jump < 0.05).

**Task 8 — voice** (review: `addons-review/voice.md`): delete the `< 0.05 dB` identity shortcut (formulas at 0 dB); per-sample-smoothed dB with coefficient recompute every 32 samples; mono-safe width `L = M + w·S, R = M − w·S` with `M = (L+R)/2` and `S` = 12 ms delayed M (mono sum is exactly 2M for any S); drive: ADAA tanh and level-matched wet so Drive=1 on a −6 dBFS sine loses ≤ 3 dB; BodePlotCanvas computes the RBJ responses the DSP uses; label "Saturation" (not "Tube"); move hard-coded strings to i18n; meters `["limiter_gr_db"]`.
Tests: `band_to_zero_db_does_not_click` (max step ≤ 1.5× normal), `mono_sum_is_flat_at_full_width` (|L+R − 2M| < 1e-4), `full_drive_loses_at_most_3db_on_minus6_sine`; TS check that BodePlot gain at 200 Hz with Low +12 equals the Rust value (assert script under `scripts/`).

**Task 9 — deep-filter** (review: `addons-review/deep-filter.md` — save the pending review there before starting): apply its (A) defects list; `latency_samples` returns the current dry-pipeline length; meters `["vad"]`; NaN sanitize; `reset()` drains rings and dry buffer.
Tests: from the review's defects, plus `latency_reports_pipeline_length` and `reset_drains_pipeline`.

### Task 10: Option list for the user

- [ ] Write `docs/superpowers/specs/addons-review/options.md`: every (B) upgrade that adds a parameter or changes sound character by choice (compressor sidechain HPF / auto-makeup / RMS / lookahead; noise-suppressor floor-in-dB with migration, mono-sum; voice HPF, de-esser, presets, true-peak limiter, output gain; deep-filter items from its review), each with value/risk/size. Present it to the user; implement nothing from it in this plan.
