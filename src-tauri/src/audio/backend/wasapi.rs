#![cfg(target_os = "windows")]

use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioCaptureClient, IAudioClient, IAudioRenderClient,
    WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::PCWSTR;
use ringbuf::{HeapProd, HeapCons, traits::Producer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Once};
use crate::audio::mixer::{MixerState, StereoFrame, pop_frames, process_block, main_output_gate_open};

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
#[allow(dead_code)]
fn align_frames_up(frames: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return frames;
    }
    frames.div_ceil(alignment) * alignment
}

/// Converts a frame count at `sample_rate` to 100ns `REFERENCE_TIME` units,
/// the unit `IAudioClient::Initialize`'s buffer-duration parameters use.
/// Rounds to the nearest unit (Microsoft's own documented formula adds half
/// a sample-period before truncating) rather than truncating outright —
/// truncating here can itself under-report the duration enough to trip
/// exclusive mode's alignment check.
fn frames_to_ref_time(frames: u32, sample_rate: u32) -> i64 {
    (frames as i64 * 10_000_000 + sample_rate as i64 / 2) / sample_rate as i64
}

pub struct ExclusiveModeResult {
    pub exclusive: bool,
    pub fallback_reason: Option<String>,
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

/// Resolves `device_id` to an `IMMDevice`. Kept separate from client
/// activation because the exclusive-mode retry protocol below needs to
/// `Activate` a FRESH `IAudioClient` from this same device more than once
/// (see `initialize_client`'s doc comment).
fn open_device(device_id: &str) -> anyhow::Result<IMMDevice> {
    // SAFETY: standard `IMMDeviceEnumerator`/`GetDevice` usage, identical to
    // `list_wasapi_devices`'s enumeration path elsewhere in this file.
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        Ok(enumerator.GetDevice(PCWSTR(wide.as_ptr()))?)
    }
}

fn activate_audio_client(device: &IMMDevice) -> anyhow::Result<IAudioClient> {
    // SAFETY: `device` is a valid `IMMDevice`; activating `IAudioClient` on
    // it is the documented way to obtain one.
    unsafe { Ok(device.Activate::<IAudioClient>(CLSCTX_ALL, None)?) }
}

/// Negotiates exclusive mode against `device`, retrying once with the
/// driver-reported aligned buffer size on `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`,
/// and falling back to shared mode if exclusive is refused outright.
///
/// Per Microsoft's documented `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` recovery
/// protocol, a client that has already had `Initialize` called on it once
/// (successfully or not) cannot be `Initialize`d again — a second call
/// returns `AUDCLNT_E_ALREADY_INITIALIZED` — so each attempt below
/// (initial, aligned retry, shared-mode fallback) activates its own FRESH
/// `IAudioClient` from `device` rather than reusing the one that just
/// failed. The one exception is `GetBufferSize` on the alignment-failure
/// path, which the same protocol documents as valid — and necessary — to
/// call on the client that just failed, to learn the corrected size.
fn initialize_client(
    device: &IMMDevice,
    requested_frames: u32,
    sample_rate: u32,
    channels: u16,
) -> anyhow::Result<(IAudioClient, ExclusiveModeResult)> {
    let format = wave_format_for_channels(sample_rate, channels);
    let period = frames_to_ref_time(requested_frames, sample_rate);

    let client = activate_audio_client(device)?;
    // SAFETY: `format` is a validly-constructed `WAVEFORMATEX`; `client` is
    // freshly activated and not yet initialized.
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
        Ok(()) => Ok((client, ExclusiveModeResult { exclusive: true, fallback_reason: None })),
        Err(e) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            // SAFETY: `client` is the client that just failed `Initialize`
            // with this specific error; `GetBufferSize` is documented as
            // valid to call on it in exactly this situation, to learn the
            // driver's required (already-aligned) frame count.
            let aligned_frames = unsafe { client.GetBufferSize() }.unwrap_or(requested_frames);
            let aligned_period = frames_to_ref_time(aligned_frames, sample_rate);
            // A fresh client for the retry — see this function's doc
            // comment on why `client` itself can't be re-`Initialize`d.
            let retry_client = activate_audio_client(device)?;
            let retry = unsafe {
                retry_client.Initialize(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    aligned_period,
                    aligned_period,
                    &format,
                    None,
                )
            };
            match retry {
                Ok(()) => Ok((retry_client, ExclusiveModeResult { exclusive: true, fallback_reason: None })),
                Err(e) => fall_back_to_shared(device, requested_frames, sample_rate, &format, e),
            }
        }
        Err(e) => fall_back_to_shared(device, requested_frames, sample_rate, &format, e),
    }
}

