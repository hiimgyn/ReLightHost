#![cfg(target_os = "windows")]

use std::sync::{LazyLock, Once};
use asio_sys::{Asio, AsioSampleType, CallbackInfo, Driver};
use ringbuf::{HeapProd, traits::Producer};
use crate::audio::mixer::{MixerState, process_block, main_output_gate_open};

/// The ASIO SDK only ever allows one loaded driver per process (loading a
/// second driver tears down the first via `removeCurrentDriver()`, which
/// would free a running callback's buffers out from under it). `asio-sys`'s
/// `Asio` type tracks "is a driver currently loaded" per-instance, so a
/// fresh `Asio::new()` per call (as `list_asio_devices` and `start_duplex`
/// each used to do) has no memory of what another call already loaded —
/// this process-wide singleton is what actually enforces the one-driver
/// rule across every caller in this module.
static ASIO: LazyLock<Asio> = LazyLock::new(Asio::new);

/// Serializes the ASIO driver *lifecycle* (load → prepare buffers → start,
/// and stop → dispose buffers) across every caller in this module —
/// `list_asio_devices`, `start_duplex`, and `stop` all hold this for their
/// entire setup/teardown body, not just the `ASIO` singleton's own
/// bookkeeping mutex. This matters because `asio-sys`'s `Driver` drop path
/// (`ASIOExit`/`removeCurrentDriver`) and `create_buffers`'s
/// stop-and-recreate path run real ASIO SDK calls against process-wide C
/// state *after* `asio-sys`'s own internal mutex has already been
/// released, so two of these lifecycle calls interleaving from different
/// threads is a real race in the underlying SDK state, not just a
/// Rust-level one. Never acquired from, or held across, the real-time
/// `bufferSwitch` callback itself — only around the ordinary-thread setup/
/// teardown calls.
static ASIO_LIFECYCLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    // Held for the whole enumeration loop (not per-iteration): each
    // iteration's `Driver` handle drops at the end of that iteration,
    // which can run real ASIO teardown calls (see `ASIO_LIFECYCLE_LOCK`'s
    // doc comment) — a concurrent `start_duplex`/`stop` must not observe
    // "nothing loaded" mid-teardown.
    let _lifecycle_guard = ASIO_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let asio = &*ASIO;
    let mut out = Vec::new();
    for name in asio.driver_names() {
        // `load_driver` returns the already-loaded driver directly when its
        // name matches (see asio-sys's own `load_driver`), so the only way
        // this errors is a DIFFERENT driver currently being loaded (e.g. by
        // a live `start_duplex` stream elsewhere in the process) — ASIO
        // only ever allows one loaded driver, so that entry genuinely can't
        // be queried right now. Fall back to `loaded_driver()` once, in
        // case it's actually this same name (defensive; `load_driver`
        // already handles the common case itself).
        let driver = match asio.load_driver(&name) {
            Ok(d) => d,
            Err(_) => match asio.loaded_driver() {
                Some(d) if d.name() == name => d,
                _ => continue,
            },
        };
        let Ok(channels) = driver.channels() else { continue };
        out.push(AsioDeviceInfo {
            name,
            input_channels: channels.ins.max(0) as usize,
            output_channels: channels.outs.max(0) as usize,
        });
    }
    out
}

