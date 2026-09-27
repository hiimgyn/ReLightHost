use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;
use std::sync::Mutex;
use anyhow::Result;
use ringbuf::{HeapRb, traits::Split};

use crate::audio::types::{AudioStatus, AudioConfig};
use crate::audio::device::AudioDevice;
use crate::audio::vu_meter::VUMeter;
use crate::audio::backend::{asio, wasapi};
use crate::audio::mixer::MixerState;

/// One side of a monitoring session's INPUT leg, in the "bridged" case
/// (every case except full-duplex same-ASIO-device).
enum BridgedInput {
    Asio(asio::AsioDuplexStream),
    // Held only for its `Drop` impl (stops the capture thread) — never read
    // directly, hence the lint below.
    #[allow(dead_code)]
    Wasapi(wasapi::WasapiCaptureStream),
}

/// One side of a monitoring session's OUTPUT leg — either the primary
/// hardware output (bridged case) or the secondary virtual/monitor mirror
/// device.
enum BridgedOutput {
    Asio(asio::AsioDuplexStream),
    // Held only for its `Drop` impl (stops the render thread) — never read
    // directly, hence the lint below.
    #[allow(dead_code)]
    Wasapi(wasapi::WasapiRenderStream),
}

/// Wraps a freshly-started ASIO leg (from `start_duplex`/`start_input_only`/
/// `start_output_only`) for the window between it succeeding and it being
/// safely stored in the final `ActiveBackend` value — i.e. while
/// `toggle_monitoring` may still run OTHER fallible/panicking setup (most
/// notably `backend::wasapi::start_capture`/`start_render`'s
/// `std::thread::spawn`, which panics rather than returning `Err` if the OS
/// refuses to create the thread).
///
/// Without this, a panic unwinding through that later setup would drop an
/// already-successful `AsioDuplexStream` implicitly via plain field-drop
/// glue — WITHOUT `ASIO_LIFECYCLE_LOCK` held — reopening the exact
/// use-after-free/race that lock exists to close (see `backend/asio.rs`'s
/// doc comments). This is the "`Option<Driver>` + locking `Drop`" fallback
/// form `AsioDuplexStream`'s own doc comment already names as the
/// alternative to documentation-only discipline, applied at the point
/// where panic-safety actually needs it instead of inside `AsioDuplexStream`
/// itself (which would need the same treatment for every field).
struct AsioGuard(Option<asio::AsioDuplexStream>);

impl Drop for AsioGuard {
    fn drop(&mut self) {
        if let Some(stream) = self.0.take() {
            asio::stop(stream);
        }
    }
}

impl AsioGuard {
    /// Extracts the guarded stream once it's safe to hand to its final,
    /// permanent home (an `ActiveBackend` about to be stored in
    /// `MonitoringStreams`). Takes the value out of `self.0` FIRST, so the
    /// implicit `Drop` that still runs on `self` at the end of this method
    /// sees `None` and is a safe no-op — no `mem::forget` needed.
    fn into_inner(mut self) -> asio::AsioDuplexStream {
        self.0.take().expect("AsioGuard::into_inner called on an already-emptied guard")
    }
}

/// Mirrors `BridgedInput`, but holds a marker instead of the guarded ASIO
/// stream (which lives in `toggle_monitoring`'s `asio_guard` local until
/// every other panicking setup step has finished — see `AsioGuard`'s doc
/// comment). The WASAPI variant already holds its final, safe-to-drop
/// stream directly since `WasapiCaptureStream` stops safely via its own
/// `Drop` impl and needs no such protection.
enum PendingBridgedInput {
    Asio,
    Wasapi(wasapi::WasapiCaptureStream),
}

/// Output counterpart of [`PendingBridgedInput`].
enum PendingBridgedOutput {
    Asio,
    Wasapi(wasapi::WasapiRenderStream),
}

/// Mirrors `ActiveBackend` during the same panic-risk window described on
/// [`AsioGuard`].
enum PendingBackend {
    AsioDuplex,
    Bridged {
        input: PendingBridgedInput,
        output: Option<PendingBridgedOutput>,
    },
}