fn fall_back_to_shared(
    device: &IMMDevice,
    requested_frames: u32,
    sample_rate: u32,
    format: &WAVEFORMATEX,
    exclusive_err: windows::core::Error,
) -> anyhow::Result<(IAudioClient, ExclusiveModeResult)> {
    let period = frames_to_ref_time(requested_frames, sample_rate);
    // A fresh client — whatever exclusive-mode attempt(s) led here already
    // consumed their own client instance's one `Initialize` call.
    let client = activate_audio_client(device)?;
    // SAFETY: `format` is a validly-constructed `WAVEFORMATEX`; shared mode
    // requires `hnsperiodicity` of 0 (the engine picks its own period).
    // `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY`
    // let the audio engine insert its own format-conversion APO — without
    // them, shared-mode `Initialize` requires our format to exactly match
    // the engine's current mix format (channel count and sample rate),
    // which fails outright on any device whose mix format differs (a
    // 5.1/7.1 mix format when we request stereo, or a different native
    // sample rate) — not an edge case in practice.
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            period,
            0,
            format,
            None,
        )
    }
    .map_err(|e| anyhow::anyhow!("WASAPI shared-mode fallback also failed: {e}"))?;
    Ok((
        client,
        ExclusiveModeResult {
            exclusive: false,
            fallback_reason: Some(format!("Exclusive mode unavailable ({exclusive_err}); using shared mode")),
        },
    ))
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

/// Everything a capture stream needs, produced by [`setup_capture`] — always
/// on the real-time thread that will go on to use it (see that function's
/// doc comment for why).
struct CaptureSetup {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
    channels: u16,
    result: ExclusiveModeResult,
}

/// Performs ALL COM setup for a capture stream: resolving the device,
/// negotiating the format (including the mono-device `GetMixFormat` query),
/// creating the event, obtaining `IAudioCaptureClient`, and starting the
/// client.
///
/// MUST be called from the same thread that will go on to wait on the
/// returned event and call methods on the returned `client`/`capture` — NOT
/// from the thread that calls `start_capture`. `IAudioClient`/
/// `IAudioCaptureClient` wrap a COM interface pointer that is not `Send`
/// (COM interfaces can require apartment marshaling in general, and
/// `windows-core` does not special-case WASAPI's specific interfaces), so
/// moving one of these objects to a different thread after creating it —
/// even a thread that has also joined a compatible apartment — is not
/// something this module attempts. Calling this function ON the real-time
/// thread instead (which joins the same `COINIT_MULTITHREADED` apartment
/// via `ensure_com_initialized()` first) means every COM object it creates
/// simply never leaves the thread it was created on.
fn setup_capture(device_id: &str, buffer_size_frames: u32, sample_rate: u32) -> anyhow::Result<CaptureSetup> {
    let device = open_device(device_id)?;
    // A throwaway client, `Activate`d but never `Initialize`d, purely to
    // query the device's native channel count before committing to a
    // format — see `query_native_channels`'s doc comment for why a
    // hardcoded stereo format isn't safe to assume.
    let probe_client = activate_audio_client(&device)?;
    let channels = query_native_channels(&probe_client);
    drop(probe_client);

    let (client, result) = initialize_client(&device, buffer_size_frames, sample_rate, channels)?;

    // SAFETY: `CreateEventW` with all-`None`/`false` arguments creates an
    // anonymous, auto-reset-off, initially-unsignaled event; a valid
    // pattern for WASAPI's event-driven mode.
    let event = unsafe { CreateEventW(None, false, false, None) }?;
    let rest = (|| -> anyhow::Result<IAudioCaptureClient> {
        // SAFETY: `event` was just created above and is a valid event
        // handle; `client` has been `Initialize`d (exclusive or shared)
        // above.
        unsafe { client.SetEventHandle(event) }?;
        // SAFETY: requesting `IAudioCaptureClient` from an initialized
        // capture client is the documented way to obtain it.
        let capture: IAudioCaptureClient = unsafe { client.GetService() }?;
        // SAFETY: `client` is fully initialized and has a service + event
        // handle registered.
        unsafe { client.Start() }?;
        Ok(capture)
    })();

    match rest {
        Ok(capture) => Ok(CaptureSetup { client, capture, event, channels, result }),
        Err(e) => {
            // Close the event on every failure path past its creation, not
            // just on success.
            // SAFETY: `event` was created above by this function and has
            // not been handed to anything else yet on this failure path.
            unsafe {
                let _ = CloseHandle(event);
            }
            Err(e)
        }
    }
}