/// Converts a mixer sample in `[-1.0, 1.0]` to a 32-bit signed integer
/// sample in ASIO's `ASIOSTInt32LSB` format.
///
/// NOTE (scope): this hardcodes the `ASIOSTInt32LSB` conversion, the most
/// common native format for consumer/prosumer ASIO drivers. It does NOT
/// handle other sample types (e.g. `ASIOSTFloat32LSB`) — checking
/// `driver.input_data_type()`/`output_data_type()` and branching on the
/// result is explicitly deferred to Task 7, which wires this function into
/// `manager.rs`. Matches the existing `f32_to_i16` clamp pattern in
/// `manager.rs`.
fn f32_to_asio_i32(v: f32) -> i32 {
    // Scales in `f64`, not `f32`: `i32::MAX` (2147483647) is not exactly
    // representable in `f32` (24-bit mantissa vs. 31 bits needed), so it
    // rounds up to 2147483648.0 there. That's harmless at `+1.0` (the
    // saturating float->int cast clamps back down to `i32::MAX`), but at
    // `-1.0` it produces the exact, in-range value `i32::MIN` instead of
    // the intended symmetric `i32::MIN + 1` (matching the existing
    // `f32_to_i16` clamp-symmetry pattern, where `i16::MAX` fits `f32`
    // exactly and this rounding issue doesn't arise). `f64` has enough
    // mantissa bits (52) to hold `i32::MAX` exactly, avoiding it here.
    (v.max(-1.0).min(1.0) as f64 * i32::MAX as f64) as i32
}

/// Inverse of [`f32_to_asio_i32`]. See its doc comment for the sample-type
/// assumption.
///
/// Unlike `f32_to_asio_i32`, this direction doesn't need the `f64`
/// workaround: there's no clamp/saturation step, and dividing by the same
/// (rounded) `i32::MAX as f32` value used for the boundary case cancels
/// out exactly at `i32::MAX`/`i32::MIN`, so both ends of the range still
/// round-trip to `1.0`/`-1.0` within float epsilon.
fn asio_i32_to_f32(v: i32) -> f32 {
    v as f32 / i32::MAX as f32
}

/// Owns a running ASIO driver and its registered duplex callback.
///
/// Callers MUST call `stop()` on this rather than letting it drop
/// implicitly. `stop()` tears this down under `ASIO_LIFECYCLE_LOCK` (see
/// that static's doc comment) so its teardown can't race a concurrent
/// `list_asio_devices`/`start_duplex` call at the ASIO SDK level; an
/// implicit drop (e.g. this value going out of scope, or being dropped as
/// part of a larger struct/`Vec`) runs the same underlying `Driver`
/// teardown (`ASIOStop`/`ASIODisposeBuffers`/`ASIOExit` via `Driver`'s own
/// `Drop`) but WITHOUT that lock held, reopening the exact race
/// `ASIO_LIFECYCLE_LOCK` exists to close. There is intentionally no custom
/// `Drop` impl here to guard against this automatically: doing so safely
/// would need `driver`/`callback_id` wrapped in `Option` so `Drop::drop`
/// (which only gets `&mut self`, not an owned `self`) could force the
/// `Driver`'s own drop to run inside its own lock-guarded scope instead of
/// via the compiler's field-drop glue afterward — adding that indirection
/// throughout this file was judged a bigger source of risk on unsafe
/// real-time FFI code than documenting the one correct call site.
pub struct AsioDuplexStream {
    driver: Driver,
    callback_id: asio_sys::BufferCallbackId,
}

