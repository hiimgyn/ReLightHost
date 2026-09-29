#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, Once};
use asio_sys::{Asio, AsioSampleType, BufferPreference, CallbackInfo, Driver};
use ringbuf::{HeapProd, HeapCons, traits::Producer};
use crate::audio::mixer::{BacklogTrimmer, MixerState, StereoFrame, pop_frames, process_block, main_output_gate_open};

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

/// Set when the running driver asks to be torn down and rebuilt
/// (`kAsioResetRequest` — typically after its buffer size was changed in the
/// driver's own control panel) or reports a new sample rate. Can't be acted
/// on inside the driver callback; `AudioManager::get_status` (polled by the
/// UI) restarts the stream. One flag suffices: ASIO loads one driver at a time.
static RESET_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn on_driver_event(event: asio_sys::AsioDriverEvent) -> bool {
    use asio_sys::{AsioDriverEvent, AsioMessageSelectors};
    match event {
        AsioDriverEvent::Message { selector: AsioMessageSelectors::kAsioResetRequest, .. }
        | AsioDriverEvent::SampleRateChanged(_) => {
            RESET_REQUESTED.store(true, Ordering::Release);
        }
        _ => {}
    }
    false
}

/// Returns (and clears) a pending driver reset request.
pub fn take_reset_request() -> bool {
    RESET_REQUESTED.swap(false, Ordering::AcqRel)
}

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
/// NOTE (scope): this hardcodes the `ASIOSTInt32LSB` conversion. It is one
/// of the two natively-supported formats — see [`AsioSampleFormat`] and
/// [`resolve_asio_sample_format`] for the other (`ASIOSTFloat32LSB`, which
/// needs no conversion at all). Broader format support (e.g. `Int16LSB`,
/// `Int24LSB`) stays out of scope. Matches the existing `f32_to_i16` clamp
/// pattern in `manager.rs`.
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
    (v.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32
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

/// The two ASIO sample formats this module knows how to read/write.
/// Resolved once per stream at setup time (see [`resolve_asio_sample_format`])
/// and stored for the lifetime of the callback — never re-queried from the
/// driver on the real-time path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsioSampleFormat {
    /// `ASIOSTInt32LSB` — needs [`f32_to_asio_i32`]/[`asio_i32_to_f32`].
    Int32Lsb,
    /// `ASIOSTFloat32LSB` — the buffer already holds native-endian `f32`
    /// values directly; no conversion needed.
    Float32Lsb,
}

/// Maps a driver-reported ASIO sample type to one of the two currently
/// supported formats, or `None` for anything else (e.g. `ASIOSTInt16LSB`/
/// `ASIOSTInt24LSB`) — broader format support stays explicitly out of scope.
fn resolve_asio_sample_format(sample_type: &AsioSampleType) -> Option<AsioSampleFormat> {
    match sample_type {
        AsioSampleType::ASIOSTInt32LSB => Some(AsioSampleFormat::Int32Lsb),
        AsioSampleType::ASIOSTFloat32LSB => Some(AsioSampleFormat::Float32Lsb),
        _ => None,
    }
}

/// Reads one sample from a raw ASIO buffer pointer, decoding it according to
/// `format` — resolved once at stream setup, not re-queried here.
///
/// # Safety
/// `ptr` must point to a valid buffer of at least `frame + 1` samples of the
/// width implied by `format` (4 bytes either way, for both currently
/// supported formats).
#[inline]
unsafe fn read_asio_sample(ptr: *const c_void, frame: usize, format: AsioSampleFormat) -> f32 {
    match format {
        AsioSampleFormat::Int32Lsb => asio_i32_to_f32(unsafe { *(ptr as *const i32).add(frame) }),
        AsioSampleFormat::Float32Lsb => unsafe { *(ptr as *const f32).add(frame) },
    }
}