/// Starts a dedicated real-time capture thread against `device_id`,
/// pushing de-interleaved stereo `f32` samples into `producer`. Negotiates
/// exclusive mode first (falling back to shared mode — see
/// [`ExclusiveModeResult`]), and handles mono-only devices by duplicating
/// the single decoded sample to both L/R, matching the old cpal-based
/// `input_channels < 2` handling in `manager.rs`.
///
/// All COM setup (see [`setup_capture`]) runs ON the spawned real-time
/// thread, not on the calling thread — the calling thread may be in a
/// different (or no) COM apartment (e.g. Tauri's command dispatch thread,
/// which `tao`'s windowing setup puts into a single-threaded apartment),
/// and WASAPI's COM objects are not safe to create on one thread and use
/// from another. The setup `Result` crosses back to the caller over a
/// bounded channel instead.
pub fn start_capture(
    device_id: &str,
    buffer_size_frames: u32,
    sample_rate: u32,
    mut producer: HeapProd<StereoFrame>,
) -> anyhow::Result<(WasapiCaptureStream, ExclusiveModeResult)> {
    let device_id = device_id.to_string();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop_flag);
    let (result_tx, result_rx) = mpsc::sync_channel::<anyhow::Result<ExclusiveModeResult>>(1);

    let thread = std::thread::spawn(move || {
        ensure_com_initialized();
        let CaptureSetup { client, capture, event, channels, result } =
            match setup_capture(&device_id, buffer_size_frames, sample_rate) {
                Ok(setup) => setup,
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                    return;
                }
            };
        if result_tx.send(Ok(result)).is_err() {
            // Caller gave up waiting (e.g. panicked) before we could report
            // success — nothing to serve, tear down and exit.
            unsafe {
                let _ = client.Stop();
                let _ = CloseHandle(event);
            }
            return;
        }

        let mmcss_once = Once::new();
        let mut device_error: Option<windows::core::Error> = None;
        'outer: while !thread_stop.load(Ordering::Relaxed) {
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

            // Drain every packet queued since the last wake, not just one —
            // in shared mode especially, more than one packet can arrive
            // between events.
            loop {
                // SAFETY: `capture` is a valid, started `IAudioCaptureClient`.
                let packet_frames = match unsafe { capture.GetNextPacketSize() } {
                    Ok(p) => p,
                    Err(e) => {
                        device_error = Some(e);
                        break;
                    }
                };
                if packet_frames == 0 {
                    break;
                }
                let mut data_ptr = std::ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;
                // SAFETY: `capture` is valid and started; `data_ptr`/
                // `num_frames`/`flags` are valid out-pointers for this call.
                if let Err(e) = unsafe { capture.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None) } {
                    device_error = Some(e);
                    break;
                }
                // `GetBuffer` can report success with a null pointer and 0
                // frames (`AUDCLNT_S_BUFFER_EMPTY`, e.g. a muted capture
                // device) — nothing to read, but still must `ReleaseBuffer`
                // the (empty) buffer it handed out before treating this as
                // "drained" and going back to waiting.
                if num_frames == 0 || data_ptr.is_null() {
                    unsafe {
                        let _ = capture.ReleaseBuffer(num_frames);
                    }
                    break;
                }
                // SAFETY: `data_ptr` was just returned by `GetBuffer` above
                // as pointing to exactly `num_frames * channels` valid
                // `f32` samples (the device was initialized with a
                // `channels`-wide `WAVEFORMATEX` of 32-bit float samples);
                // the slice does not outlive this iteration, and
                // `ReleaseBuffer` below is called before the next
                // `GetBuffer`.
                let samples = unsafe {
                    std::slice::from_raw_parts(data_ptr as *const f32, (num_frames * channels as u32) as usize)
                };
                if channels == 1 {
                    // Mono device: duplicate the single decoded sample to
                    // both L/R (matches the old cpal `input_channels < 2`
                    // handling).
                    for &s in samples.iter() {
                        let _ = producer.try_push([s, s]);
                    }
                } else {
                    for pair in samples.chunks_exact(2) {
                        let _ = producer.try_push([pair[0], pair[1]]);
                    }
                }
                // SAFETY: releases exactly the `num_frames` `GetBuffer`
                // handed out above, as required before the next
                // `GetBuffer` call.
                unsafe {
                    let _ = capture.ReleaseBuffer(num_frames);
                }
            }
            if let Some(e) = device_error {
                log::warn!("WASAPI capture device error (device may be lost), stopping capture thread: {e}");
                break 'outer;
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

    match result_rx.recv() {
        Ok(Ok(result)) => Ok((WasapiCaptureStream { stop_flag, thread: Some(thread) }, result)),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err(anyhow::anyhow!("WASAPI capture setup thread ended without reporting a result"))
        }
    }
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

