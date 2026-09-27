# Native Low-Latency Audio Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace cpal with two purpose-built native backends (ASIO via `asio-sys`, WASAPI exclusive-mode via `windows`) so ReLightHost reaches the lowest latency each backend can structurally offer, without changing `AudioManager`'s public API or any frontend/Tauri command surface.

**Architecture:** A new `audio/mixer.rs` holds the one shared per-block processing stage (mute/loopback/VU/plugin-chain/channel-offset) that both backends call. `audio/backend/asio.rs` drives a single ASIO duplex `bufferSwitch` callback with zero cross-thread buffering for same-driver I/O. `audio/backend/wasapi.rs` runs two dedicated exclusive-mode event-driven threads (capture, render) bridged by the existing lock-free ring buffer, with automatic shared-mode fallback. `device.rs` and `manager.rs` are rewritten to call these backends instead of cpal; their public signatures do not change.

**Tech Stack:** Rust, `asio-sys` 0.4 (direct ASIO SDK bindings), `windows` 0.62 (Win32 Core Audio COM), `ringbuf` 0.5 (kept, for cross-device bridging), `windows-sys` (kept, unrelated Win32 UI code, untouched).

**Spec:** `docs/superpowers/specs/2026-09-27-native-audio-backend-design.md`

## Global Constraints

- `AudioManager`'s public methods (`start`, `stop`, `toggle_monitoring`, `set_muted`, `is_muted`, `set_loopback`, `is_loopback_enabled`, `set_output_device`, `set_input_device`, `set_input_channel_offset`, `set_output_channel_offset`, `set_virtual_output_device`, `set_sample_rate`, `set_buffer_size`, `get_status`, `get_config`, `get_vu_data`, `set_process_callback`) keep their exact existing signatures — no changes in `commands/audio.rs` or the TS frontend.
- No allocation, no blocking lock, and no syscall other than the platform's wait primitive inside any real-time audio callback/thread body (matches the existing codebase's non-blocking `try_lock`/`try_push`/`try_pop` discipline).
- DirectSound and any non-ASIO/non-WASAPI cpal host are dropped — not reimplemented.
- `cpal` is removed from `Cargo.toml` only in the final integration task (Task 8), after both backends are proven working, so the app stays buildable and runnable throughout the plan.
- Every new `unsafe` block wrapping a Win32/COM/ASIO SDK call must check and propagate the `HRESULT`/`AsioError` — no `unwrap()` on driver/COM calls in real-time code paths.

## Review Focus

- WASAPI exclusive-mode `Initialize` returns `AUDCLNT_E_BUFFERSIZE_NOT_ALIGNED`: must retry once with the driver-reported aligned size, not fail outright (Task 8).
- WASAPI exclusive-mode `Initialize` fails entirely (device already claimed exclusively by another app): must fall back to shared-mode and still start monitoring, not error out (Task 8).
- Two different ASIO drivers selected as input and output simultaneously (not the same physical interface): must still bridge audio through the ring buffer, not silently produce silence (Task 6).
- Mono input device (reports 1 channel): left sample must be duplicated to the right channel, matching today's behavior (Task 6, Task 8).
- Device unplugged / invalidated while monitoring: must stop monitoring gracefully (logged, `is_monitoring` set false), never panic or crash the process (Task 6, Task 8).

---

## Task 1: Fix VST3 sandbox IPC timeout (Finding C)

**Files:**
- Modify: `src-tauri/src/plugins/processor/vst3_sandbox/mod.rs:40`
- Test: `src-tauri/src/plugins/processor/vst3_sandbox/mod.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing new.
- Produces: `fn response_timeout(block_size: u32, sample_rate: u32) -> std::time::Duration`, used wherever `PROCESS_RESPONSE_TIMEOUT` was used.

- [ ] **Step 1: Read the current constant and its call site**

Run: `grep -n "PROCESS_RESPONSE_TIMEOUT" src-tauri/src/plugins/processor/vst3_sandbox/mod.rs`
Expected: shows the `const PROCESS_RESPONSE_TIMEOUT: Duration = Duration::from_millis(20);` definition and the `recv_timeout(PROCESS_RESPONSE_TIMEOUT)` call site.

- [ ] **Step 2: Write the failing test**

```rust
#[cfg(test)]
mod timeout_tests {
    use super::response_timeout;
    use std::time::Duration;