/// Inverse of [`read_asio_sample`]: encodes `value` into a raw ASIO buffer
/// pointer according to `format`.
///
/// # Safety
/// `ptr` must point to a valid, exclusively-owned buffer of at least
/// `frame + 1` samples of the width implied by `format`.
#[inline]
unsafe fn write_asio_sample(ptr: *mut c_void, frame: usize, format: AsioSampleFormat, value: f32) {
    match format {
        AsioSampleFormat::Int32Lsb => unsafe {
            *(ptr as *mut i32).add(frame) = f32_to_asio_i32(value);
        },
        AsioSampleFormat::Float32Lsb => unsafe {
            *(ptr as *mut f32).add(frame) = value;
        },
    }
}

/// Rate the driver ends up running at: `requested` if it already runs there
/// or accepted a switch to it, otherwise its own `current` rate.
fn choose_rate(current: f64, requested: f64, switched: bool) -> f64 {
    if (current - requested).abs() < 1.0 || switched { requested } else { current }
}

/// Asks `driver` to run at `requested` (the ASIO SDK never does this on its
/// own — without it the device keeps whatever rate it was last set to) and
/// returns the rate it actually runs at.
fn ensure_rate(driver: &Driver, requested: f64, driver_name: &str) -> f64 {
    let current = driver.sample_rate().unwrap_or(requested);
    let switched = (current - requested).abs() >= 1.0
        && driver.can_sample_rate(requested).unwrap_or(false)
        && driver.set_sample_rate(requested).is_ok();
    let rate = choose_rate(current, requested, switched);
    if (rate - requested).abs() >= 1.0 {
        log::warn!("ASIO driver '{driver_name}' cannot run at {requested} Hz; using its current {rate} Hz");
    }
    rate
}

/// Resolves the rate an ASIO driver will run at before any stream starts,
/// so a WASAPI leg bridged to it can be opened at the same rate. Returns
/// `requested` unchanged if the driver can't be loaded right now.
pub fn negotiate_sample_rate(driver_name: &str, requested: f64) -> f64 {
    let _lifecycle_guard = ASIO_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if ASIO.loaded_driver().is_some() {
        return requested;
    }
    match ASIO.load_driver(driver_name) {
        Ok(driver) => ensure_rate(&driver, requested, driver_name),
        Err(_) => requested,
    }
}

/// Validates a caller-supplied buffer-size hint against what `driver`
/// actually supports. `asio-sys`'s `create_buffers` (which
/// `prepare_input_stream`/`prepare_output_stream` call into) only checks the
/// hint against the driver's max — it does not check the driver's minimum,
/// its step size, or a driver that only supports one fixed size — so an
/// out-of-range or misaligned hint would otherwise be passed straight
/// through to `ASIOCreateBuffers`, which is fatal for it.
///
/// Returns `None` (matching the OLD cpal-based code's `BufferSize::Default`
/// behavior — let the driver pick its own preferred size) when the hint
/// doesn't fit what `buffersize_range()` reports, logging why.
fn validate_buffer_size_hint(
    driver: &Driver,
    hint: Option<i32>,
    driver_name: &str,
) -> Option<i32> {
    let hint = hint?;
    match driver.buffersize_range() {
        Ok(range) => hint_fits_buffer_size_range(hint, range, driver_name),
        Err(e) => {
            log::warn!(
                "ASIO buffersize_range() query failed for '{driver_name}', ignoring \
                 configured buffer size {hint} and using the driver's own preferred \
                 size instead: {e}"
            );
            None
        }
    }
}