/// Everything a render stream needs, produced by [`setup_render`] — see
/// [`setup_capture`]'s doc comment for why this must run on the real-time
/// thread that will go on to use it, not on the calling thread.
struct RenderSetup {
    client: IAudioClient,
    render: IAudioRenderClient,
    event: HANDLE,
    channels: u16,
    buffer_frames: u32,
    result: ExclusiveModeResult,
}

/// Render's counterpart to [`setup_capture`] — same COM-setup shape, same
/// same-thread requirement, plus reading `GetBufferSize()` once the client
/// is initialized (the render loop needs its total buffer size, not just
/// `IAudioRenderClient`).
fn setup_render(device_id: &str, buffer_size_frames: u32, sample_rate: u32) -> anyhow::Result<RenderSetup> {
    let device = open_device(device_id)?;
    let probe_client = activate_audio_client(&device)?;
    let channels = query_native_channels(&probe_client);
    drop(probe_client);

    let (client, result) = initialize_client(&device, buffer_size_frames, sample_rate, channels)?;

    // SAFETY: see `setup_capture`'s identical `CreateEventW` call.
    let event = unsafe { CreateEventW(None, false, false, None) }?;
    let rest = (|| -> anyhow::Result<(IAudioRenderClient, u32)> {
        // SAFETY: `event` was just created above; `client` has been
        // `Initialize`d above.
        unsafe { client.SetEventHandle(event) }?;
        // SAFETY: requesting `IAudioRenderClient` from an initialized
        // render client is the documented way to obtain it.
        let render: IAudioRenderClient = unsafe { client.GetService() }?;
        // SAFETY: `client` is fully initialized.
        let buffer_frames = unsafe { client.GetBufferSize() }?;
        // SAFETY: `client` is fully initialized and has a service + event
        // handle registered.
        unsafe { client.Start() }?;
        Ok((render, buffer_frames))
    })();

    match rest {
        Ok((render, buffer_frames)) => Ok(RenderSetup { client, render, event, channels, buffer_frames, result }),
        Err(e) => {
            // SAFETY: `event` was created above by this function and has
            // not been handed to anything else yet on this failure path.
            unsafe {
                let _ = CloseHandle(event);
            }
            Err(e)
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
///
/// See [`start_capture`]'s doc comment for why setup runs on the spawned
/// thread and the result crosses back over a channel rather than being
/// returned directly.
pub fn start_render(
    device_id: &str,
    buffer_size_frames: u32,
    sample_rate: u32,
    mut consumer: HeapCons<StereoFrame>,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<StereoFrame>>,
) -> anyhow::Result<(WasapiRenderStream, ExclusiveModeResult)> {
    let device_id = device_id.to_string();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop_flag);
    let output_is_asio = mixer.output_is_asio;
    let (result_tx, result_rx) = mpsc::sync_channel::<anyhow::Result<ExclusiveModeResult>>(1);

    let thread = std::thread::spawn(move || {
        ensure_com_initialized();
        let RenderSetup { client, render, event, channels, buffer_frames, result } =
            match setup_render(&device_id, buffer_size_frames, sample_rate) {
                Ok(setup) => setup,
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                    return;
                }
            };
        if result_tx.send(Ok(result)).is_err() {
            unsafe {
                let _ = client.Stop();
                let _ = CloseHandle(event);
            }
            return;
        }

        let mut left_buf = vec![0.0f32; buffer_frames as usize];
        let mut right_buf = vec![0.0f32; buffer_frames as usize];
        let mmcss_once = Once::new();
        // Logs a `GetCurrentPadding` failure once rather than silently
        // treating every future call as "0 padding" forever — a device-lost
        // condition should be visible somewhere.
        let padding_err_once = Once::new();
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
            let padding = match unsafe { client.GetCurrentPadding() } {
                Ok(p) => p,
                Err(e) => {
                    padding_err_once.call_once(|| {
                        log::warn!("WASAPI GetCurrentPadding failed (device may be lost): {e}");
                    });
                    0
                }
            };
            let frames_available = buffer_frames.saturating_sub(padding);
            if frames_available == 0 {
                continue;
            }
            let frames = (frames_available as usize).min(left_buf.len());

            // A `try_pop()` miss here means the upstream producer hasn't
            // kept up — count it as an underrun rather than silently
            // playing 0.0.
            let underruns = pop_frames(&mut consumer, &mut left_buf[..frames], &mut right_buf[..frames]);
            if underruns > 0 {
                mixer.underrun_count.fetch_add(underruns, Ordering::Relaxed);
            }

            let mixer_result = process_block(&mut left_buf[..frames], &mut right_buf[..frames], &mixer, sample_rate as f64);
            let gate_open = main_output_gate_open(output_is_asio, mixer_result.is_muted, mixer_result.is_loopback);

            if mixer_result.mirror_to_virtual {
                if let Some(ref mut vp) = virt_producer {
                    for i in 0..frames {
                        let _ = vp.try_push([left_buf[i], right_buf[i]]);
                    }
                }
            }

            // SAFETY: `render` is a valid, started `IAudioRenderClient`;
            // `frames_available` was just reported by `GetCurrentPadding`
            // above as the free space in the device's buffer.
            let Ok(data_ptr) = (unsafe { render.GetBuffer(frames_available) }) else {
                continue;
            };
            // `GetBuffer` can in principle report success with a null
            // pointer — guard before building a slice from it, symmetric
            // with the same check on the capture side.
            if data_ptr.is_null() {
                unsafe {
                    let _ = render.ReleaseBuffer(frames_available, 0);
                }
                continue;
            }
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

    match result_rx.recv() {
        Ok(Ok(result)) => Ok((WasapiRenderStream { stop_flag, thread: Some(thread) }, result)),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err(anyhow::anyhow!("WASAPI render setup thread ended without reporting a result"))
        }
    }
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
