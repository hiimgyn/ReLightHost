# Native Low-Latency Audio Backend (ASIO + WASAPI Exclusive-Mode)

Status: approved design, pending spec review
Author: ReLightHost audio latency optimization project

## 1. Problem & Goals

ReLightHost is a real-time live audio routing/monitoring host (input device -> plugin
chain -> output device), where every millisecond of added latency is audible to the
user monitoring their own voice/instrument. An audit of the current pipeline (cpal
0.18 for both ASIO and WASAPI) found two structural latency ceilings that cannot be
fixed by tuning cpal's existing API:

1. **WASAPI runs shared-mode only.** cpal's WASAPI backend never requests
   `AUDCLNT_SHAREMODE_EXCLUSIVE`, so every WASAPI session goes through the Windows
   audio engine mixer, which imposes a latency floor of roughly 10ms regardless of
   the buffer size configured in the app (which, additionally, is currently not even
   applied to the stream — see Finding A below).
2. **ASIO buffers span more channels than needed.** cpal allocates I/O buffers for
   every channel the device reports (e.g. 32 on a pro interface) even though
   ReLightHost only ever routes one stereo pair, adding avoidable per-callback
   memory traffic and xrun risk at small buffer sizes.

Goals:
- Reach the lowest latency each backend can structurally offer: real WASAPI
  exclusive-mode with a graceful shared-mode fallback, and a leaner ASIO path that
  only touches the channels in use.
- Preserve 100% of existing behavior at the `AudioManager` public API boundary
  (mute, loopback, VU meter, plugin chain, session restore, tray) — no changes
  required in `commands/audio.rs`, Tauri command signatures, or the frontend, except
  two additive status fields (see section 6).
- Fix the two small, independent latency bugs found during audit alongside the
  bigger rewrite, since they touch the same files anyway.

Non-goals (explicitly out of scope for this project; tracked as separate follow-ups):
- Shared-memory ring buffer rewrite of the VST3 sandbox IPC transport. Only the
  fixed 20ms response timeout is corrected here (a one-line, low-risk fix); the
  bigger IPC-transport rewrite is a separate, independent sub-project.
- Plugin Delay Compensation (PDC). Unrelated to reducing round-trip latency — it is
  a phase-alignment correctness feature — and not implemented in this project.
- DirectSound or any other exotic cpal host. The app is Windows-only and targets
  ASIO/WASAPI; DirectSound support is dropped. Reversible later if a real user
  depends on a DirectSound-only device (none known today).

## 2. Audit Findings Being Fixed

| # | File:line (current) | Finding | Fix in this project |
|---|---|---|---|
| A | `audio/manager.rs:261-268` | `build_config` always sets `BufferSize::Default`, so the user's configured buffer size never reaches the WASAPI stream. | Superseded: the new WASAPI backend takes `buffer_size` as the requested exclusive-mode period directly (see 4.2). |
| B | (repo-wide) | No `AvSetMmThreadCharacteristicsW("Pro Audio")` anywhere; audio callback threads run at default priority. | Both new backends call it once per real-time thread (ASIO callback, WASAPI capture thread, WASAPI render thread). |
| C | `plugins/processor/vst3_sandbox/mod.rs:40` | `PROCESS_RESPONSE_TIMEOUT` is a fixed 20ms constant, ~4x the typical 5.3ms block budget the code's own comment cites. | Compute the timeout from `block_size / sample_rate` at sandbox-processor construction time instead of a hardcoded constant. |
| — | `audio/manager.rs:609-621` | Output routing zero-fills every unused device channel per block. | Structurally fixed by only ever allocating the 2 channels in use (ASIO backend, section 4.1); WASAPI already only opens the selected pair via `IAudioClient` device format negotiation. |

## 3. Module Structure