/// Pure decision core of [`validate_buffer_size_hint`], split out so the
/// min/max/step-vs-fixed-size logic can be unit-tested without a real,
/// loaded ASIO driver (see `format_tests` below).
fn hint_fits_buffer_size_range(
    hint: i32,
    range: asio_sys::BufferSizeRange,
    driver_name: &str,
) -> Option<i32> {
    if hint < range.min || hint > range.max {
        log::warn!(
            "Configured ASIO buffer size {hint} is outside driver '{driver_name}''s \
             supported range ({}..={}); using the driver's own preferred size instead",
            range.min,
            range.max
        );
        return None;
    }

    match range.preferred {
        // Note: `asio-sys` maps Steinberg ASIO SDK's `granularity == -1` (the most common
        // case: buffer sizes must increase by powers of 2 between min and max) to
        // `BufferPreference::Only(preferred)`. Despite the misleading name in `asio-sys`,
        // it does NOT mean the driver only supports a single fixed buffer size!
        // When min != max, any power-of-2 buffer size within [min, max] is valid.
        BufferPreference::Only(_)
            if range.min != range.max && !is_valid_power_of_two_buffer_size(hint, range.min) =>
        {
            log::warn!(
                "Configured ASIO buffer size {hint} is not a power of 2 as required \
                 by driver '{driver_name}' (granularity -1); using the driver's own \
                 preferred size instead",
            );
            None
        }
        BufferPreference::Stepped { step, .. }
            if step > 0 && (hint - range.min) % step as i32 != 0 =>
        {
            log::warn!(
                "Configured ASIO buffer size {hint} does not align with driver \
                 '{driver_name}''s step size of {step} frames (starting at {}); using \
                 the driver's own preferred size instead",
                range.min
            );
            None
        }
        _ => Some(hint),
    }
}

/// Helper checking if `hint` is a valid power-of-2 buffer size or a power-of-2
/// multiple of `min` (for Steinberg `granularity == -1`).
fn is_valid_power_of_two_buffer_size(hint: i32, min: i32) -> bool {
    if hint <= 0 {
        return false;
    }
    let h = hint as u32;
    if h.is_power_of_two() {
        return true;
    }
    // Rare non-power-of-two min base (e.g. min * 2^k)
    if min > 0 && hint % min == 0 {
        let factor = (hint / min) as u32;
        return factor.is_power_of_two();
    }
    false
}

