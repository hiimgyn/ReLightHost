#![cfg(target_os = "windows")]

use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, DEVICE_STATE_ACTIVE, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioCaptureClient, IAudioClient, IAudioRenderClient,
    WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::PCWSTR;
use ringbuf::{HeapProd, HeapCons, traits::{Producer, Consumer}};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
use crate::audio::mixer::{MixerState, process_block, main_output_gate_open};

/// Decodes a COM-allocated wide string and frees the CoTaskMem allocation
/// the caller owns — `IMMDevice::GetId` and `PropVariantToStringAlloc` both
/// hand back a `PWSTR` the caller must release, and `windows-rs`'s `PWSTR`
/// has no `Drop` impl that does this automatically.
fn decode_and_free_pwstr(p: windows::core::PWSTR) -> Option<String> {
    let s = unsafe { p.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

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
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Enumerates active WASAPI render (output) and capture (input) endpoints
/// via `IMMDeviceEnumerator`. Enumeration only — no stream is opened, so
/// channel counts are a conservative stereo default (see note below).
pub fn list_wasapi_devices() -> Vec<WasapiDeviceInfo> {
    ensure_com_initialized();
    let mut out = Vec::new();
    let enumerator: windows::core::Result<IMMDeviceEnumerator> =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) };
    let Ok(enumerator) = enumerator else {
        return out;
    };

    for (flow, is_output) in [(eRender, true), (eCapture, false)] {
        let Ok(collection) = (unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) })
        else {
            continue;
        };
        let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }
            .ok()
            .and_then(|d| unsafe { d.GetId() }.ok())
            .and_then(decode_and_free_pwstr);

        let count = unsafe { collection.GetCount() }.unwrap_or(0);
        for i in 0..count {
            let Ok(device) = (unsafe { collection.Item(i) }) else {
                continue;
            };
            let Ok(id_pwstr) = (unsafe { device.GetId() }) else {
                continue;
            };
            let Some(id) = decode_and_free_pwstr(id_pwstr) else {
                continue;
            };
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
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
    let store = unsafe { device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ) }.ok()?;
    // GetValue's PROPVARIANT owns COM-allocated memory (e.g. the string
    // buffer behind its VT_LPWSTR variant) that must be released via
    // PropVariantClear regardless of what happens below — done via the
    // `mut` binding freed at the end of this function, not by any Drop impl.
    let mut prop = unsafe { store.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _) }.ok()?;
    let name = unsafe { PropVariantToStringAlloc(&prop) }
        .ok()
        .and_then(decode_and_free_pwstr);
    unsafe {
        let _ = PropVariantClear(&mut prop);
    }
    name
}

#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn list_wasapi_devices_returns_at_least_the_default_render_device() {
        // Every Windows dev/CI machine has at least a default render
        // endpoint (even if it's a dummy/HDMI one) — this is a real,
        // no-mock check that CoInitializeEx + enumeration succeed.
        let devices = list_wasapi_devices();
        assert!(
            devices.iter().any(|d| d.output_channels > 0),
            "expected at least one render endpoint"
        );
    }
}

// ---------------------------------------------------------------------
// Exclusive-mode duplex streams
// ---------------------------------------------------------------------

/// Rounds `frames` up to the next multiple of `alignment`. WASAPI exclusive
/// mode requires the buffer duration passed to `Initialize` to correspond
/// to a whole number of the driver's internal packet size in some cases;
/// when it doesn't, `Initialize` fails with `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`
/// and reports the actual required frame count via `GetBufferSize`, which
/// this helper is used to round up to.
fn align_frames_up(frames: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return frames;
    }
    frames.div_ceil(alignment) * alignment
}

/// Converts a frame count at `sample_rate` to 100ns `REFERENCE_TIME` units,
/// the unit `IAudioClient::Initialize`'s buffer-duration parameters use.
fn frames_to_ref_time(frames: u32, sample_rate: u32) -> i64 {
    (frames as i64 * 10_000_000) / sample_rate as i64
}

pub struct ExclusiveModeResult {
    pub exclusive: bool,
    pub fallback_reason: Option<String>,
}