```
src-tauri/src/audio/
  backend/
    mod.rs       // AudioBackend trait, factory that picks asio.rs or wasapi.rs per device id
    asio.rs      // direct asio-sys binding: single duplex bufferSwitch callback
    wasapi.rs    // direct windows-crate binding: exclusive-mode IAudioClient, MMCSS
  mixer.rs        // NEW: extracted per-block stage (mute/loopback/VU/plugin-chain/channel-offset),
                  //      shared verbatim by both backends — replaces the duplicated logic that
                  //      used to live inline in manager.rs's output closure
  device.rs       // rewritten: ASIO enumeration via asio-sys, WASAPI enumeration via IMMDeviceEnumerator
  manager.rs      // same public API; internals call backend/mod.rs instead of cpal
  types.rs        // + 2 additive status fields (section 6)
```

`AudioManager`'s public methods (`start`, `stop`, `toggle_monitoring`, `set_muted`,
`is_muted`, `set_loopback`, `set_output_device`, `set_input_device`,
`set_sample_rate`, `set_buffer_size`, `get_status`, `get_config`, `get_vu_data`, ...)
keep their exact signatures. `commands/audio.rs` and the frontend are untouched.

Dependency changes in `Cargo.toml`:
- Remove `cpal = { version = "0.18", features = ["asio"] }`.
- Add `asio-sys = "0.4"` (the same crate cpal used internally — verified API via
  `RustAudio/cpal` source: `Asio::new`, `Asio::load_driver`, `Driver::channels`,
  `Driver::prepare_input_stream`/`prepare_output_stream`, `Driver::add_callback`,
  `Driver::start`/`stop`).
- Add `windows = "0.62"` with features `Win32_Media_Audio`, `Win32_System_Com`,
  `Win32_Foundation`, `Win32_Media_Multimedia` (for `AvSetMmThreadCharacteristicsW`,
  confirmed present in `windows-sys` already too — reuse the constant/signature but
  call it through `windows` for consistency with the new COM code). Keep the
  existing `windows-sys` dependency untouched for the non-audio Win32 UI code that
  already uses it.
- Keep `ringbuf` (still used for the cross-device bridging case) and `parking_lot`.

## 4. Backend Designs

### 4.1 ASIO backend (`backend/asio.rs`)

Confirmed via `asio-sys` source (`RustAudio/cpal/asio-sys/src/bindings/mod.rs`):
`Driver::add_callback` registers a `FnMut(&CallbackInfo) + Send` closure where
`CallbackInfo { buffer_index, system_time, callback_flag }`; the ASIO SDK's
`bufferSwitch` delivers exactly one such call per period, and **both the input and
output `AsioBufferInfo` buffers created together via `prepare_input_stream` /
`prepare_output_stream` are valid and accessible inside that single call** — this
is a true hardware duplex callback, not two independent streams.

Design:
1. `Asio::new()` once (process-wide singleton, matching the ASIO SDK's own
   one-driver-per-process constraint — same limitation cpal has today).
2. `asio.load_driver(name)` -> `Driver`. Query `driver.channels()`,
   `driver.buffersize_range()`, `driver.sample_rate()`, `driver.input_data_type()`/
   `output_data_type()`.
3. Register **only the 2 input channels and 2 output channels currently selected**
   (`input_channel_offset`/`output_channel_offset` from `AudioConfig`) via
   `AsioBufferInfo { is_input, channel_num, .. }` — not every channel the device
   reports. Changing the channel-offset config rebuilds the buffers (same
   restart-on-config-change behavior as today).
4. `driver.prepare_input_stream(None, 2, Some(config.buffer_size))` /
   `prepare_output_stream` (combined per `create_streams`), then `driver.start()`.