/// Owns a running ASIO driver and its registered callback — full-duplex
/// (`start_duplex`) or single-direction (`start_input_only`/
/// `start_output_only`). The three constructors differ only in how many
/// `AsioBufferInfo` entries they register and what the callback body reads/
/// writes; the driver-lifecycle shape (one loaded driver, one callback, torn
/// down via `stop()`) is identical regardless of direction, so all three
/// share this one type instead of three near-duplicates.
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
    event_callback_id: asio_sys::DriverEventCallbackId,
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
/// NOT handled here, and is not supported at all: the ASIO SDK only allows
/// one loaded driver per process (see `ASIO_LIFECYCLE_LOCK`'s doc comment
/// above), so `manager.rs` rejects that combination outright before calling
/// into this module rather than attempting to bridge it. This function only
/// covers the single-driver full-duplex case; `start_input_only`/
/// `start_output_only` below cover the single-ASIO-side-paired-with-WASAPI
/// case `manager.rs` actually uses instead.
///
/// Sample-type note: this function REFUSES to start (returns `Err`) unless
/// the driver's native format is `ASIOSTInt32LSB` or `ASIOSTFloat32LSB` (see
/// [`AsioSampleFormat`]/[`resolve_asio_sample_format`]) — the fixed-width
/// pointer arithmetic in the callback below would silently read/write out
/// of bounds against a driver using a different sample width (e.g. 2-byte
/// `ASIOSTInt16LSB` or 3-byte `ASIOSTInt24LSB`). The format actually in use
/// is resolved once here, at setup, and stored for the callback's lifetime
/// — never re-queried from the driver per callback invocation.
pub fn start_duplex(
    driver_name: &str,
    in_offset: usize,
    out_offset: usize,
    buffer_size_hint: Option<i32>,
    sample_rate: f64,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<StereoFrame>>,
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

    // Validate the hint once, up front — both `prepare_input_stream` and
    // `prepare_output_stream` below feed into the same `ASIOCreateBuffers`
    // call (see the two-step construction comment below), so they share one
    // validated (or `None`-if-invalid) buffer size rather than each risking
    // its own inconsistent fallback.
    ensure_rate(&driver, sample_rate, driver_name);
    let buffer_size_hint = validate_buffer_size_hint(&driver, buffer_size_hint, driver_name);

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
    // the two this module knows how to read/write: a narrower format (e.g.
    // 2-byte ASIOSTInt16LSB) would make every fixed-width pointer access
    // below read/write past the end of its real per-sample width. Resolved
    // ONCE here, at setup — the callback below branches on the stored
    // `input_format`/`output_format` rather than re-querying the driver.
    let input_type = driver
        .input_data_type()
        .map_err(|e| anyhow::anyhow!("Failed to query ASIO input sample type: {e}"))?;
    let output_type = driver
        .output_data_type()
        .map_err(|e| anyhow::anyhow!("Failed to query ASIO output sample type: {e}"))?;
    let (Some(input_format), Some(output_format)) = (
        resolve_asio_sample_format(&input_type),
        resolve_asio_sample_format(&output_type),
    ) else {
        return Err(anyhow::anyhow!(
            "ASIO driver '{driver_name}' reports unsupported sample format \
             (input: {input_type:?}, output: {output_type:?}); only \
             ASIOSTInt32LSB and ASIOSTFloat32LSB are currently supported"
        ));
    };

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
            // above for exactly `buffer_size` samples of either supported
            // format (guarded by the format check above) — both are 4 bytes
            // wide, and a zero bit pattern represents `0` in `i32` and `0.0`
            // in `f32` identically, so this zeroing loop is format-agnostic.
            // This runs once during setup, before `driver.start()`, so there
            // is no concurrent callback access to race with.
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
        // callback invocation). Kept as raw `*mut c_void` here — cast to the
        // right pointer type inside `read_asio_sample`/`write_asio_sample`
        // per `input_format`/`output_format`, which were resolved once at
        // setup (above), not re-queried here.
        let in_l_ptr = input_stream.buffer_infos[in_l].buffers[idx];
        let in_r_ptr = input_stream.buffer_infos[in_r].buffers[idx];
        let out_l_ptr = output_stream.buffer_infos[out_l].buffers[idx];
        let out_r_ptr = output_stream.buffer_infos[out_r].buffers[idx];

        // SAFETY: `in_l_ptr`/`in_r_ptr` point into buffers allocated by
        // ASIO's `ASIOCreateBuffers` above for exactly `buffer_size` samples
        // of `input_format`'s width per half of the double buffer (guarded
        // by the format check in `start_duplex`); `idx` is the half ASIO
        // just told us (via `CallbackInfo::buffer_index`) is ready to read,
        // and `frame` is bounds-checked by iterating `left_buf`/`right_buf`,
        // which were sized to `buffer_size`.
        for (frame, sample) in left_buf.iter_mut().enumerate() {
            *sample = unsafe { read_asio_sample(in_l_ptr, frame, input_format) };
        }
        for (frame, sample) in right_buf.iter_mut().enumerate() {
            *sample = unsafe { read_asio_sample(in_r_ptr, frame, input_format) };
        }

        let result = process_block(&mut left_buf, &mut right_buf, &mixer, sample_rate);
        let gate_open = main_output_gate_open(output_is_asio, result.is_muted, result.is_loopback);

        if result.mirror_to_virtual {
            if let Some(ref mut vp) = virt_producer {
                for frame in 0..left_buf.len() {
                    let _ = vp.try_push([left_buf[frame], right_buf[frame]]);
                }
            }
        }

        // SAFETY: same buffer-ownership/index reasoning as the input read
        // above, but writing; ASIO guarantees exclusive access to buffer
        // half `idx` for the duration of this callback.
        for (frame, sample) in left_buf.iter().enumerate() {
            let value = if gate_open { *sample } else { 0.0 };
            unsafe {
                write_asio_sample(out_l_ptr, frame, output_format, value);
            }
        }
        for (frame, sample) in right_buf.iter().enumerate() {
            let value = if gate_open { *sample } else { 0.0 };
            unsafe {
                write_asio_sample(out_r_ptr, frame, output_format, value);
            }
        }
    });

    RESET_REQUESTED.store(false, Ordering::Release);
    let event_callback_id = driver.add_event_callback(on_driver_event);
    if let Err(e) = driver.start() {
        // Don't leave a stale callback registered on a failed start — it
        // would otherwise sit in asio-sys's global callback list holding
        // pointers into these buffers indefinitely.
        driver.remove_callback(callback_id);
        driver.remove_event_callback(event_callback_id);
        return Err(anyhow::anyhow!("Failed to start ASIO driver: {e}"));
    }
    Ok(AsioDuplexStream { driver, callback_id, event_callback_id })
}