/// Which backend combination is currently driving monitoring.
///
/// `AsioDuplex` is the single-driver full-duplex case (same ASIO device for
/// input and output) — one `bufferSwitch` callback serves both directions.
/// `Bridged` is every other case (pure WASAPI, or one ASIO side paired with
/// one WASAPI side) — input and output run as two independent registrations
/// connected by a `ringbuf::HeapRb`. Two different ASIO drivers for
/// input+output is rejected outright before either side is started (see
/// `toggle_monitoring`) — the ASIO SDK only supports one loaded driver per
/// process, so that combination can never reach this enum at all.
///
/// IMPORTANT: `asio::AsioDuplexStream` has no lock-safe `Drop` impl by
/// design (see its doc comment in `backend/asio.rs`) — every path that
/// replaces or discards a value holding one of these variants MUST call
/// `backend::asio::stop()` on it explicitly first. `MonitoringStreams::teardown`
/// is the only place that is allowed to consume this enum for exactly that
/// reason — see its doc comment.
enum ActiveBackend {
    AsioDuplex(asio::AsioDuplexStream),
    Bridged {
        input: BridgedInput,
        output: Option<BridgedOutput>,
    },
}

/// Holds every live stream for the current monitoring session.
struct MonitoringStreams {
    backend: ActiveBackend,
    /// Optional secondary mirror device (e.g. VB-Audio Virtual Cable),
    /// mirroring the already-processed audio from whichever leg above runs
    /// the mixer stage. WASAPI only: ASIO allows only one loaded driver per
    /// process (see `backend/asio.rs`), so this can never itself be a second
    /// ASIO driver while `backend` above is already using one — and
    /// `toggle_monitoring` doesn't track a separate one-driver slot for the
    /// case where `backend` is pure WASAPI either.
    // Held only for its `Drop` impl (stops the render thread) — never read
    // directly, hence the lint below.
    #[allow(dead_code)]
    virtual_output: Option<wasapi::WasapiRenderStream>,
}

impl MonitoringStreams {
    /// Tears down every stream. Calls `backend::asio::stop()` explicitly on
    /// any ASIO-backed leg rather than relying on `Drop` — required by
    /// `AsioDuplexStream`'s documented contract (see `backend/asio.rs`):
    /// letting it drop implicitly would run its teardown WITHOUT
    /// `ASIO_LIFECYCLE_LOCK` held, reopening the exact SDK race that lock
    /// exists to close. WASAPI streams stop safely via their own `Drop`
    /// impl, so simply letting them (and `self.virtual_output`) drop at the
    /// end of this function is correct for them.
    fn teardown(self) {
        let MonitoringStreams { backend, virtual_output: _ } = self;
        match backend {
            ActiveBackend::AsioDuplex(stream) => asio::stop(stream),
            ActiveBackend::Bridged { input, output } => {
                if let BridgedInput::Asio(stream) = input {
                    asio::stop(stream);
                }
                if let Some(BridgedOutput::Asio(stream)) = output {
                    asio::stop(stream);
                }
                // Any `Wasapi(_)` variant, and `output: None`, drop safely here.
            }
        }
        // `virtual_output` (a `WasapiRenderStream`, or `None`) drops safely
        // here too — bound above only to spell out that it's intentionally
        // left untouched, not to keep it alive any longer.
    }
}

/// Signature for the plugin-chain processing callback.
/// Called per audio block with non-interleaved L/R buffers.
/// Mirrors LightHost's AudioProcessorGraph routing: INPUT → chain → OUTPUT.
type ProcessChainFn = Box<dyn Fn(&mut [f32], &mut [f32]) + Send + 'static>;