/// Starts a full-duplex ASIO stream on a single driver: reads the stereo
/// input pair at `in_offset`/`in_offset + 1`, runs it through the shared
/// mixer stage, and writes the result to the stereo output pair at
/// `out_offset`/`out_offset + 1`. Both directions are served from one
/// synchronous `bufferSwitch` callback, since ASIO delivers input and
/// output together for a single driver instance.
///
/// `in_offset`/`out_offset`: 0-based index of the first of the selected
/// stereo pair, clamped by the caller the same way `manager.rs` does
/// today.
///
/// Cross-driver bridging (two different ASIO drivers as input/output) is
/// NOT handled here — that case is composed at the `manager.rs` level in
/// Task 7 by running two independent single-direction registrations
/// bridged through the existing `ringbuf::HeapRb`. This function only
/// covers the single-driver full-duplex case.
///
/// Sample-type note: this function REFUSES to start (returns `Err`) unless
/// the driver's native format is `ASIOSTInt32LSB` (see
/// `f32_to_asio_i32`/`asio_i32_to_f32`) — the fixed-width `i32` pointer
/// arithmetic in the callback below would silently read/write out of
/// bounds against a driver using a different sample width (e.g. 2-byte
/// `ASIOSTInt16LSB` or 3-byte `ASIOSTInt24LSB`). Task 7 adds the
/// `ASIOSTFloat32LSB` branch and relaxes this guard accordingly before
/// wiring this into `manager.rs`.
pub fn start_duplex(
    driver_name: &str,
    in_offset: usize,
    out_offset: usize,
    buffer_size_hint: Option<i32>,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<f32>>,
) -> anyhow::Result<AsioDuplexStream> {
    // Held for this whole setup path (through `driver.start()` below, on
    // every return path) — see `ASIO_LIFECYCLE_LOCK`'s doc comment. Not
    // held for the lifetime of the running stream: once this function
    // returns, the callback runs on ASIO's own thread without touching
    // driver-lifecycle state, so nothing further needs serializing here
    // until `stop()` is called.
    let _lifecycle_guard = ASIO_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // `asio-sys` only ever supports one loaded driver per process. Loading
    // (or re-preparing buffers on) a second one while a driver is already
    // active would tear down the active driver's buffers via
    // `ASIODisposeBuffers`/`create_buffers`'s stop-and-recreate path while
    // its callback closure is still registered and holding pointers into
    // them — a real use-after-free the moment ASIO calls both callbacks.
    // Callers must `stop()` an existing stream before starting another.
    if ASIO.loaded_driver().is_some() {
        return Err(anyhow::anyhow!(
            "an ASIO driver is already active; call stop() before starting a new stream"
        ));
    }

    let driver = ASIO
        .load_driver(driver_name)
        .map_err(|e| anyhow::anyhow!("Failed to load ASIO driver '{driver_name}': {e}"))?;

    // asio-sys's `prepare_input_stream`/`prepare_output_stream` always
    // allocate buffers starting at channel 0 (its internal
    // `prepare_buffer_infos` helper is private and not offset-aware) —
    // there is no public API to request an arbitrary starting channel.
    // To land on the selected `in_offset`/`out_offset` pair we request
    // enough channels to cover the offset, then index into the returned
    // `buffer_infos` at the offset position (`buffer_infos[i]` always
    // corresponds to hardware channel `i`, in order).
    let in_channels = in_offset + 2;
    let out_channels = out_offset + 2;

    // Two-step construction: prepare the input stream first, then hand its
    // `AsioStream` (not a fresh `None`) into `prepare_output_stream` so
    // asio-sys creates both directions' buffers together in one
    // `ASIOCreateBuffers` call instead of the second call silently
    // discarding the first's buffers.
    let input_stream = driver
        .prepare_input_stream(None, in_channels, buffer_size_hint)
        .map_err(|e| anyhow::anyhow!("Failed to prepare ASIO input buffers: {e}"))?
        .input;
    let combined = driver
        .prepare_output_stream(input_stream, out_channels, buffer_size_hint)
        .map_err(|e| anyhow::anyhow!("Failed to prepare ASIO output buffers: {e}"))?;
    let input_stream = combined
        .input
        .ok_or_else(|| anyhow::anyhow!("ASIO driver returned no input stream"))?;
    let output_stream = combined
        .output
        .ok_or_else(|| anyhow::anyhow!("ASIO driver returned no output stream"))?;

    // Refuse to start against a driver reporting a sample format other than
    // the one this function's pointer arithmetic assumes: a narrower format
    // (e.g. 2-byte ASIOSTInt16LSB) would make every `*const/*mut i32` access
    // below read/write past the end of its real per-sample width.
    let input_type = driver
        .input_data_type()
        .map_err(|e| anyhow::anyhow!("Failed to query ASIO input sample type: {e}"))?;
    let output_type = driver
        .output_data_type()
        .map_err(|e| anyhow::anyhow!("Failed to query ASIO output sample type: {e}"))?;
    if !matches!(input_type, AsioSampleType::ASIOSTInt32LSB)
        || !matches!(output_type, AsioSampleType::ASIOSTInt32LSB)
    {
        return Err(anyhow::anyhow!(
            "ASIO driver '{driver_name}' reports unsupported sample format \
             (input: {input_type:?}, output: {output_type:?}); only \
             ASIOSTInt32LSB is currently supported"
        ));
    }

    // Both streams were allocated together by the same `ASIOCreateBuffers`
    // call above, so they share one buffer size.
    let buffer_size = output_stream.buffer_size.max(0) as usize;
    let in_l = in_offset;
    let in_r = in_offset + 1;
    let out_l = out_offset;
    let out_r = out_offset + 1;

    // Zero every output channel — including the selected pair, not only
    // the unused ones — before starting. ASIO buffers are not guaranteed
    // to start zeroed, some drivers begin outputting before the first
    // `bufferSwitch` callback has a chance to fill anything, and the
    // callback below only ever writes `buffer_infos[out_l]`/`[out_r]`
    // (leaving every other channel permanently unfilled). Either way,
    // whatever memory `ASIOCreateBuffers` handed back could otherwise play
    // out as full-scale noise. This runs once here at setup time, before
    // `driver.start()` — doing a plain loop like this from inside the
    // callback itself would not be fine (real-time thread), but here it's
    // a one-time setup cost.
    for info in output_stream.buffer_infos.iter() {
        // Copy the field out by value first: `AsioBufferInfo` is
        // `#[repr(C, packed(4))]`, so `&info.buffers` (which `.iter()`
        // would need) is an unaligned reference and rejected outright
        // (E0793) — a plain value copy sidesteps that.
        let buffers = info.buffers;
        for half in buffers {
            if half.is_null() {
                continue;
            }
            // SAFETY: `half` was allocated by the `ASIOCreateBuffers` call
            // above for exactly `buffer_size` ASIOSTInt32LSB (i32) samples
            // (guarded by the format check above); this runs once during
            // setup, before `driver.start()`, so there is no concurrent
            // callback access to race with.
            unsafe {
                std::ptr::write_bytes(half as *mut i32, 0, buffer_size);
            }
        }
    }

    let sample_rate = driver.sample_rate().unwrap_or_else(|e| {
        log::warn!("ASIO sample_rate() query failed for '{driver_name}', defaulting to 48kHz: {e}");
        48_000.0
    });
    let mmcss_once = Once::new();
    let mut left_buf = vec![0.0f32; buffer_size];
    let mut right_buf = vec![0.0f32; buffer_size];
    let output_is_asio = mixer.output_is_asio;

    let callback_id = driver.add_callback(move |info: &CallbackInfo| {
        // Force capture of the whole `AsioStream` (which asio-sys marks
        // `unsafe impl Send`), not just its `buffer_infos: Vec<AsioBufferInfo>`
        // field — Rust 2021's disjoint closure capture would otherwise
        // capture that field alone, and a bare `Vec<AsioBufferInfo>` is
        // NOT `Send` (its raw `*mut c_void` buffer pointers aren't), which
        // fails `Driver::add_callback`'s `F: Send` bound.
        let input_stream = &input_stream;
        let output_stream = &output_stream;

        mmcss_once.call_once(|| {
            crate::audio::mmcss::boost_current_thread_to_pro_audio();
        });

        let idx = info.buffer_index as usize;

        // Resolved once per channel per callback (not once per frame — the
        // channel/half a frame belongs to doesn't change within one
        // callback invocation).
        let in_l_ptr = input_stream.buffer_infos[in_l].buffers[idx] as *const i32;
        let in_r_ptr = input_stream.buffer_infos[in_r].buffers[idx] as *const i32;
        let out_l_ptr = output_stream.buffer_infos[out_l].buffers[idx] as *mut i32;
        let out_r_ptr = output_stream.buffer_infos[out_r].buffers[idx] as *mut i32;

        // SAFETY: `in_l_ptr`/`in_r_ptr` point into buffers allocated by
        // ASIO's `ASIOCreateBuffers` above for exactly `buffer_size`
        // ASIOSTInt32LSB (i32) samples per half of the double buffer
        // (guarded by the format check in `start_duplex`); `idx` is the
        // half ASIO just told us (via `CallbackInfo::buffer_index`) is
        // ready to read, and `frame` is bounds-checked by iterating
        // `left_buf`/`right_buf`, which were sized to `buffer_size`.
        for (frame, sample) in left_buf.iter_mut().enumerate() {
            *sample = unsafe { asio_i32_to_f32(*in_l_ptr.add(frame)) };
        }
        for (frame, sample) in right_buf.iter_mut().enumerate() {
            *sample = unsafe { asio_i32_to_f32(*in_r_ptr.add(frame)) };
        }

        let result = process_block(&mut left_buf, &mut right_buf, &mixer, sample_rate);
        let gate_open = main_output_gate_open(output_is_asio, result.is_muted, result.is_loopback);

        if result.mirror_to_virtual {
            if let Some(ref mut vp) = virt_producer {
                for frame in 0..left_buf.len() {
                    let _ = vp.try_push(left_buf[frame]);
                    let _ = vp.try_push(right_buf[frame]);
                }
            }
        }

        // SAFETY: same buffer-ownership/index reasoning as the input read
        // above, but writing; ASIO guarantees exclusive access to buffer
        // half `idx` for the duration of this callback.
        for (frame, sample) in left_buf.iter().enumerate() {
            let value = if gate_open { f32_to_asio_i32(*sample) } else { 0 };
            unsafe {
                *out_l_ptr.add(frame) = value;
            }
        }
        for (frame, sample) in right_buf.iter().enumerate() {
            let value = if gate_open { f32_to_asio_i32(*sample) } else { 0 };
            unsafe {
                *out_r_ptr.add(frame) = value;
            }
        }
    });

    if let Err(e) = driver.start() {
        // Don't leave a stale callback registered on a failed start — it
        // would otherwise sit in asio-sys's global callback list holding
        // pointers into these buffers indefinitely.
        driver.remove_callback(callback_id);
        return Err(anyhow::anyhow!("Failed to start ASIO driver: {e}"));
    }
    Ok(AsioDuplexStream { driver, callback_id })
}