5. `driver.add_callback(move |info| { ... })`:
   - First call: `AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut idx)` once via
     `std::sync::Once`, cached task index for cleanup.
   - Read the 2 input buffers at `info.buffer_index`, convert from the driver's
     native sample type to `f32` into preallocated `left_buf`/`right_buf` (sized to
     `AsioStream.buffer_size`, no per-callback allocation).
   - Call `mixer::process_block(&mut left_buf, &mut right_buf, &shared_state)`
     (section 5).
   - Convert `f32` back to the driver's native output sample type, write into the 2
     output buffers at `info.buffer_index`.
   - **No ring buffer, no cross-thread hop** for this path — this is the same-device
     full-duplex "insert" case that today's code already special-cases with a small
     ring buffer margin; here it becomes truly zero extra latency.
6. Two-different-ASIO-drivers case (rare: input and output selected from two
   distinct physical ASIO devices) and ASIO<->WASAPI mixed routing: bridge with the
   existing `ringbuf::HeapRb` SPSC exactly as today — different clocks need a jitter
   buffer under any backend, native or not.
7. `driver.add_event_callback` surfaces `kAsioResetRequest` /
   `kAsioResyncRequest` / sample-rate-changed events; on any of them, log and
   trigger the same tear-down/rebuild path `toggle_monitoring(false)` +
   `toggle_monitoring(true)` already uses today for config changes.

### 4.2 WASAPI backend (`backend/wasapi.rs`)

Confirmed via the `windows` crate's official docs mirror
(`microsoft.github.io/windows-docs-rs`): `IAudioClient::{Initialize, GetBufferSize,
GetDevicePeriod, GetService, SetEventHandle, Start, Stop, IsFormatSupported,
GetMixFormat}`, `IAudioRenderClient::{GetBuffer, ReleaseBuffer}`,
`IAudioCaptureClient::{GetBuffer, GetNextPacketSize, ReleaseBuffer}`,
`IMMDeviceEnumerator::{EnumAudioEndpoints, GetDefaultAudioEndpoint, GetDevice}` are
all present — the standard Core Audio COM surface, unchanged for 15+ years, exposed
idiomatically by the crate.

Unlike ASIO, **WASAPI has no native duplex callback** — capture and render are
always two independent `IAudioClient` instances/threads even in exclusive mode, so
the existing ring-buffer-bridge architecture is kept for this backend; only the
device I/O underneath it changes.

Design, per direction (capture, render), each its own dedicated OS thread:
1. `CoCreateInstance(&CLSID_MMDeviceEnumerator)` -> `IMMDeviceEnumerator` ->
   `GetDevice(id)` or `GetDefaultAudioEndpoint`.
2. `IAudioClient::Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENT_CALLBACK, period, period, &wave_format, None)`
   where `period` is computed from the configured `buffer_size` (this is where
   Finding A's fix lands: the user's buffer_size setting now genuinely drives the
   requested hardware period).
3. If `Initialize` returns `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`: call
   `GetBufferSize`, recompute the aligned period, `Initialize` again once (the
   standard, documented WASAPI exclusive-mode alignment retry).
4. If exclusive-mode `Initialize` still fails (device doesn't support it, or is
   exclusively held by another app already): **fall back to
   `AUDCLNT_SHAREMODE_SHARED`** with the same requested period as a hint, and record
   the reason (section 6) — monitoring still starts, just at shared-mode latency.
5. `SetEventHandle` with a `CreateEventW` handle; `GetService::<IAudioRenderClient>`
   / `<IAudioCaptureClient>`; `Start()`.