pub struct AudioManager {
    config:     Arc<RwLock<AudioConfig>>,
    status:     Arc<RwLock<AudioStatus>>,
    last_update: Arc<RwLock<Instant>>,
    monitoring:  Mutex<Option<MonitoringStreams>>,
    /// Plugin chain callback — set by lib.rs after AppState is built.
    process_fn:  Arc<Mutex<Option<ProcessChainFn>>>,
    /// VU meter for output level monitoring
    vu_meter:    Arc<VUMeter>,
    /// Output mute — when true the output callback writes silence.
    muted:       Arc<AtomicBool>,
    /// Loopback — when true, captures system output and mixes into the output.
    loopback_enabled: Arc<AtomicBool>,
    /// Real DSP load percentage stored as f32 bits (updated each audio block).
    dsp_load_u32: Arc<AtomicU32>,
    /// Serializes concurrent config changes (device/sample-rate/buffer-size)
    /// to prevent interleaved stop/start cycles from leaving monitoring undefined.
    config_lock: Mutex<()>,
    /// Cumulative ring-buffer underrun counter (resets on stream restart —
    /// see `toggle_monitoring`). Shared via `build_mixer_state` into
    /// `MixerState::underrun_count`, and incremented by the real-time
    /// consumer loops that drain a ring buffer into the mixer's output
    /// stage (`backend::asio::start_output_only`, `backend::wasapi::start_render`)
    /// whenever `try_pop()` misses.
    underrun_count: Arc<AtomicU64>,
    /// Whether the active WASAPI leg (see `get_status`) negotiated exclusive
    /// mode. Updated only at `toggle_monitoring` time.
    exclusive_mode_active: Arc<RwLock<bool>>,
    /// Set when a WASAPI leg fell back from exclusive to shared mode.
    /// Updated only at `toggle_monitoring` time.
    wasapi_fallback_reason: Arc<RwLock<Option<String>>>,
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            config:           Arc::new(RwLock::new(AudioConfig::default())),
            status:           Arc::new(RwLock::new(AudioStatus::default())),
            last_update:      Arc::new(RwLock::new(Instant::now())),
            monitoring:       Mutex::new(None),
            process_fn:       Arc::new(Mutex::new(None)),
            vu_meter:         Arc::new(VUMeter::new()),
            muted:            Arc::new(AtomicBool::new(false)),
            loopback_enabled: Arc::new(AtomicBool::new(false)),
            dsp_load_u32:     Arc::new(AtomicU32::new(0)),
            config_lock:      Mutex::new(()),
            underrun_count:   Arc::new(AtomicU64::new(0)),
            exclusive_mode_active: Arc::new(RwLock::new(false)),
            wasapi_fallback_reason: Arc::new(RwLock::new(None)),
        }
    }

    /// Builds a fresh `MixerState` sharing this manager's process/VU/mute/
    /// loopback/DSP-load state — the same fields every backend's output leg
    /// needs, differing only in `output_is_asio` (which flips the mute/
    /// loopback gate polarity — see `mixer::main_output_gate_open`).
    fn build_mixer_state(&self, output_is_asio: bool) -> MixerState {
        MixerState {
            process_fn: Arc::clone(&self.process_fn),
            vu_meter: Arc::clone(&self.vu_meter),
            muted: Arc::clone(&self.muted),
            loopback_enabled: Arc::clone(&self.loopback_enabled),
            dsp_load_u32: Arc::clone(&self.dsp_load_u32),
            output_is_asio,
            underrun_count: Arc::clone(&self.underrun_count),
        }
    }

    /// Register the plugin-chain callback.
    pub fn set_process_callback<F>(&self, f: F)
    where
        F: Fn(&mut [f32], &mut [f32]) + Send + 'static,
    {
        *self.process_fn.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
    }

    /// Restore a previously saved AudioConfig without restarting any running streams.
    /// Call this during startup before calling toggle_monitoring.
    pub fn restore_config(&self, config: AudioConfig) {
        {
            let mut status = self.status.write();
            status.sample_rate = config.sample_rate;
            status.buffer_size = config.buffer_size;
            status.latency_ms = (config.buffer_size as f32 / config.sample_rate as f32) * 1000.0;
        }
        *self.config.write() = config;
    }

    /// Start audio engine
    pub fn start(&self) -> Result<()> {
        let config = self.config.read().clone();

        {
            let mut status = self.status.write();
            status.sample_rate = config.sample_rate;
            status.buffer_size = config.buffer_size;
            status.latency_ms = (config.buffer_size as f32 / config.sample_rate as f32) * 1000.0;
        }

        *self.last_update.write() = Instant::now();

        log::info!("{} Audio engine started: {}Hz, {} samples", crate::core::threading::thread_prefix("audio/engine"), config.sample_rate, config.buffer_size);
        Ok(())
    }

    /// Stop audio engine
    pub fn stop(&self) -> Result<()> {
        self.toggle_monitoring(false)?;
        let mut status = self.status.write();
        status.is_monitoring = false;
        status.cpu_usage = 0.0;

        log::info!("{} Audio engine stopped", crate::core::threading::thread_prefix("audio/engine"));
        Ok(())
    }

    /// Toggle real-time input monitoring (routes input device → plugin chain → output device).
    ///
    /// # ASIO note
    /// ASIO is full-duplex: input and output are driven by a single driver
    /// callback at the exact same buffer size. When the same ASIO device is
    /// selected for both input and output, `backend::asio::start_duplex`
    /// runs a single registration covering both directions. Two DIFFERENT
    /// ASIO drivers for input+output is rejected outright — the ASIO SDK
    /// only supports one loaded driver per process (see `backend/asio.rs`'s
    /// `ASIO_LIFECYCLE_LOCK` doc comment); attempting it would tear down
    /// whichever driver loaded first out from under its running callback.
    /// Every other combination (WASAPI on both sides, or one ASIO side
    /// paired with one WASAPI side) runs as two independent registrations
    /// bridged through a `ringbuf::HeapRb`.
    ///
    /// The ring buffer capacity is set to 4× the configured buffer size
    /// (stereo samples) for same-device ASIO, 8× otherwise — enough for a
    /// couple of full blocks without adding noticeable latency.
    pub fn toggle_monitoring(&self, enabled: bool) -> Result<()> {
        if !enabled {
            let mut monitoring_guard = self.monitoring.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(streams) = monitoring_guard.take() {
                streams.teardown();
            }
            *self.exclusive_mode_active.write() = false;
            *self.wasapi_fallback_reason.write() = None;
            self.status.write().is_monitoring = false;
            log::info!("{} Input monitoring stopped", crate::core::threading::thread_prefix("audio/monitor"));
            return Ok(());
        }

        // Hold the monitoring mutex for the entire setup so that a second call
        // (e.g. React StrictMode double-effect) cannot race and create duplicate
        // streams — which caused STATUS_ACCESS_VIOLATION when the first set of
        // streams was dropped mid-callback.
        let mut monitoring_guard = self.monitoring.lock().unwrap_or_else(|e| e.into_inner());
        if monitoring_guard.is_some() {
            log::info!("{} Audio stream already running — ignoring duplicate start", crate::core::threading::thread_prefix("audio/monitor"));
            return Ok(());
        }

        let config = self.config.read().clone();

        // -----------------------------------------------------------------
        // Same-ASIO-device (full-duplex insert, e.g. a Voicemeeter insert)
        // detection, and the cross-driver rejection this task adds. Both
        // operate on the RAW configured device ids (user intent) — NOT on
        // whatever a later per-leg resolution/fallback below might actually
        // resolve to, since that's a separate, per-leg concern (see the
        // `else` branch).
        // -----------------------------------------------------------------
        let input_is_asio = config.input_device_id.as_deref()
            .map(|id| id.starts_with("asio_")).unwrap_or(false);
        let output_is_asio = config.output_device_id.as_deref()
            .map(|id| id.starts_with("asio_")).unwrap_or(false);

        let in_asio_name  = config.input_device_id.as_deref().and_then(|id| id.strip_prefix("asio_"));
        let out_asio_name = config.output_device_id.as_deref().and_then(|id| id.strip_prefix("asio_"));
        let same_asio_device = input_is_asio && output_is_asio && in_asio_name == out_asio_name;

        // Correction found during Task 5's review, binding on this task: the
        // real ASIO SDK only supports ONE loaded driver per process — a
        // second, different ASIO driver for the other direction would tear
        // the first one's buffers out from under its running callback. This
        // is a real use-after-free, not a rare inconvenience a ring buffer
        // can bridge around, so it's rejected before starting anything.
        if input_is_asio && output_is_asio && !same_asio_device {
            return Err(anyhow::anyhow!(
                "ASIO does not support using two different ASIO drivers at the same time for input and output. Select the same ASIO device for both, or pair an ASIO device with a WASAPI device."
            ));
        }

        // Enumerated once and reused for every channel-count/existence
        // lookup below (including inside `AudioDevice::find_input_device`/
        // `find_output_device`/`find_asio_device_pair`, which all take this
        // cached list rather than re-enumerating themselves) —
        // `list_asio_devices()` briefly loads every registered driver to
        // query channels (see its doc comment), so caching it here avoids
        // repeating that expensive work multiple times in a single
        // `toggle_monitoring` call, and avoids the window (per Task 5's
        // known limitation) where re-enumerating temporarily excludes
        // whatever driver is currently streaming from the list. Computed up
        // front, before the virtual-mirror resolution below, so that path
        // can use it too.
        let asio_devices = if input_is_asio || output_is_asio {
            asio::list_asio_devices()
        } else {
            Vec::new()
        };

        // -----------------------------------------------------------------
        // Lock-free SPSC ring buffer capacity — stereo, sized by mode:
        //  • Same-ASIO full-duplex: input fires synchronously before output
        //    within the same bufferSwitch → 2 stereo frames is enough.
        //    Keep a small margin (4×) to absorb any block-size discrepancy.
        //  • Cross-device (WASAPI or ASIO+WASAPI): clocks can drift; keep
        //    the existing 8× safety margin.
        // -----------------------------------------------------------------
        let buf_capacity = if same_asio_device {
            (config.buffer_size as usize).max(2048) * 4 * 2  // 4 frames × stereo
        } else {
            (config.buffer_size as usize).max(4096) * 8 * 2  // 8 frames × stereo
        };

        self.underrun_count.store(0, Ordering::Relaxed);

        // -----------------------------------------------------------------
        // Virtual/monitor output mirror (e.g. VB-Audio Virtual Cable).
        // Resolved up front so its ring-buffer producer half can be threaded
        // into whichever leg below actually runs the mixer stage — the
        // consumer half is only used AFTER that leg starts successfully.
        //
        // WASAPI only: ASIO allows only one loaded driver per process (see
        // `backend/asio.rs`), so this can never itself be a second ASIO
        // driver while the primary path below is already using one, and
        // this function doesn't track a separate one-driver slot for the
        // case where the primary path is pure WASAPI either.
        // ponytail: WASAPI-only virtual mirror — add ASIO support here
        // (sharing `start_output_only`'s one-driver bookkeeping) if ever needed.
        // -----------------------------------------------------------------
        let virt_wasapi_id: Option<String> = config.virtual_output_device_id.as_deref().and_then(|id| {
            if id.starts_with("asio_") {
                log::warn!(
                    "{} Virtual output device '{}' is an ASIO device; ASIO virtual monitor mirrors are not supported (only one ASIO driver can be loaded per process). Skipping.",
                    crate::core::threading::thread_prefix("audio/monitor"), id
                );
                return None;
            }
            match AudioDevice::find_output_device(id, &asio_devices) {
                Some(resolved) => Some(resolved.strip_prefix("out_").unwrap_or(&resolved).to_string()),
                None => {
                    log::warn!("{} Virtual output device '{}' not found; skipping", crate::core::threading::thread_prefix("audio/monitor"), id);
                    None
                }
            }
        });

        let (virt_producer, mut virt_consumer) = if virt_wasapi_id.is_some() {
            let virt_rb = HeapRb::<f32>::new(buf_capacity);
            let (p, c) = virt_rb.split();
            (Some(p), Some(c))
        } else {
            (None, None)
        };

        // Populated by whichever WASAPI leg(s) actually start below, used to
        // fill the two additive `AudioStatus` fields. Output takes priority
        // over input when both happen to be WASAPI (exclusive mode matters
        // most for the leg the user actually listens to). Left at `None`
        // for the full-duplex ASIO branch (no WASAPI leg exists there).
        let mut input_wasapi_result: Option<wasapi::ExclusiveModeResult> = None;
        let mut output_wasapi_result: Option<wasapi::ExclusiveModeResult> = None;

        // Holds a freshly-started ASIO leg (at most one ever exists per
        // call — enforced by the cross-driver rejection above plus the
        // bridged branch's own "at most one side is ASIO" invariant) until
        // every other fallible/panicking setup step below has finished —
        // see `AsioGuard`'s doc comment for why.
        let mut asio_guard: Option<AsioGuard> = None;

        let pending: PendingBackend = if same_asio_device {
            // ---------------------------------------------------------
            // Full-duplex insert mode: one driver, one callback, both
            // directions. Failure anywhere here is fatal — there's no
            // partial/degraded mode for a single combined stream.
            // ---------------------------------------------------------
            let asio_name = in_asio_name
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("ASIO insert mode requires a device name after 'asio_' prefix"))?;
            let _ = AudioDevice::find_asio_device_pair(asio_name, &asio_devices)
                .ok_or_else(|| anyhow::anyhow!("ASIO device '{}' not found for insert mode", asio_name))?;

            let (in_channels, out_channels) = asio_devices.iter()
                .find(|d| d.name == asio_name)
                .map(|d| (d.input_channels, d.output_channels))
                .unwrap_or((2, 2));
            // Clamp the configured channel pair to what the device actually
            // has — an offset saved for a different (wider) interface must
            // not be requested against a narrower one.
            let in_offset  = if in_channels  >= 2 { config.input_channel_offset.min(in_channels - 2) } else { 0 };
            let out_offset = if out_channels >= 2 { config.output_channel_offset.min(out_channels - 2) } else { 0 };

            let mixer_state = self.build_mixer_state(true);
            let stream = asio::start_duplex(
                asio_name,
                in_offset,
                out_offset,
                Some(config.buffer_size as i32),
                mixer_state,
                virt_producer,
            ).map_err(|e| anyhow::anyhow!("Failed to start ASIO full-duplex stream: {e}"))?;
            asio_guard = Some(AsioGuard(Some(stream)));
            PendingBackend::AsioDuplex
        } else {
            // ---------------------------------------------------------
            // Bridged: at most one side is ASIO (guaranteed by the
            // cross-driver rejection above — same name would have taken
            // the `same_asio_device` branch instead). Input failing is
            // fatal (monitoring needs an input); output failing is
            // NOT fatal — matches the old cpal-based code's behavior of
            // continuing monitoring without hardware output. This also
            // means an ASIO leg is never left loaded-but-undiscarded on an
            // `Err`-return path here: once an ASIO leg succeeds, nothing
            // after it in this branch can return `Err` from
            // `toggle_monitoring` — and `asio_guard` now covers the
            // remaining panic-unwind case too (see its doc comment).
            // ---------------------------------------------------------
            let rb = HeapRb::<f32>::new(buf_capacity);
            let (producer, consumer) = rb.split();

            let input_id = config.input_device_id.as_deref()
                .and_then(|id| AudioDevice::find_input_device(id, &asio_devices))
                .or_else(AudioDevice::default_input_device_id)
                .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

            let input = if let Some(name) = input_id.strip_prefix("asio_") {
                let in_channels = asio_devices.iter()
                    .find(|d| d.name == name).map(|d| d.input_channels).unwrap_or(2);
                let in_offset = if in_channels >= 2 { config.input_channel_offset.min(in_channels - 2) } else { 0 };
                let stream = asio::start_input_only(name, in_offset, Some(config.buffer_size as i32), producer)
                    .map_err(|e| anyhow::anyhow!("Failed to start ASIO input '{name}': {e}"))?;
                asio_guard = Some(AsioGuard(Some(stream)));
                PendingBridgedInput::Asio
            } else {
                let raw = input_id.strip_prefix("in_").unwrap_or(&input_id);
                let (stream, result) = wasapi::start_capture(raw, config.buffer_size, config.sample_rate, producer)
                    .map_err(|e| anyhow::anyhow!("Failed to start WASAPI input '{raw}': {e}"))?;
                input_wasapi_result = Some(result);
                PendingBridgedInput::Wasapi(stream)
            };

            // If the user explicitly set output_device_id to None, do NOT
            // fall back to the system default — treat it as "no hardware
            // out configured" (unchanged from the old cpal-based behavior).
            let output_target: Option<String> = if config.output_device_id.is_some() {
                config.output_device_id.as_deref()
                    .and_then(|id| AudioDevice::find_output_device(id, &asio_devices))
                    .or_else(AudioDevice::default_output_device_id)
            } else {
                None
            };
            let output_leg_is_asio = output_target.as_deref().map(|id| id.starts_with("asio_")).unwrap_or(false);
            let mixer_state = self.build_mixer_state(output_leg_is_asio);

            let output = match output_target {
                Some(ref id) if id.starts_with("asio_") => {
                    let name = id.strip_prefix("asio_").unwrap_or(id.as_str());
                    let out_channels = asio_devices.iter()
                        .find(|d| d.name == name).map(|d| d.output_channels).unwrap_or(2);
                    let out_offset = if out_channels >= 2 { config.output_channel_offset.min(out_channels - 2) } else { 0 };
                    match asio::start_output_only(name, out_offset, Some(config.buffer_size as i32), consumer, mixer_state, virt_producer) {
                        Ok(stream) => {
                            asio_guard = Some(AsioGuard(Some(stream)));
                            Some(PendingBridgedOutput::Asio)
                        }
                        Err(e) => {
                            log::warn!("{} Failed to start ASIO output '{name}': {e}; continuing without hardware output", crate::core::threading::thread_prefix("audio/monitor"));
                            None
                        }
                    }
                }
                Some(ref id) => {
                    let raw = id.strip_prefix("out_").unwrap_or(id.as_str());
                    match wasapi::start_render(raw, config.buffer_size, config.sample_rate, consumer, mixer_state, virt_producer) {
                        Ok((stream, result)) => {
                            output_wasapi_result = Some(result);
                            Some(PendingBridgedOutput::Wasapi(stream))
                        }
                        Err(e) => {
                            log::warn!("{} Failed to start WASAPI output '{raw}': {e}; continuing without hardware output", crate::core::threading::thread_prefix("audio/monitor"));
                            None
                        }
                    }
                }
                None => None,
            };

            PendingBackend::Bridged { input, output }
        };

        // -----------------------------------------------------------------
        // Virtual/monitor output — starts AFTER the primary backend(s), and
        // never fatal (matches the old cpal-based code's identical
        // "warn and continue without it" treatment of this device).
        // -----------------------------------------------------------------
        let virtual_output = match (virt_wasapi_id, virt_consumer.take()) {
            (Some(id), Some(consumer)) => {
                // A pass-through `MixerState`: no plugin chain (`process_fn`
                // stays `None`, so `process_block` only relays the
                // already-processed samples this consumer receives), and a
                // gate that's always open (`output_is_asio: false` +
                // `loopback_enabled: true` constant → `main_output_gate_open`
                // returns `true` unconditionally) — this device should
                // simply play back whatever the primary leg's mixer stage
                // already decided to mirror to it, with no further gating
                // or re-processing. Uses its own throwaway VU meter/DSP-load
                // counter so this second pass doesn't smear the real ones.
                let passthrough_mixer = MixerState {
                    process_fn: Arc::new(Mutex::new(None)),
                    vu_meter: Arc::new(VUMeter::new()),
                    muted: Arc::new(AtomicBool::new(false)),
                    loopback_enabled: Arc::new(AtomicBool::new(true)),
                    dsp_load_u32: Arc::new(AtomicU32::new(0)),
                    output_is_asio: false,
                    // Throwaway counter — see this literal's other throwaway
                    // fields above; this second pass's underruns shouldn't
                    // smear the real one's count.
                    underrun_count: Arc::new(AtomicU64::new(0)),
                };
                match wasapi::start_render(&id, config.buffer_size, config.sample_rate, consumer, passthrough_mixer, None) {
                    Ok((stream, _result)) => Some(stream),
                    Err(e) => {
                        log::warn!("{} Failed to start virtual output stream: {e}; continuing without it", crate::core::threading::thread_prefix("audio/monitor"));
                        None
                    }
                }
            }
            _ => None,
        };

        // -----------------------------------------------------------------
        // Final extraction: every other fallible/panicking setup step (the
        // output leg, the virtual-mirror leg) has now finished, so it's
        // safe to move the guarded ASIO stream (if any) into its permanent
        // home. `AsioGuard::into_inner` takes the stream out before its own
        // `Drop` runs, so this never double-stops anything on this normal
        // path — see `AsioGuard`'s doc comment.
        // -----------------------------------------------------------------
        let backend: ActiveBackend = match pending {
            PendingBackend::AsioDuplex => {
                let stream = asio_guard.take()
                    .expect("PendingBackend::AsioDuplex without a guarded ASIO stream")
                    .into_inner();
                ActiveBackend::AsioDuplex(stream)
            }
            PendingBackend::Bridged { input, output } => {
                let input = match input {
                    PendingBridgedInput::Asio => BridgedInput::Asio(
                        asio_guard.take()
                            .expect("PendingBridgedInput::Asio without a guarded ASIO stream")
                            .into_inner(),
                    ),
                    PendingBridgedInput::Wasapi(stream) => BridgedInput::Wasapi(stream),
                };
                let output = match output {
                    Some(PendingBridgedOutput::Asio) => Some(BridgedOutput::Asio(
                        asio_guard.take()
                            .expect("PendingBridgedOutput::Asio without a guarded ASIO stream")
                            .into_inner(),
                    )),
                    Some(PendingBridgedOutput::Wasapi(stream)) => Some(BridgedOutput::Wasapi(stream)),
                    None => None,
                };
                ActiveBackend::Bridged { input, output }
            }
        };

        let (resolved_exclusive, resolved_fallback_reason) = output_wasapi_result
            .as_ref()
            .or(input_wasapi_result.as_ref())
            .map(|r| (r.exclusive, r.fallback_reason.clone()))
            .unwrap_or((false, None));
        *self.exclusive_mode_active.write() = resolved_exclusive;
        *self.wasapi_fallback_reason.write() = resolved_fallback_reason;

        let has_virt = virtual_output.is_some();
        *monitoring_guard = Some(MonitoringStreams { backend, virtual_output });
        self.status.write().is_monitoring = true;
        log::info!(
            "{} Input monitoring started ({}Hz, {} samples{})",
            crate::core::threading::thread_prefix("audio/monitor"),
            config.sample_rate,
            config.buffer_size,
            if has_virt { " + hardware out" } else { "" },
        );
        Ok(())
    }

    /// Set output mute state.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        log::info!("{} Audio output {}", crate::core::threading::thread_prefix("audio/control"), if muted { "muted" } else { "unmuted" });
    }

    /// Get current mute state.
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// Enable or disable monitoring through Hardware Out.
    /// Takes effect immediately — the output callback is gated by this flag,
    /// so no stream restart is needed and toggling is glitch-free.
    pub fn set_loopback(&self, enabled: bool) -> Result<()> {
        self.loopback_enabled.store(enabled, Ordering::Relaxed);
        log::info!("{} Hardware Out monitoring {}", crate::core::threading::thread_prefix("audio/control"), if enabled { "enabled" } else { "disabled" });
        Ok(())
    }

    /// Get current loopback state.
    pub fn is_loopback_enabled(&self) -> bool {
        self.loopback_enabled.load(Ordering::Relaxed)
    }

    /// Get current audio status
    pub fn get_status(&self) -> AudioStatus {
        let dsp_load = f32::from_bits(self.dsp_load_u32.load(Ordering::Relaxed));
        self.status.write().cpu_usage = dsp_load;
        let mut status = self.status.read().clone();
        status.is_muted = self.muted.load(Ordering::Relaxed);
        status.loopback_enabled = self.loopback_enabled.load(Ordering::Relaxed);
        status.underrun_count = self.underrun_count.load(Ordering::Relaxed);
        status.vst3_settling = crate::plugins::processor::vst3::is_vst3_settling();
        status.exclusive_mode_active = *self.exclusive_mode_active.read();
        status.wasapi_fallback_reason = self.wasapi_fallback_reason.read().clone();
        status
    }

    /// Get current audio configuration
    pub fn get_config(&self) -> AudioConfig {
        self.config.read().clone()
    }

    /// Get current VU meter data
    pub fn get_vu_data(&self) -> crate::audio::vu_meter::VUData {
        self.vu_meter.get_data()
    }

    /// Set output device
    pub fn set_output_device(&self, device_id: Option<String>) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().output_device_id = device_id;
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set input device
    pub fn set_input_device(&self, device_id: Option<String>) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().input_device_id = device_id;
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set the input channel pair (0-based index of the first channel).
    /// Only meaningful for multi-channel (ASIO) devices; WASAPI legs always
    /// use channels 0/1 (or duplicate a mono channel to both), matching the
    /// backend's own capture/render API.
    pub fn set_input_channel_offset(&self, offset: usize) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().input_channel_offset = offset;
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set the output channel pair (0-based index of the first channel).
    pub fn set_output_channel_offset(&self, offset: usize) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().output_channel_offset = offset;
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set virtual output device (e.g. VB-Audio Virtual Cable / VAIO).
    /// When non-None, processed audio is mirrored to this device alongside
    /// the primary output — useful for routing to OBS / Discord while still
    /// monitoring through speakers or headphones.
    pub fn set_virtual_output_device(&self, device_id: Option<String>) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().virtual_output_device_id = device_id;
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set sample rate
    pub fn set_sample_rate(&self, rate: u32) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().sample_rate = rate;
        {
            let mut status = self.status.write();
            status.sample_rate = rate;
            status.latency_ms = (status.buffer_size as f32 / rate as f32) * 1000.0;
        }
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set buffer size
    pub fn set_buffer_size(&self, size: u32) -> Result<()> {
        let _guard = self.config_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.config.write().buffer_size = size;
        {
            let mut status = self.status.write();
            status.buffer_size = size;
            status.latency_ms = (size as f32 / status.sample_rate as f32) * 1000.0;
        }
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }
}

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}