/// Wraps a COM interface so it can be moved into this module's dedicated
/// real-time capture/render thread. `windows-core` does not implement
/// `Send` for COM interface types in general (COM interfaces can require
/// apartment marshaling), but `IAudioClient`/`IAudioCaptureClient`/
/// `IAudioRenderClient` are documented by WASAPI as safe to call from any
/// thread once obtained, provided that thread is itself part of a COM
/// apartment — the real-time threads below call `ensure_com_initialized()`
/// (`COINIT_MULTITHREADED`, the same call this module's setup path already
/// uses) as their first action, so every thread that touches one of these
/// objects is in the same process-wide multi-threaded apartment and no
/// marshaling is required moving between them. This is the same shape of
/// unsafe assertion this module already makes for the event `HANDLE`
/// (moved across as a plain `isize`) — just applied to the COM objects
/// themselves instead of a raw handle.
struct SendComPtr<T>(T);
// SAFETY: see the doc comment above — every thread that dereferences the
// wrapped value has itself joined the same MTA before doing so.
unsafe impl<T> Send for SendComPtr<T> {}

impl<T> std::ops::Deref for SendComPtr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Builds a 32-bit float `WAVEFORMATEX` for the given channel count.
fn wave_format_for_channels(sample_rate: u32, channels: u16) -> WAVEFORMATEX {
    let bits_per_sample: u16 = 32;
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

/// Queries the device's native mix format channel count via
/// `IAudioClient::GetMixFormat`, so exclusive-mode format negotiation can
/// build a format the device will actually accept. A device that is
/// natively mono-only (e.g. many cheap USB headsets/mics) REJECTS a
/// hardcoded stereo `WAVEFORMATEX` in exclusive mode. Only the mono case is
/// special-cased here — anything else (2, 6, 8 channels, ...) defaults to
/// stereo — matching the old cpal-based code's `input_channels < 2` check
/// in `manager.rs`. Defaults to stereo (2) if the query itself fails.
fn query_native_channels(client: &IAudioClient) -> u16 {
    // SAFETY: `GetMixFormat` is a valid `IAudioClient` method callable
    // before `Initialize`; on success it hands back a `CoTaskMemAlloc`'d
    // `WAVEFORMATEX*` that we read once (bounded to the one `nChannels`
    // field) then free via `CoTaskMemFree` below, mirroring the
    // `decode_and_free_pwstr` ownership pattern already used in this file.
    match unsafe { client.GetMixFormat() } {
        Ok(fmt_ptr) if !fmt_ptr.is_null() => {
            // SAFETY: `fmt_ptr` is non-null and was just allocated by
            // `GetMixFormat` as a valid `WAVEFORMATEX`; no other thread has
            // a reference to it yet.
            let channels = unsafe { (*fmt_ptr).nChannels };
            unsafe { CoTaskMemFree(Some(fmt_ptr as *const _)) };
            if channels == 1 { 1 } else { 2 }
        }
        _ => 2,
    }
}

fn open_audio_client(device_id: &str) -> anyhow::Result<IAudioClient> {
    ensure_com_initialized();
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
        Ok(device.Activate::<IAudioClient>(CLSCTX_ALL, None)?)
    }
}

/// Initializes the client in exclusive mode, retrying once with the
/// driver-reported aligned buffer size on `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`,
/// and falling back to shared mode if exclusive is refused outright.
fn initialize_client(
    client: &IAudioClient,
    requested_frames: u32,
    sample_rate: u32,
    channels: u16,
) -> anyhow::Result<ExclusiveModeResult> {
    let format = wave_format_for_channels(sample_rate, channels);
    let period = frames_to_ref_time(requested_frames, sample_rate);

    // SAFETY: `format` is a validly-constructed `WAVEFORMATEX`; `client`
    // has not been initialized yet on this path.
    let first = unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_EXCLUSIVE,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            period,
            period,
            &format,
            None,
        )
    };

    match first {
        Ok(()) => Ok(ExclusiveModeResult { exclusive: true, fallback_reason: None }),
        Err(e) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            // SAFETY: `client` is a valid, freshly-failed-`Initialize`d
            // client; `GetBufferSize` is documented as valid to call after
            // a buffer-alignment failure to learn the required size.
            // `GetBufferSize` reports the driver's own already-aligned
            // frame count in response to the alignment failure above (per
            // WASAPI's documented retry protocol) — no further rounding via
            // `align_frames_up` is needed against it.
            let aligned_frames = unsafe { client.GetBufferSize() }.unwrap_or(requested_frames);
            let aligned_period = frames_to_ref_time(aligned_frames, sample_rate);
            let retry = unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    aligned_period,
                    aligned_period,
                    &format,
                    None,
                )
            };
            match retry {
                Ok(()) => Ok(ExclusiveModeResult { exclusive: true, fallback_reason: None }),
                Err(e) => fall_back_to_shared(client, requested_frames, sample_rate, &format, e),
            }
        }
        Err(e) => fall_back_to_shared(client, requested_frames, sample_rate, &format, e),
    }
}

