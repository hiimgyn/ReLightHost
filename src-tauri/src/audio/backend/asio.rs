#![cfg(target_os = "windows")]

use std::sync::Once;
use asio_sys::{Asio, CallbackInfo, Driver};
use ringbuf::{HeapProd, traits::Producer};
use crate::audio::mixer::{MixerState, process_block, main_output_gate_open};

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
fn asio_i32_to_f32(v: i32) -> f32 {
    v as f32 / i32::MAX as f32
}

/// Owns a running ASIO driver and its registered duplex callback.
///
/// Opaque to callers: `stop` is the only supported way to tear this down
/// (dropping it without calling `stop` still releases the driver via
/// `Driver`'s own `Drop`, but leaves stream teardown ordering to that
/// impl rather than doing it explicitly).
pub struct AsioDuplexStream {
    driver: Driver,
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
/// Sample-type note: assumes the driver's native format is
/// `ASIOSTInt32LSB` (see `f32_to_asio_i32`/`asio_i32_to_f32`); Task 7
/// checks `driver.input_data_type()`/`output_data_type()` and adds the
/// `ASIOSTFloat32LSB` branch before wiring this into `manager.rs`.
pub fn start_duplex(
    driver_name: &str,
    in_offset: usize,
    out_offset: usize,
    buffer_size_hint: Option<i32>,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<f32>>,
) -> anyhow::Result<AsioDuplexStream> {
    let asio = Asio::new();
    let driver = asio
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

    // Both streams were allocated together by the same `ASIOCreateBuffers`
    // call above, so they share one buffer size.
    let buffer_size = output_stream.buffer_size.max(0) as usize;
    let in_l = in_offset;
    let in_r = in_offset + 1;
    let out_l = out_offset;
    let out_r = out_offset + 1;

    let sample_rate = driver.sample_rate().unwrap_or(48_000.0);
    let mmcss_once = Once::new();
    let mut left_buf = vec![0.0f32; buffer_size];
    let mut right_buf = vec![0.0f32; buffer_size];
    let output_is_asio = mixer.output_is_asio;

    driver.add_callback(move |info: &CallbackInfo| {
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

        // SAFETY: `input_stream.buffer_infos[in_l/in_r].buffers[idx]` was
        // allocated by ASIO's `ASIOCreateBuffers` above for exactly
        // `buffer_size` ASIOSTInt32LSB (i32) samples per half of the
        // double buffer; `idx` is the half ASIO just told us (via
        // `CallbackInfo::buffer_index`) is ready to read, and `frame` is
        // bounds-checked by iterating `left_buf`/`right_buf`, which were
        // sized to `buffer_size`.
        for (frame, sample) in left_buf.iter_mut().enumerate() {
            let ptr = input_stream.buffer_infos[in_l].buffers[idx] as *const i32;
            *sample = unsafe { asio_i32_to_f32(*ptr.add(frame)) };
        }
        for (frame, sample) in right_buf.iter_mut().enumerate() {
            let ptr = input_stream.buffer_infos[in_r].buffers[idx] as *const i32;
            *sample = unsafe { asio_i32_to_f32(*ptr.add(frame)) };
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
            let ptr = output_stream.buffer_infos[out_l].buffers[idx] as *mut i32;
            let value = if gate_open { f32_to_asio_i32(*sample) } else { 0 };
            unsafe {
                *ptr.add(frame) = value;
            }
        }
        for (frame, sample) in right_buf.iter().enumerate() {
            let ptr = output_stream.buffer_infos[out_r].buffers[idx] as *mut i32;
            let value = if gate_open { f32_to_asio_i32(*sample) } else { 0 };
            unsafe {
                *ptr.add(frame) = value;
            }
        }
    });

    driver
        .start()
        .map_err(|e| anyhow::anyhow!("Failed to start ASIO driver: {e}"))?;
    Ok(AsioDuplexStream { driver })
}

/// Stops the stream and releases its ASIO buffers. Errors from the
/// underlying ASIO calls are logged rather than propagated since there is
/// nothing further the caller can do once teardown has already begun.
pub fn stop(stream: AsioDuplexStream) {
    if let Err(e) = stream.driver.stop() {
        log::warn!("ASIO stop() failed: {e}");
    }
    if let Err(e) = stream.driver.dispose_buffers() {
        log::warn!("ASIO dispose_buffers() failed: {e}");
    }
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