/// Starts ASIO input capture only (no output side registered) — used for
/// the "bridged" case where the other direction is a WASAPI device. Pushes
/// de-interleaved stereo `f32` samples straight into `producer`; no mixer
/// stage runs on the input side, mirroring `backend::wasapi::start_capture`'s
/// shape.
///
/// See `start_duplex`'s doc comment for the lifecycle/offset/sample-type
/// notes, which apply identically here — this is the same setup collapsed
/// to one direction (`prepare_input_stream` called alone, per the brief's
/// note that `asio-sys`'s single-direction prepare calls already support
/// this via `None` for the other side).
pub fn start_input_only(
    driver_name: &str,
    offset: usize,
    buffer_size_hint: Option<i32>,
    sample_rate: f64,
    mut producer: HeapProd<StereoFrame>,
) -> anyhow::Result<AsioDuplexStream> {
    // See `ASIO_LIFECYCLE_LOCK`'s doc comment — held for this whole setup path.
    let _lifecycle_guard = ASIO_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if ASIO.loaded_driver().is_some() {
        return Err(anyhow::anyhow!(
            "an ASIO driver is already active; call stop() before starting a new stream"
        ));
    }

    let driver = ASIO
        .load_driver(driver_name)
        .map_err(|e| anyhow::anyhow!("Failed to load ASIO driver '{driver_name}': {e}"))?;

    // See `start_duplex`'s identical comment on why the hint is validated
    // against the driver's actual range/step before use.
    ensure_rate(&driver, sample_rate, driver_name);
    let buffer_size_hint = validate_buffer_size_hint(&driver, buffer_size_hint, driver_name);

    // See `start_duplex`'s identical comment on why enough channels to
    // cover `offset` are requested rather than just 2.
    let channels = offset + 2;
    let input_stream = driver
        .prepare_input_stream(None, channels, buffer_size_hint)
        .map_err(|e| anyhow::anyhow!("Failed to prepare ASIO input buffers: {e}"))?
        .input
        .ok_or_else(|| anyhow::anyhow!("ASIO driver returned no input stream"))?;

    // See `start_duplex`'s identical comment — resolved once here, stored
    // for the callback's lifetime, never re-queried per callback invocation.
    let input_type = driver
        .input_data_type()
        .map_err(|e| anyhow::anyhow!("Failed to query ASIO input sample type: {e}"))?;
    let Some(input_format) = resolve_asio_sample_format(&input_type) else {
        return Err(anyhow::anyhow!(
            "ASIO driver '{driver_name}' reports unsupported input sample format \
             ({input_type:?}); only ASIOSTInt32LSB and ASIOSTFloat32LSB are currently \
             supported"
        ));
    };

    let buffer_size = input_stream.buffer_size.max(0) as usize;
    let in_l = offset;
    let in_r = offset + 1;
    let mmcss_once = Once::new();

    let callback_id = driver.add_callback(move |info: &CallbackInfo| {
        // Force whole-`AsioStream` capture — see `start_duplex`'s identical
        // comment on why a bare `Vec<AsioBufferInfo>` field capture isn't `Send`.
        let input_stream = &input_stream;

        mmcss_once.call_once(|| {
            crate::audio::mmcss::boost_current_thread_to_pro_audio();
        });

        let idx = info.buffer_index as usize;
        let in_l_ptr = input_stream.buffer_infos[in_l].buffers[idx];
        let in_r_ptr = input_stream.buffer_infos[in_r].buffers[idx];

        // SAFETY: same buffer-ownership/index reasoning as `start_duplex`'s
        // identical input-read loop.
        for frame in 0..buffer_size {
            let l = unsafe { read_asio_sample(in_l_ptr, frame, input_format) };
            let r = unsafe { read_asio_sample(in_r_ptr, frame, input_format) };
            // Non-blocking: drop the frame rather than blocking the
            // real-time thread if the ring buffer is full.
            let _ = producer.try_push([l, r]);
        }
    });

    RESET_REQUESTED.store(false, Ordering::Release);
    let event_callback_id = driver.add_event_callback(on_driver_event);
    if let Err(e) = driver.start() {
        driver.remove_callback(callback_id);
        driver.remove_event_callback(event_callback_id);
        return Err(anyhow::anyhow!("Failed to start ASIO driver: {e}"));
    }
    Ok(AsioDuplexStream { driver, callback_id, event_callback_id })
}