    #[test]
    fn scales_with_block_size_and_sample_rate() {
        // 128 samples @ 48kHz = 2.666ms block; timeout must exceed it but
        // not be pinned to the old fixed 20ms regardless of input.
        let t_small = response_timeout(128, 48_000);
        assert!(t_small > Duration::from_micros(2_666));
        assert!(t_small < Duration::from_millis(20));

        // 2048 samples @ 48kHz = ~42.7ms block; timeout must scale up past
        // the old fixed 20ms constant instead of truncating the plugin.
        let t_large = response_timeout(2048, 48_000);
        assert!(t_large > Duration::from_millis(20));
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p app_lib response_timeout --lib`
Expected: FAIL with "cannot find function `response_timeout`"

- [ ] **Step 4: Implement the minimal function and use it at the call site**

Replace the `const PROCESS_RESPONSE_TIMEOUT: Duration = Duration::from_millis(20);` line with:

```rust
/// Sandbox round-trip budget: the block's own duration plus a fixed
/// scheduling/IPC margin, instead of a fixed constant that either wastes
/// time on small blocks or truncates large ones.
fn response_timeout(block_size: u32, sample_rate: u32) -> std::time::Duration {
    let block = std::time::Duration::from_secs_f64(block_size as f64 / sample_rate as f64);
    block + std::time::Duration::from_millis(8)
}
```

Update the `recv_timeout(...)` call site to `recv_timeout(response_timeout(self.block_size, self.sample_rate))` (use whichever fields the surrounding `SandboxedVst3Processor` struct already stores for these two values — confirm the exact field names with `grep -n "block_size\|sample_rate" src-tauri/src/plugins/processor/vst3_sandbox/mod.rs` before editing, since this plan does not re-derive the full struct definition).

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p app_lib response_timeout --lib`
Expected: PASS

- [ ] **Step 6: Full crate build check**

Run: `cargo build -p app_lib`
Expected: builds with no errors (confirms the call-site field names used in Step 4 were correct).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/plugins/processor/vst3_sandbox/mod.rs
git commit -m "fix(vst3-sandbox): scale IPC response timeout with block duration"
```

---

## Task 2: Extract the shared mixer stage (`audio/mixer.rs`)

**Files:**
- Create: `src-tauri/src/audio/mixer.rs`
- Modify: `src-tauri/src/audio/manager.rs:507-621` (replace inline closure body with a call into `mixer::process_block`)
- Modify: `src-tauri/src/audio/mod.rs` (or wherever `audio`'s submodules are declared — add `pub mod mixer;`)
- Test: `src-tauri/src/audio/mixer.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Produces:
  ```rust
  pub struct MixerState {
      pub process_fn: std::sync::Arc<std::sync::Mutex<Option<Box<dyn Fn(&mut [f32], &mut [f32]) + Send + 'static>>>>,
      pub vu_meter: std::sync::Arc<crate::audio::vu_meter::VUMeter>,
      pub muted: std::sync::Arc<std::sync::atomic::AtomicBool>,
      pub loopback_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
      pub dsp_load_u32: std::sync::Arc<std::sync::atomic::AtomicU32>,
      pub output_is_asio: bool,
  }

  /// Runs the plugin chain + mute/loopback gating on one block of already
  /// input-populated `left`/`right` (in place). Returns whether the
  /// virtual-output mirror should receive this block, so the caller (which
  /// owns the virtual ring buffer producer) can push it.
  pub fn process_block(left: &mut [f32], right: &mut [f32], state: &MixerState, sample_rate_hz: f64) -> bool
  ```
- Consumes (later tasks): backend callbacks call `mixer::process_block` after filling `left`/`right` from their input source, then check the returned `bool` before mirroring to the virtual-output producer they own.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn state(output_is_asio: bool) -> MixerState {
        MixerState {
            process_fn: std::sync::Arc::new(std::sync::Mutex::new(None)),
            vu_meter: std::sync::Arc::new(crate::audio::vu_meter::VUMeter::new()),
            muted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            loopback_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            dsp_load_u32: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            output_is_asio,
        }
    }

    #[test]
    fn main_output_gate_polarity_matches_backend() {
        // ASIO: main output follows mute (open when NOT muted).
        assert!(main_output_gate_open(true, false, false));
        assert!(!main_output_gate_open(true, true, false));
        // Non-ASIO: main output follows loopback, independent of mute.
        assert!(main_output_gate_open(false, true, true));
        assert!(!main_output_gate_open(false, false, false));
    }

    #[test]
    fn asio_virtual_mirror_follows_loopback_not_mute() {
        let s = state(true);
        s.muted.store(true, Ordering::Relaxed);
        s.loopback_enabled.store(true, Ordering::Relaxed);
        let mut l = vec![0.5f32; 4];
        let mut r = vec![0.5f32; 4];
        let mirror = process_block(&mut l, &mut r, &s, 48_000.0);
        assert!(mirror, "ASIO output: virtual mirror must follow loopback flag, not mute");
    }

    #[test]
    fn plugin_chain_runs_when_lock_uncontended() {
        let s = state(false);
        *s.process_fn.lock().unwrap() = Some(Box::new(|l: &mut [f32], r: &mut [f32]| {
            for s in l.iter_mut() { *s *= 0.5; }
            for s in r.iter_mut() { *s *= 0.5; }
        }));
        let mut l = vec![1.0f32; 4];
        let mut r = vec![1.0f32; 4];
        process_block(&mut l, &mut r, &s, 48_000.0);
        assert_eq!(l[0], 0.5);
        assert_eq!(r[0], 0.5);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p app_lib mixer:: --lib`
Expected: FAIL with "unresolved module `mixer`" (module doesn't exist yet)

- [ ] **Step 3: Implement `mixer.rs`**

```rust
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use crate::audio::vu_meter::VUMeter;

pub struct MixerState {
    pub process_fn: Arc<Mutex<Option<Box<dyn Fn(&mut [f32], &mut [f32]) + Send + 'static>>>>,
    pub vu_meter: Arc<VUMeter>,
    pub muted: Arc<std::sync::atomic::AtomicBool>,
    pub loopback_enabled: Arc<std::sync::atomic::AtomicBool>,
    pub dsp_load_u32: Arc<std::sync::atomic::AtomicU32>,
    pub output_is_asio: bool,
}

/// Runs the plugin chain in place on `left`/`right`, updates the VU meter
/// and DSP-load estimate, and resolves the mute/loopback gate. Returns
/// whether the caller should mirror this block to the virtual-output
/// producer (the caller owns that ring buffer, not this function).
///
/// Extracted verbatim from the original `AudioManager::toggle_monitoring`
/// output-stream closure so both the ASIO and WASAPI backends share one
/// implementation instead of two hand-kept-in-sync copies.
pub fn process_block(left: &mut [f32], right: &mut [f32], state: &MixerState, sample_rate_hz: f64) -> bool {
    let t0 = std::time::Instant::now();
    if let Ok(guard) = state.process_fn.try_lock() {
        if let Some(ref f) = *guard {
            f(left, right);
            let dsp_ns = t0.elapsed().as_nanos() as f64;
            let block_ns = left.len() as f64 / sample_rate_hz * 1_000_000_000.0;
            let measured = ((dsp_ns / block_ns) * 100.0).clamp(0.0, 100.0) as f32;
            let old = f32::from_bits(state.dsp_load_u32.load(Ordering::Relaxed));
            let smoothed = old * 0.9 + measured * 0.1;
            state.dsp_load_u32.store(smoothed.to_bits(), Ordering::Relaxed);
        }
    }

    state.vu_meter.update(left, right, t0);

    let is_muted = state.muted.load(Ordering::Relaxed);
    let is_loopback = state.loopback_enabled.load(Ordering::Relaxed);

    // ASIO: main output follows mute, virtual mirror follows loopback.
    // Non-ASIO: main output follows loopback, virtual mirror follows !mute.
    // (Matches the pre-existing manager.rs gate polarity exactly.)
    let mirror_to_virtual = if state.output_is_asio { is_loopback } else { !is_muted };
    mirror_to_virtual
}

/// Resolves whether the main hardware-output path should currently be
/// silent. Callers combine this with `frame < frames_to_process` from
/// their own ring-buffer-drain bookkeeping.
pub fn main_output_gate_open(output_is_asio: bool, muted: bool, loopback: bool) -> bool {
    if output_is_asio { !muted } else { loopback }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p app_lib mixer:: --lib`
Expected: PASS (all 3 tests)

- [ ] **Step 5: Wire `manager.rs`'s existing cpal output closure to call the new functions**

In `manager.rs`, replace the body between "Step 2: Run plugin chain" and "Step 3: Re-interleave" (currently lines ~557-621) with calls to `mixer::process_block` and `mixer::main_output_gate_open`, keeping the surrounding ring-buffer-drain and re-interleave loops (which stay cpal-specific until Task 8) unchanged. Build a `MixerState` once outside the closure (alongside the existing `Arc::clone` calls at manager.rs:507-512) and move it into the closure instead of the individual `Arc`s.

- [ ] **Step 6: Full manual regression check (existing cpal path still works)**

Use the `run` skill to launch the app, start monitoring on the current default device, and confirm: audio passes through, mute silences output, loopback mirrors to virtual output, VU meter moves. This proves the extraction is behavior-preserving before any new backend code is introduced.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/audio/mixer.rs src-tauri/src/audio/manager.rs src-tauri/src/audio/mod.rs
git commit -m "refactor(audio): extract shared mixer stage from output callback"
```

---

## Task 3: MMCSS helper (`audio/mmcss.rs`)

**Files:**
- Create: `src-tauri/src/audio/mmcss.rs`
- Modify: `src-tauri/src/audio/mod.rs` (add `#[cfg(target_os = "windows")] pub mod mmcss;` — gated, matching the codebase's existing convention for Windows-only modules such as `main.rs`'s and `plugins/gui/vst3.rs`'s Windows-specific code, since the `windows` crate dependency itself is only declared under that same cfg in Cargo.toml)
- Modify: `src-tauri/Cargo.toml` (add `windows` dependency)

**Interfaces:**
- Produces: `pub fn boost_current_thread_to_pro_audio() -> Option<windows::Win32::System::Threading::HANDLE>` — call once per real-time thread; returns the MMCSS task handle to keep alive for the thread's lifetime (dropping/reverting it is not required for a thread that lives until process exit, but the handle is returned so a backend can `AvRevertMmThreadCharacteristics` on clean shutdown if desired).

- [ ] **Step 1: Add the `windows` dependency**

In `src-tauri/Cargo.toml`, inside the existing `[target.'cfg(target_os = "windows")'.dependencies]` block, add beneath the existing `windows-sys` line:

```toml
windows = { version = "0.62", features = [
    "Win32_Media_Audio",
    "Win32_System_Threading",   # AvSetMmThreadCharacteristicsW, and later
                                # WaitForSingleObject/CreateEventW (Task 7)
    "Win32_System_Com",
    "Win32_Foundation",
] }
```

- [ ] **Step 2: Write the test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_does_not_panic_and_is_idempotent_per_call() {
        // Real MMCSS registration requires the Multimedia Class Scheduler
        // service to be running (it is, on every non-Server-Core Windows
        // install) — this exercises the real Win32 call rather than a mock,
        // since the whole point of this helper is the syscall succeeding.
        let handle1 = boost_current_thread_to_pro_audio();
        assert!(handle1.is_some(), "AvSetMmThreadCharacteristicsW should succeed on a normal Windows dev machine");
        let handle2 = boost_current_thread_to_pro_audio();
        assert!(handle2.is_some());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p app_lib mmcss:: --lib`
Expected: FAIL with "unresolved module `mmcss`"

- [ ] **Step 4: Implement**

```rust
#![cfg(target_os = "windows")]

use windows::core::PCWSTR;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::Win32::Foundation::HANDLE;

/// Registers the calling thread with MMCSS under the "Pro Audio" task
/// profile, which raises its scheduling priority and reduces the chance
/// the OS scheduler preempts it mid-buffer — the difference between
/// needing a larger safety-margin buffer size and not.
///
/// Call once per real-time audio thread (ASIO callback thread, WASAPI
/// capture thread, WASAPI render thread), guarded by a `std::sync::Once`
/// at each call site so it only runs on the first callback invocation.
pub fn boost_current_thread_to_pro_audio() -> Option<HANDLE> {
    let name: Vec<u16> = "Pro Audio\0".encode_utf16().collect();
    let mut task_index: u32 = 0;
    let handle = unsafe {
        AvSetMmThreadCharacteristicsW(PCWSTR(name.as_ptr()), &mut task_index)
    };
    match handle {
        Ok(h) if !h.is_invalid() => Some(h),
        _ => {
            log::warn!("{} AvSetMmThreadCharacteristicsW(\"Pro Audio\") failed", crate::core::threading::thread_prefix("audio/mmcss"));
            None
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p app_lib mmcss:: --lib`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/audio/mmcss.rs src-tauri/src/audio/mod.rs
git commit -m "feat(audio): add MMCSS Pro Audio thread-boost helper"
```

---

## Task 4: ASIO backend — driver enumeration (`backend/asio.rs` part 1)

**Files:**
- Create: `src-tauri/src/audio/backend/mod.rs`
- Create: `src-tauri/src/audio/backend/asio.rs`
- Modify: `src-tauri/Cargo.toml` (add `asio-sys` dependency)
- Modify: `src-tauri/src/audio/mod.rs` (add `pub mod backend;`)
- Test: `src-tauri/src/audio/backend/asio.rs` (inline `#[cfg(test)]`, enumeration-only — no live driver required for the pure-logic parts)

**Interfaces:**
- Produces:
  ```rust
  pub struct AsioDeviceInfo { pub name: String, pub input_channels: usize, pub output_channels: usize }
  pub fn list_asio_devices() -> Vec<AsioDeviceInfo>
  ```
- Consumes (Task 7): `device.rs` calls `list_asio_devices()` in place of the current cpal-based ASIO enumeration branch.

- [ ] **Step 1: Add the `asio-sys` dependency**

In `src-tauri/Cargo.toml`, replace the line:
```toml
cpal = { version = "0.18", features = ["asio"] }
```
with (keep cpal for now — it is only removed in Task 8 — add asio-sys alongside it):
```toml
cpal = { version = "0.18", features = ["asio"] }
asio-sys = "0.4"
```

- [ ] **Step 2: Write the test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_asio_devices_reports_consistent_channel_counts_or_none() {
        // On a machine with no ASIO drivers registered this returns an
        // empty Vec (the real assertion is that the call completes without
        // panicking or erroring at all — a fixed count can't be asserted
        // since it depends on what's installed on the build machine). Every
        // entry that IS returned must have a non-empty name, since an
        // unnamed device is not something the UI can list.
        let devices = list_asio_devices();
        assert!(devices.iter().all(|d| !d.name.is_empty()));
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p app_lib backend::asio --lib`
Expected: FAIL with "unresolved module `backend`"

- [ ] **Step 4: Implement `backend/mod.rs`**

```rust
pub mod asio;
pub mod wasapi;
```

- [ ] **Step 5: Implement `backend/asio.rs` enumeration**

```rust
use asio_sys::Asio;

pub struct AsioDeviceInfo {
    pub name: String,
    pub input_channels: usize,
    pub output_channels: usize,
}

/// Lists every ASIO driver registered in the Windows registry, with its
/// channel counts. Loading a driver briefly to query channels is required
/// by the ASIO SDK (channel counts aren't in the registry) — this mirrors
/// what cpal's own ASIO host does today.
pub fn list_asio_devices() -> Vec<AsioDeviceInfo> {
    let asio = Asio::new();
    let mut out = Vec::new();
    for name in asio.driver_names() {
        let Ok(driver) = asio.load_driver(&name) else { continue };
        let Ok(channels) = driver.channels() else { continue };
        out.push(AsioDeviceInfo {
            name,
            input_channels: channels.ins.max(0) as usize,
            output_channels: channels.outs.max(0) as usize,
        });
    }
    out
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p app_lib backend::asio --lib`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/audio/backend/mod.rs src-tauri/src/audio/backend/asio.rs src-tauri/src/audio/mod.rs
git commit -m "feat(audio): add ASIO driver enumeration via asio-sys"
```

---

## Task 5: ASIO backend — duplex stream (`backend/asio.rs` part 2)

**Files:**
- Modify: `src-tauri/src/audio/backend/asio.rs`

**Interfaces:**
- Consumes: `crate::audio::mixer::{MixerState, process_block, main_output_gate_open}` (Task 2); `crate::audio::mmcss::boost_current_thread_to_pro_audio` (Task 3).
- Produces:
  ```rust
  pub struct AsioDuplexStream { /* opaque; owns the Driver + registered callback */ }

  /// `in_offset`/`out_offset`: 0-based index of the first of the selected
  /// stereo pair, clamped by the caller the same way manager.rs does today.
  pub fn start_duplex(
      driver_name: &str,
      in_offset: usize,
      out_offset: usize,
      buffer_size_hint: Option<i32>,
      mixer: crate::audio::mixer::MixerState,
      virt_producer: Option<ringbuf::HeapProd<f32>>,
  ) -> anyhow::Result<AsioDuplexStream>

  pub fn stop(stream: AsioDuplexStream)
  ```
- Cross-driver bridging (Review Focus item: two different ASIO drivers as input/output) is **not** handled by this function — that case is composed at the `manager.rs` level in Task 7 by running two independent `start_duplex`-like single-direction registrations bridged through the existing `ringbuf::HeapRb`, exactly mirroring today's non-`same_asio_device` cpal branch. This function only covers the single-driver full-duplex case.

- [ ] **Step 1: Write the test** (pure logic only — sample conversion — the live callback itself needs a real driver and is covered by the manual verification pass in Task 9)

```rust
#[cfg(test)]
mod duplex_tests {
    use super::*;

    #[test]
    fn f32_to_asio_int32_lsb_round_trips_at_boundaries() {
        assert_eq!(f32_to_asio_i32(1.0), i32::MAX);
        assert_eq!(f32_to_asio_i32(-1.0), i32::MIN + 1); // clamp symmetry, matches existing f32_to_i16 pattern
        assert_eq!(f32_to_asio_i32(0.0), 0);
    }

    #[test]
    fn asio_int32_lsb_to_f32_round_trips_at_boundaries() {
        let back = asio_i32_to_f32(i32::MAX);
        assert!((back - 1.0).abs() < 0.0001);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app_lib backend::asio::duplex_tests --lib`
Expected: FAIL with "cannot find function `f32_to_asio_i32`"

- [ ] **Step 3: Implement sample conversion + duplex stream**

```rust
use std::sync::Once;
use std::sync::atomic::{AtomicUsize, Ordering};
use asio_sys::{Asio, AsioBufferInfo, CallbackInfo, Driver};
use ringbuf::{HeapProd, traits::Producer};
use crate::audio::mixer::{MixerState, process_block, main_output_gate_open};

fn f32_to_asio_i32(v: f32) -> i32 {
    (v.max(-1.0).min(1.0) * i32::MAX as f32) as i32
}

fn asio_i32_to_f32(v: i32) -> f32 {
    v as f32 / i32::MAX as f32
}

pub struct AsioDuplexStream {
    driver: Driver,
}

pub fn start_duplex(
    driver_name: &str,
    in_offset: usize,
    out_offset: usize,
    buffer_size_hint: Option<i32>,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<f32>>,
) -> anyhow::Result<AsioDuplexStream> {
    let asio = Asio::new();
    let driver = asio.load_driver(driver_name)
        .map_err(|e| anyhow::anyhow!("Failed to load ASIO driver '{driver_name}': {e}"))?;

    let mut input_infos = vec![
        AsioBufferInfo { is_input: 1, channel_num: in_offset as i32, buffers: [std::ptr::null_mut(); 2] },
        AsioBufferInfo { is_input: 1, channel_num: (in_offset + 1) as i32, buffers: [std::ptr::null_mut(); 2] },
    ];
    let mut output_infos = vec![
        AsioBufferInfo { is_input: 0, channel_num: out_offset as i32, buffers: [std::ptr::null_mut(); 2] },
        AsioBufferInfo { is_input: 0, channel_num: (out_offset + 1) as i32, buffers: [std::ptr::null_mut(); 2] },
    ];

    let streams = driver.prepare_input_stream(None, 2, buffer_size_hint)
        .and_then(|_| driver.prepare_output_stream(None, 2, buffer_size_hint))
        .map_err(|e| anyhow::anyhow!("Failed to prepare ASIO buffers: {e}"))?;
    // `prepare_output_stream` above returns the combined AsioStreams; pull
    // the actual buffer_infos it allocated (positions match what we asked
    // for: 2 input channels then 2 output channels) rather than the
    // pre-buffer-creation placeholders declared above.
    let combined = streams;
    input_infos = combined.input.as_ref().map(|s| s.buffer_infos.clone()).unwrap_or_default();
    output_infos = combined.output.as_ref().map(|s| s.buffer_infos.clone()).unwrap_or_default();
    let buffer_size = combined.output.as_ref().or(combined.input.as_ref())
        .map(|s| s.buffer_size).unwrap_or(0) as usize;

    let sample_rate = driver.sample_rate().unwrap_or(48_000.0);
    let mmcss_once = Once::new();
    let mut left_buf = vec![0.0f32; buffer_size];
    let mut right_buf = vec![0.0f32; buffer_size];
    let output_is_asio = mixer.output_is_asio;

    driver.add_callback(move |info: &CallbackInfo| {
        mmcss_once.call_once(|| { crate::audio::mmcss::boost_current_thread_to_pro_audio(); });

        let idx = info.buffer_index as usize;
        // Read input: input_infos[0] = left, input_infos[1] = right.
        for (frame, sample) in left_buf.iter_mut().enumerate() {
            let ptr = input_infos[0].buffers[idx] as *const i32;
            *sample = unsafe { asio_i32_to_f32(*ptr.add(frame)) };
        }
        for (frame, sample) in right_buf.iter_mut().enumerate() {
            let ptr = input_infos[1].buffers[idx] as *const i32;
            *sample = unsafe { asio_i32_to_f32(*ptr.add(frame)) };
        }

        let mirror = process_block(&mut left_buf, &mut right_buf, &mixer, sample_rate);
        let is_muted = mixer.muted.load(Ordering::Relaxed);
        let is_loopback = mixer.loopback_enabled.load(Ordering::Relaxed);
        let gate_open = main_output_gate_open(output_is_asio, is_muted, is_loopback);

        if mirror {
            if let Some(ref mut vp) = virt_producer {
                for frame in 0..left_buf.len() {
                    let _ = vp.try_push(left_buf[frame]);
                    let _ = vp.try_push(right_buf[frame]);
                }
            }
        }

        for (frame, sample) in left_buf.iter().enumerate() {
            let ptr = output_infos[0].buffers[idx] as *mut i32;
            let value = if gate_open { f32_to_asio_i32(*sample) } else { 0 };
            unsafe { *ptr.add(frame) = value; }
        }
        for (frame, sample) in right_buf.iter().enumerate() {
            let ptr = output_infos[1].buffers[idx] as *mut i32;
            let value = if gate_open { f32_to_asio_i32(*sample) } else { 0 };
            unsafe { *ptr.add(frame) = value; }
        }
    });

    driver.start().map_err(|e| anyhow::anyhow!("Failed to start ASIO driver: {e}"))?;
    Ok(AsioDuplexStream { driver })
}

pub fn stop(stream: AsioDuplexStream) {
    let _ = stream.driver.stop();
    let _ = stream.driver.dispose_buffers();
}
```

**Note for the implementer:** the sample-type conversion above assumes
`AsioSampleType::ASIOSTInt32LSB`, the most common native format for
consumer/prosumer ASIO drivers. Before wiring this into `manager.rs`
(Task 7), call `driver.input_data_type()` / `output_data_type()` once
after `prepare_*_stream` and branch to the matching conversion (the
plan's Task 7 test list includes a case for a driver reporting
`ASIOSTFloat32LSB`, the other common format, to catch a driver that isn't
Int32).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p app_lib backend::asio::duplex_tests --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/audio/backend/asio.rs
git commit -m "feat(audio): add ASIO single-driver duplex callback backend"
```

---

## Task 6: WASAPI backend — device enumeration (`backend/wasapi.rs` part 1)

**Files:**
- Create: `src-tauri/src/audio/backend/wasapi.rs`
- Modify: `src-tauri/src/audio/backend/mod.rs` (already declares `pub mod wasapi;` from Task 4 — no change needed)

**Interfaces:**
- Produces:
  ```rust
  pub struct WasapiDeviceInfo { pub id: String, pub name: String, pub is_default: bool, pub input_channels: usize, pub output_channels: usize }
  pub fn list_wasapi_devices() -> Vec<WasapiDeviceInfo>
  ```
- Consumes (Task 8): the exclusive-mode stream setup resolves a device by the `id: String` this returns (the WASAPI endpoint ID string, stable across enumeration calls).

- [ ] **Step 1: Write the test**

```rust
#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn list_wasapi_devices_returns_at_least_the_default_render_device() {
        // Every Windows dev/CI machine has at least a default render
        // endpoint (even if it's a dummy/HDMI one) — this is a real,
        // no-mock check that CoInitializeEx + enumeration succeed.
        let devices = list_wasapi_devices();
        assert!(devices.iter().any(|d| d.output_channels > 0), "expected at least one render endpoint");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app_lib backend::wasapi --lib`
Expected: FAIL with "cannot find function `list_wasapi_devices`"

- [ ] **Step 3: Implement**

```rust
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eAll, eCapture, eConsole, eRender, DEVICE_STATE_ACTIVE, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

pub struct WasapiDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub input_channels: usize,
    pub output_channels: usize,
}

fn ensure_com_initialized() {
    // Idempotent per-thread: CoInitializeEx returns S_FALSE (still Ok) if
    // already initialized on this thread; ignore "already initialized"
    // failures from a prior different concurrency model since enumeration
    // itself doesn't require a specific one beyond MULTITHREADED here.
    unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
}

pub fn list_wasapi_devices() -> Vec<WasapiDeviceInfo> {
    ensure_com_initialized();
    let mut out = Vec::new();
    let enumerator: windows::core::Result<IMMDeviceEnumerator> = unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
    };
    let Ok(enumerator) = enumerator else { return out };

    for (flow, is_output) in [(eRender, true), (eCapture, false)] {
        let Ok(collection) = (unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }) else { continue };
        let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }
            .ok()
            .and_then(|d| unsafe { d.GetId() }.ok())
            .map(|p| unsafe { p.to_string() }.unwrap_or_default());

        let count = unsafe { collection.GetCount() }.unwrap_or(0);
        for i in 0..count {
            let Ok(device) = (unsafe { collection.Item(i) }) else { continue };
            let Ok(id_pwstr) = (unsafe { device.GetId() }) else { continue };
            let id = unsafe { id_pwstr.to_string() }.unwrap_or_default();
            let name = device_friendly_name(&device).unwrap_or_else(|| "<unknown>".to_string());
            let is_default = default_id.as_deref() == Some(id.as_str());
            out.push(WasapiDeviceInfo {
                id,
                name,
                is_default,
                input_channels: if is_output { 0 } else { 2 },
                output_channels: if is_output { 2 } else { 0 },
            });
        }
    }
    out
}

fn device_friendly_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    let store = unsafe { device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ) }.ok()?;
    let prop = unsafe { store.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _) }.ok()?;
    let pwstr = unsafe { PropVariantToStringAlloc(&prop) }.ok()?;
    unsafe { pwstr.to_string() }.ok()
}
```

**Note for the implementer:** channel counts above are hardcoded to 2
because `IMMDevice` alone doesn't expose channel count without opening an
`IAudioClient` and calling `GetMixFormat` — Task 8's exclusive-mode setup
already does this per-device when a device is actually selected, so this
enumeration function intentionally reports a conservative stereo default
(matches what the current UI needs: a device list to choose from, not a
channel count per unopened device). If the frontend's device picker needs
real channel counts before opening a device, open a throwaway shared-mode
`IAudioClient` here to call `GetMixFormat` — flagged as a follow-up, not
required for this plan's latency goal.

**Mono-device requirement carried into Task 7 (Review Focus item):** a
device that is natively mono-only will reject the hardcoded stereo
`WAVEFORMATEX` Task 7 builds. Before calling `initialize_client`, Task 7
must call `client.GetMixFormat()` once to read the device's native
`nChannels`; if it is `1`, build a mono `WAVEFORMATEX` instead and, in the
capture thread's copy loop, duplicate the single decoded sample to both
`left`/`right` before pushing (`producer.try_push(sample); producer.try_push(sample);`)
— the same duplication `manager.rs`'s current cpal input callback already
does for `input_channels < 2`. This must not be dropped in the rewrite.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p app_lib backend::wasapi --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/audio/backend/wasapi.rs
git commit -m "feat(audio): add WASAPI device enumeration via IMMDeviceEnumerator"
```

---

## Task 7: WASAPI backend — exclusive-mode duplex streams (`backend/wasapi.rs` part 2)

**Files:**
- Modify: `src-tauri/src/audio/backend/wasapi.rs`

**Interfaces:**
- Consumes: `crate::audio::mixer::{MixerState, process_block, main_output_gate_open}`; `crate::audio::mmcss::boost_current_thread_to_pro_audio`; `ringbuf::{HeapRb, HeapProd, HeapCons}`.
- Produces:
  ```rust
  pub struct WasapiCaptureStream { /* opaque; owns thread + IAudioClient, stop on Drop */ }
  pub struct WasapiRenderStream { /* opaque; owns thread + IAudioClient, stop on Drop */ }
  pub struct ExclusiveModeResult { pub exclusive: bool, pub fallback_reason: Option<String> }

  pub fn start_capture(device_id: &str, buffer_size_frames: u32, sample_rate: u32, producer: ringbuf::HeapProd<f32>) -> anyhow::Result<(WasapiCaptureStream, ExclusiveModeResult)>
  pub fn start_render(device_id: &str, buffer_size_frames: u32, sample_rate: u32, consumer: ringbuf::HeapCons<f32>, mixer: MixerState, virt_producer: Option<ringbuf::HeapProd<f32>>) -> anyhow::Result<(WasapiRenderStream, ExclusiveModeResult)>
  ```

- [ ] **Step 1: Write the test** (pure period-alignment math only; the live thread is covered by manual verification in Task 9)

```rust
#[cfg(test)]
mod exclusive_tests {
    use super::*;

    #[test]
    fn aligns_period_up_to_the_next_multiple_when_not_aligned() {
        // REFERENCE_TIME units are 100ns; GetBufferSize reports frames.
        // 47 frames not aligned to a driver's 48-frame block -> round up to 48.
        assert_eq!(align_frames_up(47, 48), 48);
        assert_eq!(align_frames_up(96, 48), 96);
        assert_eq!(align_frames_up(1, 48), 48);
    }

    #[test]
    fn frames_to_reference_time_matches_sample_rate() {
        // 480 frames @ 48kHz = 10ms = 100_000 * 100ns units.
        assert_eq!(frames_to_ref_time(480, 48_000), 100_000);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app_lib backend::wasapi::exclusive_tests --lib`
Expected: FAIL with "cannot find function `align_frames_up`"

- [ ] **Step 3: Implement the alignment helpers, then the capture/render stream setup**

```rust
fn align_frames_up(frames: u32, alignment: u32) -> u32 {
    if alignment == 0 { return frames; }
    frames.div_ceil(alignment) * alignment
}

fn frames_to_ref_time(frames: u32, sample_rate: u32) -> i64 {
    (frames as i64 * 10_000_000) / sample_rate as i64
}
```

```rust
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENT_CALLBACK,
    IAudioCaptureClient, IAudioClient, IAudioRenderClient, WAVEFORMATEX,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
use ringbuf::{HeapProd, HeapCons, traits::{Producer, Consumer}};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::audio::mixer::{MixerState, process_block, main_output_gate_open};

pub struct ExclusiveModeResult {
    pub exclusive: bool,
    pub fallback_reason: Option<String>,
}

fn stereo_wave_format(sample_rate: u32) -> WAVEFORMATEX {
    let bits_per_sample: u16 = 32;
    let channels: u16 = 2;
    let block_align = channels * (bits_per_sample / 8);
    WAVEFORMATEX {
        wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate * block_align as u32,
        nBlockAlign: block_align,
        wBitsPerSample: bits_per_sample,
        cbSize: 0,
    }
}

fn open_audio_client(device_id: &str) -> anyhow::Result<IAudioClient> {
    ensure_com_initialized();
    let enumerator: IAudioClient; // placeholder type inference anchor removed below
    unsafe {
        use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator};
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        use windows::core::PCWSTR;
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
        Ok(device.Activate::<IAudioClient>(CLSCTX_ALL, None)?)
    }
}

/// Initializes the client in exclusive mode, retrying once with the
/// driver-reported aligned buffer size on `AUDCLNT_E_BUFFERSIZE_NOT_ALIGNED`,
/// and falling back to shared mode if exclusive is refused outright.
fn initialize_client(client: &IAudioClient, requested_frames: u32, sample_rate: u32) -> anyhow::Result<ExclusiveModeResult> {
    let format = stereo_wave_format(sample_rate);
    let period = frames_to_ref_time(requested_frames, sample_rate);

    let first = unsafe {
        client.Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENT_CALLBACK, period, period, &format, None)
    };

    match first {
        Ok(()) => Ok(ExclusiveModeResult { exclusive: true, fallback_reason: None }),
        Err(e) if e.code().0 as u32 == 0x88890019 /* AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED */ => {
            let aligned_frames = unsafe { client.GetBufferSize() }.unwrap_or(requested_frames);
            let aligned_period = frames_to_ref_time(aligned_frames, sample_rate);
            let retry = unsafe {
                client.Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENT_CALLBACK, aligned_period, aligned_period, &format, None)
            };
            match retry {
                Ok(()) => Ok(ExclusiveModeResult { exclusive: true, fallback_reason: None }),
                Err(e) => fall_back_to_shared(client, requested_frames, sample_rate, &format, e),
            }
        }
        Err(e) => fall_back_to_shared(client, requested_frames, sample_rate, &format, e),
    }
}

fn fall_back_to_shared(client: &IAudioClient, requested_frames: u32, sample_rate: u32, format: &WAVEFORMATEX, exclusive_err: windows::core::Error) -> anyhow::Result<ExclusiveModeResult> {
    let period = frames_to_ref_time(requested_frames, sample_rate);
    unsafe {
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENT_CALLBACK, period, 0, format, None)
    }.map_err(|e| anyhow::anyhow!("WASAPI shared-mode fallback also failed: {e}"))?;
    Ok(ExclusiveModeResult {
        exclusive: false,
        fallback_reason: Some(format!("Exclusive mode unavailable ({exclusive_err}); using shared mode")),
    })
}

pub struct WasapiCaptureStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WasapiCaptureStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

pub fn start_capture(device_id: &str, buffer_size_frames: u32, sample_rate: u32, mut producer: HeapProd<f32>) -> anyhow::Result<(WasapiCaptureStream, ExclusiveModeResult)> {
    let client = open_audio_client(device_id)?;
    let result = initialize_client(&client, buffer_size_frames, sample_rate)?;
    let event = unsafe { CreateEventW(None, false, false, None) }?;
    unsafe { client.SetEventHandle(event) }?;
    let capture: IAudioCaptureClient = unsafe { client.GetService() }?;
    unsafe { client.Start() }?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop_flag);
    let event_raw = event.0 as isize;

    let thread = std::thread::spawn(move || {
        let event = HANDLE(event_raw as _);
        let mmcss_once = std::sync::Once::new();
        while !thread_stop.load(Ordering::Relaxed) {
            let wait = unsafe { WaitForSingleObject(event, 1000) };
            if wait != WAIT_OBJECT_0 { continue; }
            mmcss_once.call_once(|| { crate::audio::mmcss::boost_current_thread_to_pro_audio(); });

            let Ok(packet_frames) = (unsafe { capture.GetNextPacketSize() }) else { break };
            if packet_frames == 0 { continue; }
            let mut data_ptr = std::ptr::null_mut();
            let mut num_frames = 0u32;
            let mut flags = 0u32;
            if unsafe { capture.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None) }.is_err() {
                break;
            }
            let samples = unsafe { std::slice::from_raw_parts(data_ptr as *const f32, (num_frames * 2) as usize) };
            for pair in samples.chunks_exact(2) {
                let _ = producer.try_push(pair[0]);
                let _ = producer.try_push(pair[1]);
            }
            unsafe { let _ = capture.ReleaseBuffer(num_frames); }
        }
        unsafe { let _ = client.Stop(); }
        unsafe { let _ = CloseHandle(event); }
    });

    Ok((WasapiCaptureStream { stop_flag, thread: Some(thread) }, result))
}

pub struct WasapiRenderStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WasapiRenderStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

pub fn start_render(device_id: &str, buffer_size_frames: u32, sample_rate: u32, mut consumer: HeapCons<f32>, mixer: MixerState, mut virt_producer: Option<HeapProd<f32>>) -> anyhow::Result<(WasapiRenderStream, ExclusiveModeResult)> {
    let client = open_audio_client(device_id)?;
    let result = initialize_client(&client, buffer_size_frames, sample_rate)?;
    let event = unsafe { CreateEventW(None, false, false, None) }?;
    unsafe { client.SetEventHandle(event) }?;
    let render: IAudioRenderClient = unsafe { client.GetService() }?;
    let buffer_frames = unsafe { client.GetBufferSize() }?;
    unsafe { client.Start() }?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop_flag);
    let event_raw = event.0 as isize;
    let output_is_asio = mixer.output_is_asio;
    let mut left_buf = vec![0.0f32; buffer_frames as usize];
    let mut right_buf = vec![0.0f32; buffer_frames as usize];

    let thread = std::thread::spawn(move || {
        let event = HANDLE(event_raw as _);
        let mmcss_once = std::sync::Once::new();
        while !thread_stop.load(Ordering::Relaxed) {
            let wait = unsafe { WaitForSingleObject(event, 1000) };
            if wait != WAIT_OBJECT_0 { continue; }
            mmcss_once.call_once(|| { crate::audio::mmcss::boost_current_thread_to_pro_audio(); });

            let padding = unsafe { client.GetCurrentPadding() }.unwrap_or(0);
            let frames_available = buffer_frames.saturating_sub(padding);
            if frames_available == 0 { continue; }
            let frames = (frames_available as usize).min(left_buf.len());

            for i in 0..frames {
                left_buf[i] = consumer.try_pop().unwrap_or(0.0);
                right_buf[i] = consumer.try_pop().unwrap_or(0.0);
            }

            let mirror = process_block(&mut left_buf[..frames], &mut right_buf[..frames], &mixer, sample_rate as f64);
            let is_muted = mixer.muted.load(Ordering::Relaxed);
            let is_loopback = mixer.loopback_enabled.load(Ordering::Relaxed);
            let gate_open = main_output_gate_open(output_is_asio, is_muted, is_loopback);

            if mirror {
                if let Some(ref mut vp) = virt_producer {
                    for i in 0..frames {
                        let _ = vp.try_push(left_buf[i]);
                        let _ = vp.try_push(right_buf[i]);
                    }
                }
            }

            let Ok(data_ptr) = (unsafe { render.GetBuffer(frames_available) }) else { continue };
            let out = unsafe { std::slice::from_raw_parts_mut(data_ptr as *mut f32, (frames_available * 2) as usize) };
            for i in 0..frames_available as usize {
                let (l, r) = if gate_open && i < frames { (left_buf[i], right_buf[i]) } else { (0.0, 0.0) };
                out[i * 2] = l;
                out[i * 2 + 1] = r;
            }
            unsafe { let _ = render.ReleaseBuffer(frames_available, 0); }
        }
        unsafe { let _ = client.Stop(); }
        unsafe { let _ = CloseHandle(event); }
    });

    Ok((WasapiRenderStream { stop_flag, thread: Some(thread) }, result))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p app_lib backend::wasapi::exclusive_tests --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/audio/backend/wasapi.rs
git commit -m "feat(audio): add WASAPI exclusive-mode capture/render backend with shared-mode fallback"
```

---

## Task 8: Wire backends into `manager.rs` / `device.rs`, remove cpal

**Files:**
- Modify: `src-tauri/src/audio/device.rs` (rewrite `list_devices`, `find_input_device`, `find_output_device`, `find_asio_device_pair` to call `backend::asio::list_asio_devices` / `backend::wasapi::list_wasapi_devices` instead of cpal; device "handles" become plain `String` ids as they are today, no `cpal::Device` type anywhere)
- Modify: `src-tauri/src/audio/manager.rs` (`toggle_monitoring` builds a `MixerState`, then dispatches to `backend::asio::start_duplex` for the same-ASIO-device case, or to `backend::wasapi::start_capture`/`start_render` bridged by a `ringbuf::HeapRb` for every other case, including cross-driver ASIO which reuses two independent `asio::start_duplex`-style single-direction registrations — see note below)
- Modify: `src-tauri/src/audio/types.rs` (add `exclusive_mode_active: bool` and `wasapi_fallback_reason: Option<String>` to `AudioStatus`, defaulted to `false`/`None`)
- Modify: `src-tauri/Cargo.toml` (remove the `cpal` dependency line entirely)
- Test: manual verification (this task has no new pure-logic unit tests of its own — it is integration wiring; Task 2/4/6/7's unit tests already cover the logic being wired together)

**Interfaces:**
- Consumes: everything produced by Tasks 2, 3, 4, 5, 6, 7.
- Produces: no new public interface — `AudioManager`'s existing signatures, now backed by the new backends.

- [ ] **Step 1: Rewrite `device.rs` enumeration**

Replace the cpal-based bodies of `list_devices`, `find_input_device`, `find_output_device`, `find_asio_device_pair` with calls to `backend::asio::list_asio_devices()` and `backend::wasapi::list_wasapi_devices()`, keeping the exact same `AudioDeviceInfo` struct shape and the same `"asio_{name}"`/`"in_{name}"`/`"out_{name}"` id prefixing scheme documented in the current file's doc comments (Section 3 of the spec requires the frontend/commands layer to see no difference). `find_input_device`/`find_output_device` now return the resolved `String` id (already validated against the enumeration list) instead of a `cpal::Device`.

- [ ] **Step 2: Rewrite `manager.rs`'s `toggle_monitoring`**

Keep the existing `same_asio_device` detection logic (manager.rs:192-224) unchanged — it already computes exactly the branch this task needs. Replace the stream-building section (manager.rs:226-648) with:
- `same_asio_device == true`: build one `MixerState`, call `backend::asio::start_duplex(asio_name, in_offset, out_offset, Some(config.buffer_size as i32), mixer, virt_producer)`.
- Otherwise: build a `ringbuf::HeapRb::<f32>::new(buf_capacity)` exactly as today (buf_capacity formula unchanged), split into producer/consumer. For each side (input, output) independently check whether its resolved device id has the `"asio_"` prefix or not, and call the matching single-direction starter:
  - ASIO input only: reuse `backend::asio::start_duplex`'s internals is not applicable (that function is combined duplex) — instead add a second, smaller pair of functions in `backend/asio.rs` for this task, `start_input_only(driver_name, offset, buffer_size_hint, producer)` and `start_output_only(...)`, following the same pattern as `start_duplex` but registering only 2 `AsioBufferInfo` entries for one direction (via `driver.prepare_input_stream`/`prepare_output_stream` called alone, matching the SDK calls already shown in Task 5 minus the paired half).
  - WASAPI side: `backend::wasapi::start_capture` / `start_render` as already implemented.
- Store the returned `ExclusiveModeResult` (WASAPI) or a default "not applicable" result (ASIO) into `self.status` for the two new fields added in Step 3.
- `MonitoringStreams` (manager.rs:16-24) changes from holding `cpal::Stream` fields to holding an enum `enum ActiveBackend { AsioDuplex(backend::asio::AsioDuplexStream), Bridged { input: BridgedInput, output: BridgedOutput } }` where `BridgedInput`/`BridgedOutput` are small enums over `{ Asio(...), Wasapi(...) }` for the two single-direction cases — dropping any variant stops that stream (both new stream types stop in their `Drop` impl, from Tasks 5/7).

- [ ] **Step 3: Add the two additive status fields**

In `types.rs`, add to `AudioStatus`:
```rust
pub exclusive_mode_active: bool,
pub wasapi_fallback_reason: Option<String>,
```
with `Default` impl (or `#[derive(Default)]` if already present) covering them as `false`/`None`. In `manager.rs::get_status`, copy these from whatever the active `MonitoringStreams` last recorded (store them as `Arc<RwLock<...>>` fields on `AudioManager` the same way `status` already is, updated at `toggle_monitoring` time — there is no per-block update needed since this value only changes on stream (re)start).

- [ ] **Step 4: Remove `cpal` from `Cargo.toml`**

Delete the `cpal = { version = "0.18", features = ["asio"] }` line and its preceding comment block about `CPAL_ASIO_DIR` (the new `asio-sys` dependency still needs the same `CPAL_ASIO_DIR` env var at build time — update the comment to reference `asio-sys` instead of cpal, not delete the instruction).

- [ ] **Step 5: Full workspace build check**

Run: `cargo build -p app_lib --release`
Expected: builds with no errors and no remaining references to `cpal::` anywhere (`grep -rn "cpal::" src-tauri/src` should return nothing once this task is done).

- [ ] **Step 6: Run the full existing test suite**

Run: `cargo test -p app_lib --lib`
Expected: all tests from Tasks 1-7 still pass (mixer, mmcss, backend::asio, backend::wasapi).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/audio/device.rs src-tauri/src/audio/manager.rs src-tauri/src/audio/types.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "refactor(audio): replace cpal with native ASIO/WASAPI backends end to end"
```

---

## Task 9: Manual verification pass

**Files:** none (verification only)

- [ ] **Step 1: Launch the app**

Use the `run` skill to build and launch ReLightHost.

- [ ] **Step 2: WASAPI path**

Select the default WASAPI input/output devices, start monitoring. Confirm: audio passes through with no crackle at the smallest configured buffer size that doesn't underrun; `get_status`'s `exclusive_mode_active` is `true` on a device that isn't in use elsewhere; mute silences output instantly; loopback mirrors to the configured virtual output; VU meter moves; CPU/RAM readouts still update.

- [ ] **Step 3: WASAPI fallback path**

While ReLightHost is monitoring a WASAPI device in exclusive mode, open another app that plays audio through the same device (e.g. a browser tab) — confirm the OTHER app's audio is blocked while ReLightHost holds it exclusively (expected, documented trade-off), and that stopping ReLightHost's monitoring immediately frees the device for the other app.

- [ ] **Step 4: A device already in use**

Start a different app playing through a WASAPI device first, then try to start ReLightHost monitoring on that same device — confirm ReLightHost falls back to shared mode (`exclusive_mode_active: false`, `wasapi_fallback_reason` populated) and still starts monitoring rather than failing outright.

- [ ] **Step 5: ASIO path**

With a real ASIO interface or ASIO4ALL installed, select it as both input and output (insert mode), start monitoring. Confirm: audio passes through, `latency_ms` reflects the driver's real buffer size, mute/loopback/VU meter all work identically to the WASAPI checks above.

- [ ] **Step 6: Mono input device**

Select a mono-capable input device (or a device forced to 1-channel mode if available) — confirm both left and right processed channels carry the same (duplicated) signal, matching pre-rewrite behavior (see Task 7's mono-device note).

- [ ] **Step 6b: Two different ASIO drivers bridged (Review Focus item)**

If two distinct ASIO-capable devices are available (e.g. an audio interface plus VoiceMeeter Virtual ASIO), select one as input and the other as output (not the same-device insert case). Confirm audio is still routed through correctly via the ring-buffer bridge added in Task 8 Step 2, not silently dropped — this is the one Review Focus case that cannot be exercised by a unit test since it requires two real ASIO driver instances.

- [ ] **Step 7: Device removal**

While monitoring on a USB audio device (either backend), physically unplug it — confirm ReLightHost logs the error, sets `is_monitoring` to `false`, and does not crash or hang.

- [ ] **Step 8: Regression checks**

Confirm session restore (relaunch the app, previous device/buffer-size selections reload), tray mute/loopback toggles, and a VST3 plugin that has previously triggered the crash-isolation sandbox still function unchanged.

- [ ] **Step 9: Final commit**

If Step 1-8 required any fixes, commit them individually per the normal workflow. Once all checks pass with no outstanding fixes:

```bash
git log --oneline -12
```

Confirm the full task history for this plan is present, then report completion to the user.
