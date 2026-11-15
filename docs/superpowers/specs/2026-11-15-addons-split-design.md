# Add-ons: split the built-in plugins out of the main source

Date: 2026-11-15 · Status: approved 2026-11-15

## Intent
The four built-in processors (Noise Suppressor/RNNoise, DeepFilterNet, Compressor, Voice) stay compiled into
ReLightHost.exe, but their code moves out of the host source tree into self-contained **add-ons**, so each one
can be upgraded, versioned and tested on its own, and adding a new one is "new folder + one registry line".
After the split, each add-on gets a skeptical review-and-upgrade pass.

## Goals
- Each add-on = one folder holding its DSP (Rust crate), its parameter list/metadata, its React GUI and its tests.
- Host knows add-ons only through one small API crate and one registry list (no per-id `match` scattered in
  `builtin/mod.rs`, `scanner.rs`, `BuiltinPluginGuiSwitch.tsx`).
- DeepFilterNet's heavy deps (tract, vendored libDF) live only in its add-on crate.
- Existing presets/sessions keep loading unchanged.

## Non-goals
- Runtime loading (DLLs), separate distribution, a third-party add-on SDK.
- Changing the preset/session file format.

## Anti-goals
- Any user-visible regression in audio, parameters, presets or GUIs caused by the move itself.
- Real-time path gaining allocations, locks or dynamic dispatch beyond today's single `Box<dyn …>` call.

## Approaches considered
- **A. One crate per add-on + shared API crate** — chosen by the user. Clean boundaries, per-add-on deps and
  versions; costs a workspace setup and a one-time file move.
- B. One `relight-addons` crate with a module per add-on — simpler, but deps/versions stay coupled.
- C. Rename `builtin/` → `addons/` inside the host — cheapest, but doesn't actually separate anything.

## Design (decided)
```
addons/
  api/                 relight-addon-api: AddonProcessor trait, AddonDescriptor, ParamSpec
  noise-suppressor/    Cargo.toml, src/, ui/NoiseSuppressorGui.tsx
  deep-filter/         Cargo.toml, src/, ui/, vendor/DeepFilterNet (moved from src-tauri/vendor)
  compressor/
  voice/
  ui-common/           AudioKnob, BodePlotCanvas, … (moved from src/components/plugin-gui/common)
  registry.ts          id → lazy GUI component (frontend registry)
```
- **API crate** (no dependency on the host):
  `AddonProcessor` = today's `BuiltinProcessor` (process_stereo, set_parameter, latency_samples, meters).
  `AddonDescriptor { id, name, vendor, version, category, params: &'static [ParamSpec], create: fn(f32) -> Option<Box<dyn AddonProcessor>> }`.
- **Host**: `src-tauri/src/plugins/addons.rs` holds `ADDONS: &[&AddonDescriptor]` and converts descriptors to
  `PluginInfo`/`PluginParameter`; `scanner::builtin_plugins`, `instance.rs` creation and `builtin_initial_params`
  go through it. `plugins/builtin/` is deleted.
- **IDs stay** `builtin::<name>` and the stored `format: builtin` stays; the UI labels the group "Add-ons" (en/vi).
- **Frontend**: relative imports (no alias — it would need Vite + tsconfig `paths` kept in sync), tsconfig
  `include` adds `addons`; add-on GUIs reach host code only via `addons/ui-common/host.ts`;
  `BuiltinPluginGuiSwitch` reads `addons/registry.ts`.
- **Tests**: each add-on's unit tests move with it; host keeps integration tests (instance/session/autosave).

## Calls made (coordinator, correctable)
- **Add-on GUIs may import host modules** (`lib/tauri`, `lib/types`, stores, `windowVisibility`) via `addons/ui-common/host.ts` re-exports —
  no host-interface rewrite of the four GUIs. Reason: "only split the code"; a bridge layer is speculative.
- **Each add-on owns its UI strings**: `addons/<name>/ui/i18n.ts` exports `{ en, vi }`; host `src/i18n` merges
  them (types derived from the add-on objects), so "new folder + one registry line" holds.
- **Meters are in the API from day one**: `fn meters(&self) -> AddonMeters` (input/output/GR dB + VAD, written
  to atomics by the processor, read by the host without the processor lock); `get_vad` becomes a field of it.
  Reason: the compressor's fake GR meter is a known defect, and every review is likely to want real meters.
- **Labels**: the library tab and the format tag read "Add-ons" (en) / "Add-ons" (vi); add-on vendor becomes
  "ReLightHost" (display only — loading keys on `path`, not vendor).
- **Upgrade pass is in scope**; review results are saved to `docs/superpowers/specs/addons-review/<addon>.md`
  and the plan's upgrade phase is built from them.

## Upgrade pass (after the split)
Per-add-on skeptical reviews (running now, read-only) produce ranked proposals: correctness, real metering
through the API (e.g. replace the compressor's fake GR meter), latency/underrun behaviour, parameter UX,
non-48 kHz support. High-value, low-risk items get implemented per add-on folder in parallel; anything that
changes sound character or adds new parameters is listed for the user to pick.

## Left to the builder
Exact Cargo workspace wiring (workspace root vs path deps, `package.workspace`), CI/tauri-action path impact,
module layout inside each crate, how meters are generalised in the trait.