fn fall_back_to_shared(
    client: &IAudioClient,
    requested_frames: u32,
    sample_rate: u32,
    format: &WAVEFORMATEX,
    exclusive_err: windows::core::Error,
) -> anyhow::Result<ExclusiveModeResult> {
    let period = frames_to_ref_time(requested_frames, sample_rate);
    // SAFETY: `format` is a validly-constructed `WAVEFORMATEX`; shared mode
    // requires `hnsperiodicity` of 0 (the engine picks its own period).
    unsafe {
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, period, 0, format, None)
    }
    .map_err(|e| anyhow::anyhow!("WASAPI shared-mode fallback also failed: {e}"))?;
    Ok(ExclusiveModeResult {
        exclusive: false,
        fallback_reason: Some(format!("Exclusive mode unavailable ({exclusive_err}); using shared mode")),
    })
}

/// Owns a running WASAPI exclusive/shared-mode capture stream's real-time
/// thread and its `IAudioClient`. Signals the thread to stop and joins it
/// on drop; the thread itself stops the client and closes its event handle.
pub struct WasapiCaptureStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WasapiCaptureStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts a dedicated real-time capture thread against `device_id`,
/// pushing de-interleaved stereo `f32` samples into `producer`. Negotiates
/// exclusive mode first (falling back to shared mode — see
/// [`ExclusiveModeResult`]), and handles mono-only devices by duplicating
/// the single decoded sample to both L/R, matching the old cpal-based
/// `input_channels < 2` handling in `manager.rs`.
pub fn start_capture(
    device_id: &str,
    buffer_size_frames: u32,
    sample_rate: u32,
    mut producer: HeapProd<f32>,
) -> anyhow::Result<(WasapiCaptureStream, ExclusiveModeResult)> {
    let client = open_audio_client(device_id)?;
    let channels = query_native_channels(&client);
    let result = initialize_client(&client, buffer_size_frames, sample_rate, channels)?;
    // SAFETY: `CreateEventW` with all-`None`/`false` arguments creates an
    // anonymous, auto-reset-off, initially-unsignaled event; a valid
    // pattern for WASAPI's event-driven mode.
    let event = unsafe { CreateEventW(None, false, false, None) }?;
    // SAFETY: `event` was just created above and is a valid event handle;
    // `client` has been `Initialize`d (exclusive or shared) above.
    unsafe { client.SetEventHandle(event) }?;
    // SAFETY: requesting `IAudioCaptureClient` from an initialized capture
    // client is the documented way to obtain it.
    let capture: IAudioCaptureClient = unsafe { client.GetService() }?;
    // SAFETY: `client` is fully initialized and has a service + event
    // handle registered.
    unsafe { client.Start() }?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop_flag);
    let event_raw = event.0 as isize;
    // See `SendComPtr`'s doc comment: these COM objects are moved into the
    // dedicated real-time thread below, which joins the same MTA via
    // `ensure_com_initialized()` as its first action.
    let client = SendComPtr(client);
    let capture = SendComPtr(capture);

    let thread = std::thread::spawn(move || {
        ensure_com_initialized();
        // `client`/`capture` are used here only through `SendComPtr`'s
        // `Deref` (never a direct `.0` projection), so Rust 2021's
        // disjoint closure capture moves the whole wrapper into this
        // closure rather than just its inner field (which would recreate
        // the original `Send` error, since the field itself isn't `Send`)
        // — the same pitfall `asio.rs`'s `add_callback` closure documents
        // for its own buffer-info fields.
        let client = client;
        let capture = capture;
        // `HANDLE` wraps a raw `*mut c_void`, which is not `Send`, so the
        // handle crosses the thread boundary as a plain `isize` and is
        // reconstructed here.
        let event = HANDLE(event_raw as _);
        let mmcss_once = Once::new();
        while !thread_stop.load(Ordering::Relaxed) {
            // SAFETY: `event` is a valid, still-open event handle for the
            // lifetime of this loop (closed only after the loop exits,
            // below).
            let wait = unsafe { WaitForSingleObject(event, 1000) };
            if wait != WAIT_OBJECT_0 {
                continue;
            }
            mmcss_once.call_once(|| {
                crate::audio::mmcss::boost_current_thread_to_pro_audio();
            });

            // SAFETY: `capture` is a valid, started `IAudioCaptureClient`.
            let Ok(packet_frames) = (unsafe { capture.GetNextPacketSize() }) else {
                break;
            };
            if packet_frames == 0 {
                continue;
            }
            let mut data_ptr = std::ptr::null_mut();
            let mut num_frames = 0u32;
            let mut flags = 0u32;
            // SAFETY: `capture` is valid and started; `data_ptr`/
            // `num_frames`/`flags` are valid out-pointers for this call.
            if unsafe { capture.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None) }.is_err() {
                break;
            }
            // SAFETY: `data_ptr` was just returned by `GetBuffer` above as
            // pointing to exactly `num_frames * channels` valid `f32`
            // samples (the device was initialized with a `channels`-wide
            // `WAVEFORMATEX` of 32-bit float samples); the slice does not
            // outlive this iteration, and `ReleaseBuffer` below is called
            // before the next `GetBuffer`.
            let samples = unsafe {
                std::slice::from_raw_parts(data_ptr as *const f32, (num_frames * channels as u32) as usize)
            };
            if channels == 1 {
                // Mono device: duplicate the single decoded sample to both
                // L/R (matches the old cpal `input_channels < 2` handling).
                for &s in samples.iter() {
                    let _ = producer.try_push(s);
                    let _ = producer.try_push(s);
                }
            } else {
                for pair in samples.chunks_exact(2) {
                    let _ = producer.try_push(pair[0]);
                    let _ = producer.try_push(pair[1]);
                }
            }
            // SAFETY: releases exactly the `num_frames` `GetBuffer` handed
            // out above, as required before the next `GetBuffer` call.
            unsafe {
                let _ = capture.ReleaseBuffer(num_frames);
            }
        }
        // SAFETY: `client` was successfully `Start()`ed above; stopping an
        // already-stopped client is also documented as returning `Ok`.
        unsafe {
            let _ = client.Stop();
        }
        // SAFETY: `event` is a valid handle created by this function's
        // `CreateEventW` call above and not used again after this point.
        unsafe {
            let _ = CloseHandle(event);
        }
    });

    Ok((WasapiCaptureStream { stop_flag, thread: Some(thread) }, result))
}