/// Starts ASIO output only (no input side registered) — used for the
/// "bridged" case where the other direction is a WASAPI device. Pulls from
/// `consumer`, runs the shared mixer stage, and writes the result to the
/// selected output pair; mirrors `backend::wasapi::start_render`'s shape,
/// including the same optional virtual-mirror producer.
///
/// See `start_duplex`'s doc comment for the lifecycle/offset/sample-type
/// notes, which apply identically here.
pub fn start_output_only(
    driver_name: &str,
    offset: usize,
    buffer_size_hint: Option<i32>,
    sample_rate: f64,
    mut consumer: HeapCons<StereoFrame>,
    mixer: MixerState,
    mut virt_producer: Option<HeapProd<StereoFrame>>,
) -> anyhow::Result<AsioDuplexStream> {
    // See `ASIO_LIFECYCLE_LOCK`'s doc comment — held for this whole setup path.
    let _lifecycle_guard = ASIO_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if ASIO.loaded_driver().is_some() {
        return Err(anyhow::anyhow!(
            "an ASIO driver is already active; call stop() before starting a new stream"
        ));
    }

    let driver = ASIO
        .load_driver(driver_name)
        .map_err(|e| anyhow::anyhow!("Failed to load ASIO driver '{driver_name}': {e}"))?;

    // See `start_duplex`'s identical comment on why the hint is validated
    // against the driver's actual range/step before use.
    ensure_rate(&driver, sample_rate, driver_name);
    let buffer_size_hint = validate_buffer_size_hint(&driver, buffer_size_hint, driver_name);

    let channels = offset + 2;
    let output_stream = driver
        .prepare_output_stream(None, channels, buffer_size_hint)
        .map_err(|e| anyhow::anyhow!("Failed to prepare ASIO output buffers: {e}"))?
        .output
        .ok_or_else(|| anyhow::anyhow!("ASIO driver returned no output stream"))?;

    // See `start_duplex`'s identical comment — resolved once here, stored
    // for the callback's lifetime, never re-queried per callback invocation.
    let output_type = driver
        .output_data_type()
        .map_err(|e| anyhow::anyhow!("Failed to query ASIO output sample type: {e}"))?;
    let Some(output_format) = resolve_asio_sample_format(&output_type) else {
        return Err(anyhow::anyhow!(
            "ASIO driver '{driver_name}' reports unsupported output sample format \
             ({output_type:?}); only ASIOSTInt32LSB and ASIOSTFloat32LSB are currently \
             supported"
        ));
    };

    let buffer_size = output_stream.buffer_size.max(0) as usize;
    let out_l = offset;
    let out_r = offset + 1;

    // Zero every output channel before starting — see `start_duplex`'s
    // identical comment for why.
    for info in output_stream.buffer_infos.iter() {
        let buffers = info.buffers;
        for half in buffers {
            if half.is_null() {
                continue;
            }
            // SAFETY: see `start_duplex`'s identical zeroing loop.
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
    let mut trimmer = BacklogTrimmer::new(sample_rate as usize);
    let mut left_buf = vec![0.0f32; buffer_size];
    let mut right_buf = vec![0.0f32; buffer_size];
    let output_is_asio = mixer.output_is_asio;

    let callback_id = driver.add_callback(move |info: &CallbackInfo| {
        let output_stream = &output_stream;

        mmcss_once.call_once(|| {
            crate::audio::mmcss::boost_current_thread_to_pro_audio();
        });

        let idx = info.buffer_index as usize;
        let out_l_ptr = output_stream.buffer_infos[out_l].buffers[idx];
        let out_r_ptr = output_stream.buffer_infos[out_r].buffers[idx];

        // A `try_pop()` miss here means the upstream producer (WASAPI
        // capture, or whatever feeds this ring buffer) hasn't kept up —
        // count it as an underrun rather than silently playing 0.0.
        trimmer.before_pop(&mut consumer, buffer_size);
        let underruns = pop_frames(&mut consumer, &mut left_buf, &mut right_buf);
        if underruns > 0 {
            mixer.underrun_count.fetch_add(underruns, Ordering::Relaxed);
        }

        let result = process_block(&mut left_buf, &mut right_buf, &mixer, sample_rate);
        let gate_open = main_output_gate_open(output_is_asio, result.is_muted, result.is_loopback);

        if result.mirror_to_virtual {
            if let Some(ref mut vp) = virt_producer {
                for frame in 0..left_buf.len() {
                    let _ = vp.try_push([left_buf[frame], right_buf[frame]]);
                }
            }
        }

        // SAFETY: same buffer-ownership/index reasoning as `start_duplex`'s
        // identical output-write loop.
        for (frame, sample) in left_buf.iter().enumerate() {
            let value = if gate_open { *sample } else { 0.0 };
            unsafe {
                write_asio_sample(out_l_ptr, frame, output_format, value);
            }
        }
        for (frame, sample) in right_buf.iter().enumerate() {
            let value = if gate_open { *sample } else { 0.0 };
            unsafe {
                write_asio_sample(out_r_ptr, frame, output_format, value);
            }
        }
    });

    RESET_REQUESTED.store(false, Ordering::Release);
    let event_callback_id = driver.add_event_callback(on_driver_event);
    if let Err(e) = driver.start() {
        driver.remove_callback(callback_id);
        driver.remove_event_callback(event_callback_id);
        return Err(anyhow::anyhow!("Failed to start ASIO driver: {e}"));
    }
    Ok(AsioDuplexStream { driver, callback_id, event_callback_id })
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
    stream.driver.remove_event_callback(stream.event_callback_id);
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
mod reset_tests {
    use super::*;
    use asio_sys::{AsioDriverEvent, AsioMessageSelectors};

    #[test]
    fn reset_and_rate_change_requests_are_latched_once() {
        let _ = take_reset_request();
        on_driver_event(AsioDriverEvent::Message { selector: AsioMessageSelectors::kAsioResyncRequest, value: 0 });
        assert!(!take_reset_request(), "resync is not a reset");
        on_driver_event(AsioDriverEvent::Message { selector: AsioMessageSelectors::kAsioResetRequest, value: 0 });
        assert!(take_reset_request());
        assert!(!take_reset_request(), "consumed");
        on_driver_event(AsioDriverEvent::SampleRateChanged(44_100.0));
        assert!(take_reset_request());
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

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn rate_choice_keeps_requested_when_already_there_or_settable() {
        assert_eq!(choose_rate(48_000.0, 48_000.0, false), 48_000.0);
        assert_eq!(choose_rate(44_100.0, 48_000.0, true), 48_000.0);
        assert_eq!(choose_rate(44_100.0, 48_000.0, false), 44_100.0, "unsupported → driver's own rate");
    }

    #[test]
    fn resolves_only_int32_and_float32_lsb() {
        assert_eq!(resolve_asio_sample_format(&AsioSampleType::ASIOSTInt32LSB), Some(AsioSampleFormat::Int32Lsb));
        assert_eq!(resolve_asio_sample_format(&AsioSampleType::ASIOSTFloat32LSB), Some(AsioSampleFormat::Float32Lsb));
        // Broader format support stays explicitly out of scope.
        assert_eq!(resolve_asio_sample_format(&AsioSampleType::ASIOSTInt16LSB), None);
        assert_eq!(resolve_asio_sample_format(&AsioSampleType::ASIOSTInt24LSB), None);
    }

    #[test]
    fn float32lsb_read_write_is_a_direct_passthrough() {
        let mut buf = [0.0f32; 4];
        let ptr = buf.as_mut_ptr() as *mut c_void;
        unsafe {
            write_asio_sample(ptr, 2, AsioSampleFormat::Float32Lsb, 0.25);
            assert_eq!(read_asio_sample(ptr as *const c_void, 2, AsioSampleFormat::Float32Lsb), 0.25);
        }
        // Confirms it's a direct f32 write (no int conversion happened).
        assert_eq!(buf[2], 0.25);
    }

    #[test]
    fn int32lsb_read_write_matches_existing_conversion_functions() {
        let mut buf = [0i32; 4];
        let ptr = buf.as_mut_ptr() as *mut c_void;
        unsafe {
            write_asio_sample(ptr, 1, AsioSampleFormat::Int32Lsb, 1.0);
            assert_eq!(buf[1], i32::MAX);
            assert_eq!(
                read_asio_sample(ptr as *const c_void, 1, AsioSampleFormat::Int32Lsb),
                asio_i32_to_f32(i32::MAX)
            );
        }
    }

    fn range(min: i32, max: i32, preferred: BufferPreference) -> asio_sys::BufferSizeRange {
        asio_sys::BufferSizeRange { min, max, preferred }
    }

    #[test]
    fn hint_within_stepped_range_and_aligned_is_accepted() {
        let r = range(64, 2048, BufferPreference::Stepped { preferred: 512, step: 64 });
        assert_eq!(hint_fits_buffer_size_range(512, r, "test"), Some(512));
    }

    #[test]
    fn hint_outside_min_max_falls_back_to_none() {
        let r = range(64, 2048, BufferPreference::Preferred(512));
        assert_eq!(hint_fits_buffer_size_range(32, r, "test"), None);
        assert_eq!(hint_fits_buffer_size_range(4096, r, "test"), None);
    }

    #[test]
    fn hint_misaligned_to_step_falls_back_to_none() {
        let r = range(64, 2048, BufferPreference::Stepped { preferred: 512, step: 64 });
        assert_eq!(hint_fits_buffer_size_range(100, r, "test"), None);
    }

    #[test]
    fn hint_power_of_two_in_only_preference_is_accepted() {
        // `asio-sys` maps Steinberg granularity == -1 (powers of 2) to BufferPreference::Only.
        let r = range(64, 2048, BufferPreference::Only(512));
        // Configured size 256 is accepted even though preferred size is 512:
        assert_eq!(hint_fits_buffer_size_range(256, r, "test"), Some(256));
        assert_eq!(hint_fits_buffer_size_range(512, r, "test"), Some(512));
        assert_eq!(hint_fits_buffer_size_range(1024, r, "test"), Some(1024));
        // Non-power-of-two size is rejected and falls back to None:
        assert_eq!(hint_fits_buffer_size_range(300, r, "test"), None);
    }

    #[test]
    fn hint_fixed_size_driver_where_min_equals_max() {
        let r = range(256, 256, BufferPreference::Only(256));
        assert_eq!(hint_fits_buffer_size_range(512, r, "test"), None);
        assert_eq!(hint_fits_buffer_size_range(256, r, "test"), Some(256));
    }
}