6. Thread loop: `WaitForSingleObject(event, INFINITE)` ->
   `GetBuffer`/`GetNextPacketSize` -> copy samples -> `ReleaseBuffer`. First
   iteration calls `AvSetMmThreadCharacteristicsW(w!("Pro Audio"), ..)` once.
   - Capture thread pushes de-interleaved L/R into the SPSC ring buffer (same shape
     as today's input callback).
   - Render thread pops from the ring buffer and calls the same
     `mixer::process_block` used by the ASIO backend, then writes the result into
     the exclusive-mode buffer.
7. Device invalidated (unplugged, format changed): `IAudioClient` calls return
   `AUDCLNT_E_DEVICE_INVALIDATED` on the next `GetBuffer`/`Start` — caught, logged,
   and surfaced through the existing error path (`toggle_monitoring` stops
   gracefully), matching cpal's current error-callback behavior.

### 5. Shared mixer stage (`audio/mixer.rs`)

Extracted, byte-for-byte equivalent to the current output-closure body in
`manager.rs` (drain-or-passthrough, plugin-chain `try_lock`, VU meter update, mute/
loopback gate resolution, channel-offset re-interleave), as one function:

```rust
pub fn process_block(
    left: &mut [f32],
    right: &mut [f32],
    state: &MixerState, // Arc-cloned handles: process_fn, vu_meter, muted,
                        // loopback_enabled, dsp_load_u32, virt_producer, output_is_asio
)
```

Both backends call exactly this function from their real-time callback/thread —
mute/loopback/VU/plugin-chain behavior is guaranteed identical between ASIO and
WASAPI paths by construction (one implementation, not two kept in sync by hand).
This is also what makes the pure logic unit-testable (section 7) independent of any
real device.

### 6. Config/status additions (`types.rs`)

Two additive fields on `AudioStatus` (existing fields unchanged, no breaking
change):
- `exclusive_mode_active: bool` — true when the current WASAPI session is running
  exclusive-mode; always `false` when on ASIO (the concept doesn't apply) or when a
  WASAPI session fell back to shared mode.
- `wasapi_fallback_reason: Option<String>` — set when exclusive-mode `Initialize`
  failed and shared-mode was used instead (e.g. "Device in use by another
  application"), `None` otherwise.

Frontend: a small, additive UI change (out of scope for the Rust plan besides
exposing these two fields) can show "Exclusive mode active" vs the fallback reason
as a tooltip — not required for this project to be complete, since the fields are
optional/additive and the UI already has a status footer that reads `AudioStatus`.

### 7. Testing Strategy

Real-time audio correctness ultimately requires real hardware/drivers and is
verified manually (see below); automated tests cover every pure-logic piece that
does not require a live device:

- `mixer::process_block` — pure function of buffers + state; one test per gate
  combination (muted, loopback on/off, ASIO vs non-ASIO gate polarity) plus one
  proving mute is glitch-free (no stream restart / no discontinuity beyond the
  expected sample-accurate silence).
- Sample-format conversion helpers (`f32_to_i16/u16/i32` and their ASIO
  `AsioSampleType` counterparts) — round-trip conversion tests at boundary values
  (`-1.0`, `0.0`, `1.0`).
- WASAPI exclusive-mode period alignment math — given a requested `buffer_size` and
  a mock `GetBufferSize`/device period, assert the retried-aligned value is used.
- VST3 sandbox timeout calculation (Finding C) — given block_size/sample_rate,
  assert the computed timeout is greater than the block duration but no longer a
  fixed 20ms regardless of input.

Manual verification pass (required before calling this project done, cannot be
automated): using the project's `run` skill, exercise both backends —
- ASIO (a real interface, or ASIO4ALL as a stand-in): monitoring starts, mute/
  loopback/VU meter work, `latency_ms` reflects the driver's real buffer size, no
  crash on device disconnect.
- WASAPI (default speakers/mic): same checks, plus confirm exclusive-mode engages
  (`exclusive_mode_active` true) on a device that supports it, and that a device
  already in use by another app cleanly falls back to shared mode instead of
  failing to start.
- Regression check: existing session-restore, tray mute/loopback toggles, and the
  VST3 sandbox (crash-isolated plugin) still function unchanged.

## 8. Rollout / Risk

This replaces the entire device I/O layer in one Windows-only app with a single
active user base (no external plugin API depends on `audio/` internals) — the risk
is contained to this module. `AudioManager`'s public surface not changing means the
change is not "big-bang" for the rest of the codebase; it can be built and manually
verified backend-by-backend (ASIO first since its win is smaller/safer to validate,
then WASAPI) before removing the `cpal` dependency entirely in the final commit of
the plan.