/// Owns a running WASAPI exclusive/shared-mode render stream's real-time
/// thread and its `IAudioClient`. Signals the thread to stop and joins it
/// on drop; the thread itself stops the client and closes its event handle.
pub struct WasapiRenderStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WasapiRenderStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts a dedicated real-time render thread against `device_id`, pulling
/// interleaved stereo `f32` samples from `consumer`, running them through
/// the shared mixer stage, and writing the result to the device. Negotiates
/// exclusive mode first (falling back to shared mode — see
/// [`ExclusiveModeResult`]). If the device is mono-only, only the left
/// channel of the processed stereo pair is written (matching the old
/// cpal-based output-channel handling in `manager.rs`, which writes the
/// selected left channel and leaves any channel beyond the device's count
/// unwritten/silent).
pub fn start_render(
    device_id: &str,
    buffer_size_frames: u32,
    sample_rate: u32,
    mut consumer: HeapCons<f32>,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<f32>>,
) -> anyhow::Result<(WasapiRenderStream, ExclusiveModeResult)> {
    let client = open_audio_client(device_id)?;
    let channels = query_native_channels(&client);
    let result = initialize_client(&client, buffer_size_frames, sample_rate, channels)?;
    // SAFETY: see `start_capture`'s identical `CreateEventW` call.
    let event = unsafe { CreateEventW(None, false, false, None) }?;
    // SAFETY: `event` was just created above; `client` has been
    // `Initialize`d above.
    unsafe { client.SetEventHandle(event) }?;
    // SAFETY: requesting `IAudioRenderClient` from an initialized render
    // client is the documented way to obtain it.
    let render: IAudioRenderClient = unsafe { client.GetService() }?;
    // SAFETY: `client` is fully initialized.
    let buffer_frames = unsafe { client.GetBufferSize() }?;
    // SAFETY: `client` is fully initialized and has a service + event
    // handle registered.
    unsafe { client.Start() }?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop_flag);
    let event_raw = event.0 as isize;
    let output_is_asio = mixer.output_is_asio;
    let mut left_buf = vec![0.0f32; buffer_frames as usize];
    let mut right_buf = vec![0.0f32; buffer_frames as usize];
    // See `SendComPtr`'s doc comment.
    let client = SendComPtr(client);
    let render = SendComPtr(render);

    let thread = std::thread::spawn(move || {
        ensure_com_initialized();
        // See the identical note in `start_capture`'s thread body.
        let client = client;
        let render = render;
        let event = HANDLE(event_raw as _);
        let mmcss_once = Once::new();
        while !thread_stop.load(Ordering::Relaxed) {
            // SAFETY: `event` is a valid, still-open event handle for the
            // lifetime of this loop.
            let wait = unsafe { WaitForSingleObject(event, 1000) };
            if wait != WAIT_OBJECT_0 {
                continue;
            }
            mmcss_once.call_once(|| {
                crate::audio::mmcss::boost_current_thread_to_pro_audio();
            });

            // SAFETY: `client` is valid and started.
            let padding = unsafe { client.GetCurrentPadding() }.unwrap_or(0);
            let frames_available = buffer_frames.saturating_sub(padding);
            if frames_available == 0 {
                continue;
            }
            let frames = (frames_available as usize).min(left_buf.len());

            for i in 0..frames {
                left_buf[i] = consumer.try_pop().unwrap_or(0.0);
                right_buf[i] = consumer.try_pop().unwrap_or(0.0);
            }

            let mixer_result = process_block(&mut left_buf[..frames], &mut right_buf[..frames], &mixer, sample_rate as f64);
            let gate_open = main_output_gate_open(output_is_asio, mixer_result.is_muted, mixer_result.is_loopback);

            if mixer_result.mirror_to_virtual {
                if let Some(ref mut vp) = virt_producer {
                    for i in 0..frames {
                        let _ = vp.try_push(left_buf[i]);
                        let _ = vp.try_push(right_buf[i]);
                    }
                }
            }

            // SAFETY: `render` is a valid, started `IAudioRenderClient`;
            // `frames_available` was just reported by `GetCurrentPadding`
            // above as the free space in the device's buffer.
            let Ok(data_ptr) = (unsafe { render.GetBuffer(frames_available) }) else {
                continue;
            };
            // SAFETY: `data_ptr` was just returned by `GetBuffer` above as
            // pointing to exactly `frames_available * channels` valid
            // writable `f32` slots (the device was initialized with a
            // `channels`-wide `WAVEFORMATEX` of 32-bit float samples); the
            // slice does not outlive this iteration, and `ReleaseBuffer`
            // below is called before the next `GetBuffer`.
            let out = unsafe {
                std::slice::from_raw_parts_mut(data_ptr as *mut f32, (frames_available * channels as u32) as usize)
            };
            if channels == 1 {
                // Mono device: write only the left channel of the
                // processed stereo pair (matches the old cpal-based
                // output-channel handling, which writes the selected left
                // channel and leaves channels beyond the device's count
                // unwritten).
                for i in 0..frames_available as usize {
                    out[i] = if gate_open && i < frames { left_buf[i] } else { 0.0 };
                }
            } else {
                for i in 0..frames_available as usize {
                    let (l, r) = if gate_open && i < frames { (left_buf[i], right_buf[i]) } else { (0.0, 0.0) };
                    out[i * 2] = l;
                    out[i * 2 + 1] = r;
                }
            }
            // SAFETY: releases exactly the `frames_available` `GetBuffer`
            // handed out above, as required before the next `GetBuffer`
            // call.
            unsafe {
                let _ = render.ReleaseBuffer(frames_available, 0);
            }
        }
        // SAFETY: `client` was successfully `Start()`ed above.
        unsafe {
            let _ = client.Stop();
        }
        // SAFETY: `event` is a valid handle created by this function's
        // `CreateEventW` call above and not used again after this point.
        unsafe {
            let _ = CloseHandle(event);
        }
    });

    Ok((WasapiRenderStream { stop_flag, thread: Some(thread) }, result))
}

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