/// Stops the stream and releases its ASIO buffers. Errors from the
/// underlying ASIO calls are logged rather than propagated since there is
/// nothing further the caller can do once teardown has already begun.
pub fn stop(stream: AsioDuplexStream) {
    // See `ASIO_LIFECYCLE_LOCK`'s doc comment — held for this whole
    // teardown body.
    let _lifecycle_guard = ASIO_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Stop the driver first, then unregister the callback, then dispose
    // buffers: stopping before removing avoids a brief window where the
    // driver keeps running with no callback filling its output (the
    // reverse order it used to be in), and disposing buffers last means
    // nothing can still be mid-callback against them.
    if let Err(e) = stream.driver.stop() {
        log::warn!("ASIO stop() failed: {e}");
    }
    stream.driver.remove_callback(stream.callback_id);
    if let Err(e) = stream.driver.dispose_buffers() {
        log::warn!("ASIO dispose_buffers() failed: {e}");
    }

    // Explicit, load-bearing: Rust drops a function's body locals BEFORE
    // its parameters, so without this, `stream` (a parameter) would drop
    // — running `Driver`'s own teardown (`ASIOExit`/`removeCurrentDriver`)
    // — AFTER `_lifecycle_guard` (a body local) has already released the
    // lock above. That would reopen the exact race `ASIO_LIFECYCLE_LOCK`
    // exists to close, just moved here instead of `list_asio_devices`.
    // Consuming `stream` here, while `_lifecycle_guard` is still in scope,
    // forces that teardown to happen before the lock is released. Do not
    // remove this as "dead code" — it changes drop order, not behavior
    // visible from reading the calls above.
    drop(stream);
}

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
