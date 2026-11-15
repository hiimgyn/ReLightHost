use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};
use crate::plugins::types::{PluginInfo, PluginInstanceInfo, PluginParameter, PluginFormat};
use crate::plugins::processor::clap::ClapProcessor;
use crate::plugins::processor::vst2::Vst2Processor;
use crate::plugins::processor::vst3::Vst3Processor;
use relight_addon_api::{AddonDescriptor, AddonProcessor, AddonStatus};
use crate::plugins::crash_protection::{self, SharedCrashProtection};
use anyhow::{Error, Result};
use rayon::prelude::*;

/// Upper bound a non-audio caller waits for a processor mutex the audio
/// callback holds for the duration of one block.
const STATE_LOCK_TIMEOUT: Duration = Duration::from_millis(50);

/// Represents a loaded plugin instance with an optional real audio processor.
///
/// Mirrors LightHost's per-node model in AudioProcessorGraph:
///   instance_id  ↔  node id
///   processor.process_stereo  ↔  AudioPlugin::processBlock
///   processor.get_state/set_state  ↔  getStateInformation/setStateInformation
pub struct PluginInstance {
    instance_id:    String,
    plugin_info:    PluginInfo,
    /// User-set display name; None means use plugin_info.name.
    display_name:   RwLock<Option<String>>,
    bypassed:       Arc<AtomicBool>,
    parameters:     Arc<RwLock<Vec<PluginParameter>>>,
    /// VST3 audio processor (vst3-rs), in-process. Present when format == VST3.
    vst3_processor: Mutex<Option<Vst3Processor>>,
    /// VST2 audio processor (vst-rs) — present when format == VST.
    vst2_processor: Mutex<Option<Vst2Processor>>,
    /// CLAP audio processor — present when format == CLAP.
    clap_processor: Mutex<Option<ClapProcessor>>,
    /// Add-on descriptor — present when format == Builtin and the id is known.
    addon: Option<&'static AddonDescriptor>,
    /// Add-on DSP. Only the audio thread (try_lock), un-bypass reset and
    /// Drop take this lock; params, meters and latency go through the
    /// atomics below so a control thread never makes the audio thread skip a block.
    addon_proc: Mutex<Option<Box<dyn AddonProcessor>>>,
    /// Latest value per param id (f32 bits); `param_dirty` bit i = id i changed.
    param_values: [AtomicU32; 64],
    param_dirty: AtomicU64,
    /// Meter values (f32 bits) written by the audio thread after each block.
    meter_values: [AtomicU32; 8],
    addon_latency: AtomicU32,
    /// `AddonStatus` packed by `pack_status`, published after each block.
    addon_status: AtomicU64,
    /// Last state blob successfully read from (or written to) the plugin.
    /// Returned by `get_state_binary` when a live read isn't possible right
    /// now (processor busy) so an
    /// autosave never overwrites real state with nothing.
    last_state:     RwLock<Option<Vec<u8>>>,
    /// Track if GUI window is currently open (prevents multiple windows)
    gui_open:       Arc<AtomicBool>,
    /// HWND (as isize) of the open GUI window; 0 when none.
    /// Set by the GUI thread after CreateWindowExW, cleared on exit.
    /// Lets Drop post WM_CLOSE so the GUI thread finishes before
    /// Vst3Processor::drop calls terminate() — prevents STATUS_ACCESS_VIOLATION.
    gui_hwnd:       Arc<AtomicIsize>,
    /// Crash protection state
    crash_protection: SharedCrashProtection,
    /// Set by the audio thread when the crash limit is hit; the event is
    /// emitted from a normal thread (see `get_crash_statuses`).
    crash_notice_pending: AtomicBool,
    /// Before/after level history for the plugin GUI's waveform.
    scope: crate::plugins::core::scope::Scope,
}

impl PluginInstance {
    /// Create a new plugin instance.
    ///
    /// `sample_rate` and `block_size` are used to initialize the VST3 audio
    /// processor — matching LightHost's `deviceManager.initialise` values passed
    /// to `formatManager.createPluginInstance`.
    pub fn new(plugin_info: PluginInfo, sample_rate: f64, block_size: usize) -> Result<Self> {
        let instance_id = format!("instance_{}", next_instance_id());

        log::info!("{} Creating plugin instance: {} ({})", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, instance_id);

        // Load the appropriate audio processor for the plugin format.
        // Failure is non-fatal; the instance still works in pass-through mode
        // and shows the error on its card (`load_error`).
        // Every plugin is created on the plugin host thread: JUCE binds its
        // message thread to the creating thread, and CLAP init/activate are
        // main-thread calls (see core::host_thread).
        let mut load_error: Option<String> = None;
        let vst3_processor = if plugin_info.format == PluginFormat::VST3 {
            // Marked active BEFORE the call, not after a successful return:
            // the whole point of this marker is to survive a *native* crash
            // (access violation, heap corruption) inside Vst3Processor::load()
            // itself — createInstance/initialize() are exactly the calls a
            // fragile plugin can crash inside, and a crash there never
            // reaches an `Ok(proc) =>` arm to mark itself active after the
            // fact. A clean `Err` (ordinary load failure, not a crash) clears
            // the marker right back below — only a crash leaves it stuck for
            // take_unclean_exit_plugins() to find on next launch.
            crate::core::crash_marker::mark_active(&plugin_info.path);
            match on_host(|| Vst3Processor::load_class(&plugin_info.path, plugin_info.sub_index, sample_rate, block_size)) {
                Ok(proc) => {
                    log::info!("{} VST3 processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("{} VST3 audio processor failed for '{}': {}", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, e);
                    crate::core::crash_marker::mark_inactive(&plugin_info.path);
                    load_error = Some(e.to_string());
                    None
                }
            }
        } else {
            None
        };

        let vst2_processor = if plugin_info.format == PluginFormat::VST {
            match on_host(|| Vst2Processor::load(&plugin_info.path, sample_rate, block_size)) {
                Ok(proc) => {
                    log::info!("{} VST2 processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("{} VST2 audio processor failed for '{}': {}", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, e);
                    load_error = Some(e.to_string());
                    None
                }
            }
        } else {
            None
        };

        let clap_processor = if plugin_info.format == PluginFormat::CLAP {
            match on_host(|| ClapProcessor::load_index(&plugin_info.path, plugin_info.sub_index, sample_rate, block_size)) {
                Ok(proc) => {
                    log::info!("{} CLAP processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("{} CLAP audio processor failed for '{}': {}", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, e);
                    load_error = Some(e.to_string());
                    None
                }
            }
        } else {
            None
        };

        let addon = if plugin_info.format == PluginFormat::Builtin {
            crate::plugins::addons::find(&plugin_info.path)
        } else {
            None
        };
        let initial_params = addon.map(crate::plugins::addons::initial_params).unwrap_or_default();
        let param_values: [AtomicU32; 64] = std::array::from_fn(|_| AtomicU32::new(0));
        let addon_proc = addon.and_then(|d| {
            let mut proc = (d.create)(sample_rate as f32);
            if proc.is_some() {
                log::info!("{} Built-in processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
            } else {
                log::warn!("{} Built-in '{}' ({}) unavailable at {} Hz", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, plugin_info.path, sample_rate);
            }
            // Defaults go straight to the processor (nothing dirty) so it matches the frontend.
            for (i, spec) in d.params.iter().enumerate() {
                param_values[i].store(spec.default.to_bits(), Ordering::Relaxed);
                if let Some(ref mut p) = proc {
                    p.set_parameter(spec.id, spec.default);
                }
            }
            proc
        });
        let addon_latency = addon_proc.as_ref().map_or(0, |p| p.latency_samples());
        let addon_status = addon_proc.as_ref().map_or(0, |p| pack_status(p.status()));

        let crash_protection = crash_protection::create_shared();
        if plugin_info.format == PluginFormat::Builtin && addon_proc.is_none() {
            // NoiseSuppressorGui and DeepFilterNetGui key their rate alert on "needs 48000 Hz".
            crash_protection.lock().status = crash_protection::PluginStatus::Error(format!(
                "'{}' cannot run at {} Hz (needs 48000 Hz) — passing audio through",
                plugin_info.name, sample_rate
            ));
        }
        if let Some(err) = load_error {
            crash_protection.lock().status =
                crash_protection::PluginStatus::Error(load_error_message(&plugin_info, &err));
        }

        Ok(Self {
            instance_id,
            plugin_info,
            display_name:      RwLock::new(None),
            bypassed:          Arc::new(AtomicBool::new(false)),
            parameters:        Arc::new(RwLock::new(initial_params)),
            vst3_processor:    Mutex::new(vst3_processor),
            vst2_processor:    Mutex::new(vst2_processor),
            clap_processor:    Mutex::new(clap_processor),
            addon,
            addon_proc:        Mutex::new(addon_proc),
            param_values,
            param_dirty:       AtomicU64::new(0),
            meter_values:      std::array::from_fn(|_| AtomicU32::new(0)),
            addon_latency:     AtomicU32::new(addon_latency),
            addon_status:      AtomicU64::new(addon_status),
            last_state:        RwLock::new(None),
            gui_open:          Arc::new(AtomicBool::new(false)),
            gui_hwnd:          Arc::new(AtomicIsize::new(0)),
            crash_protection,
            crash_notice_pending: AtomicBool::new(false),
            scope: crate::plugins::core::scope::Scope::new((sample_rate / 100.0).round() as usize),
        })
    }

    /// Get instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Set bypass state. Un-bypassing an add-on resets it first (control
    /// thread; the audio thread doesn't take the lock while bypassed) so
    /// audio queued before the bypass is not replayed.
    pub fn set_bypassed(&self, bypassed: bool) {
        if !bypassed && self.is_bypassed() {
            // ponytail: a lock timeout un-bypasses without a reset (stale audio
            // can replay once); upgrade: flag the reset and let the audio
            // thread run it at its next lock.
            if let Some(mut guard) = self.addon_proc.try_lock_for(STATE_LOCK_TIMEOUT) {
                if let Some(proc) = guard.as_mut() {
                    proc.reset();
                }
            }
        }
        self.bypassed.store(bypassed, Ordering::Release);
    }

    /// Check if bypassed — lock-free, cheap enough to call every audio block.
    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Acquire)
    }

    pub fn request_close_gui(&self, timeout: Duration) -> bool {
        if !self.gui_open.load(Ordering::Acquire) {
            return true;
        }

        let mut close_sent = false;
        let deadline = Instant::now() + timeout;
        while self.gui_open.load(Ordering::Acquire) && Instant::now() < deadline {
            if !close_sent {
                let hwnd = self.gui_hwnd.load(Ordering::Acquire);
                if hwnd != 0 {
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use windows_sys::Win32::Foundation::HWND;
                        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
                        PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
                    }
                    close_sent = true;
                }
            }
            crate::core::host_thread::wait_a_moment();
        }

        !self.gui_open.load(Ordering::Acquire)
    }

    /// Process a stereo buffer in-place through the VST3 plugin.
    ///
    /// Mirrors LightHost's chain: INPUT → plugin → OUTPUT.
    /// Uses try_lock so the audio callback never blocks waiting for the mutex.
    /// Wrapped with crash protection to prevent plugin crashes from taking down the app.
    pub fn process_stereo(&self, left: &mut [f32], right: &mut [f32]) {
        self.scope.around(left, right, |left, right| self.process_inner(left, right));
    }

    /// Delay this plugin adds to the chain, in samples (0 when bypassed —
    /// bypass passes audio straight through).
    pub fn latency_samples(&self) -> u32 {
        if self.is_bypassed() {
            return 0;
        }
        let t = STATE_LOCK_TIMEOUT;
        let reported = match self.plugin_info.format {
            PluginFormat::VST3 => self.vst3_processor.try_lock_for(t).and_then(|g| g.as_ref().map(|p| p.latency_samples())),
            PluginFormat::VST => self.vst2_processor.try_lock_for(t).and_then(|g| g.as_ref().map(|p| p.latency_samples())),
            PluginFormat::CLAP => self.clap_processor.try_lock_for(t).and_then(|g| g.as_ref().map(|p| p.latency_samples())),
            PluginFormat::Builtin => Some(self.addon_latency.load(Ordering::Relaxed)),
        };
        reported.unwrap_or(0)
    }

    /// Add-on meter values from the last processed block (empty for non-add-ons).
    pub fn addon_meters(&self) -> Vec<f32> {
        let n = self.addon.map_or(0, |d| d.meters.len());
        self.meter_values[..n].iter().map(|m| f32::from_bits(m.load(Ordering::Relaxed))).collect()
    }

    /// (input, output) peak per 10 ms window, oldest first (≈2 s).
    pub fn scope_snapshot(&self) -> Vec<(f32, f32)> {
        self.scope.snapshot()
    }

    fn process_inner(&self, left: &mut [f32], right: &mut [f32]) {
        if self.is_bypassed() {
            return; // Pass through unchanged — matches LightHost bypass logic
        }
        
        // Check if plugin is in crashed state
        if let Some(mut protection) = self.crash_protection.try_lock() {
            // Configuration error (no processor could be created): pass through.
            if matches!(protection.status, crash_protection::PluginStatus::Error(_)) {
                return;
            }
            if !protection.is_healthy() && !protection.try_auto_recover() {
                // Plugin crashed - fill with silence until cooldown expires.
                left.fill(0.0);
                right.fill(0.0);
                return;
            }
        }
        
        // Dispatch to exactly one processor based on format — no fallthrough.
        // Lock contended on any path → pass through (real-time safe).
        macro_rules! run_processor {
            ($guard:expr, $label:expr) => {
                run_processor!($guard, $label, |proc| proc.process_stereo(left, right))
            };
            ($guard:expr, $label:expr, |$proc:ident| $body:expr) => {
                if let Some(mut guard) = $guard {
                    if let Some(ref mut $proc) = *guard {
                        if let Err(crash_msg) = crash_protection::protected_call(AssertUnwindSafe(|| {
                            $body;
                        })) {
                            log::error!("{} plugin crashed during processing: {}", $label, crash_msg);
                            if let Some(mut protection) = self.crash_protection.try_lock() {
                                protection.mark_crashed(crash_msg);
                                // Notify the frontend exactly once when the crash limit is
                                // reached — flagged here, emitted off the audio thread by
                                // PluginInstanceManager::get_crash_statuses (polled by the UI).
                                if protection.crash_count == 3 {
                                    self.crash_notice_pending.store(true, Ordering::Release);
                                }
                            }
                            left.fill(0.0);
                            right.fill(0.0);
                        }
                    }
                }
            };
        }

        match self.plugin_info.format {
            PluginFormat::VST3    => run_processor!(self.vst3_processor.try_lock(), "VST3"),
            PluginFormat::VST     => run_processor!(self.vst2_processor.try_lock(),    "VST2"),
            PluginFormat::Builtin => run_processor!(self.addon_proc.try_lock(), "Built-in", |proc| self.run_addon(proc.as_mut(), left, right)),
            PluginFormat::CLAP    => run_processor!(self.clap_processor.try_lock(),    "CLAP"),
        }
    }

    /// One add-on block on the audio thread, with `addon_proc` held: apply
    /// changed params, process, publish meters and latency. No allocation.
    fn run_addon(&self, proc: &mut dyn AddonProcessor, left: &mut [f32], right: &mut [f32]) {
        // Writers store the value, then set its bit (Release). Taking the bits
        // (Acquire) before loading the values means every bit we see comes
        // with its value. A write landing after this swap re-sets its bit, so
        // it is applied next block (at worst applied twice) — never lost.
        let mut dirty = self.param_dirty.swap(0, Ordering::Acquire);
        while dirty != 0 {
            let i = dirty.trailing_zeros();
            dirty &= dirty - 1;
            proc.set_parameter(i, f32::from_bits(self.param_values[i as usize].load(Ordering::Relaxed)));
        }
        let n = self.addon.map_or(0, |d| d.meters.len());
        let mut buf = [0.0f32; 8];
        for (b, m) in buf.iter_mut().zip(&self.meter_values) {
            *b = f32::from_bits(m.load(Ordering::Relaxed));
        }
        proc.process(left, right, &mut buf[..n]);
        for (m, b) in self.meter_values.iter().zip(&buf[..n]) {
            m.store(b.to_bits(), Ordering::Relaxed);
        }
        self.addon_latency.store(proc.latency_samples(), Ordering::Relaxed);
        self.addon_status.store(pack_status(proc.status()), Ordering::Relaxed);
    }

    /// The add-on's last published status (Ok for anything but an add-on).
    pub fn addon_status(&self) -> AddonStatus {
        unpack_status(self.addon_status.load(Ordering::Relaxed))
    }

    /// Serialize plugin state as raw bytes (mirrors LightHost's `getStateInformation`).
    ///
    /// Never called from the audio thread, so it may wait briefly for the
    /// processor mutex the audio callback holds for one block. When a live
    /// read still isn't possible (busy), returns the last known state
    /// instead of nothing. Plugin state is read on the plugin host thread
    /// (a UI/main-thread call in every format).
    pub fn get_state_binary(&self) -> Vec<u8> {
        let live = match self.plugin_info.format {
            PluginFormat::VST3 => on_host(|| self.vst3_processor.try_lock_for(STATE_LOCK_TIMEOUT)
                .and_then(|g| g.as_ref().map(|p| p.get_state()))),
            PluginFormat::VST => on_host(|| self.vst2_processor.try_lock_for(STATE_LOCK_TIMEOUT)
                .and_then(|mut g| g.as_mut().map(|p| p.get_state()))),
            // Saved without the processor mutex, so the audio thread keeps
            // processing meanwhile (CLAP allows state calls concurrently
            // with process()).
            PluginFormat::CLAP => self.clap_state_handles()
                .map(|(plugin, ext)| on_host(|| unsafe { ClapProcessor::save_state(plugin, ext) })),
            PluginFormat::Builtin => None,
        };
        match live {
            Some(state) if !state.is_empty() => {
                *self.last_state.write() = Some(state.clone());
                state
            }
            _ => self.last_state.read().clone().unwrap_or_default(),
        }
    }

    /// Restore plugin state from raw bytes (mirrors LightHost's `setStateInformation`).
    pub fn set_state_binary(&self, data: &[u8]) {
        if !data.is_empty() {
            *self.last_state.write() = Some(data.to_vec());
        }
        match self.plugin_info.format {
            PluginFormat::VST3 => on_host(|| {
                if let Some(guard) = self.vst3_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
                    if let Some(ref proc) = *guard {
                        proc.set_state(data);
                    }
                }
            }),
            PluginFormat::VST => on_host(|| {
                if let Some(mut guard) = self.vst2_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
                    if let Some(ref mut proc) = *guard {
                        proc.set_state(data);
                    }
                }
            }),
            PluginFormat::CLAP => {
                if let Some((plugin, ext)) = self.clap_state_handles() {
                    on_host(|| unsafe { ClapProcessor::load_state(plugin, ext, data) });
                }
            }
            PluginFormat::Builtin => {}
        }
    }

    /// CLAP (plugin, state extension) pointers, taken under the processor
    /// mutex but used after it is released. They stay valid while `self`
    /// lives: only `Drop` removes the processor.
    fn clap_state_handles(&self) -> Option<(usize, usize)> {
        self.clap_processor.try_lock_for(STATE_LOCK_TIMEOUT)
            .and_then(|g| g.as_ref().and_then(|p| p.state_handles()))
    }

    /// Get instance info for serialization
    pub fn get_info(&self) -> PluginInstanceInfo {
        let name = self.display_name.read()
            .clone()
            .unwrap_or_else(|| self.plugin_info.name.clone());
        PluginInstanceInfo {
            instance_id: self.instance_id.clone(),
            plugin_id: self.plugin_info.id.clone(),
            name,
            vendor: self.plugin_info.vendor.clone(),
            version: self.plugin_info.version.clone(),
            path: self.plugin_info.path.clone(),
            format: self.plugin_info.format,
            category: self.plugin_info.category.clone(),
            bypassed: self.is_bypassed(),
            parameters: self.parameters.read().clone(),
            gui_open: self.gui_open.load(Ordering::Acquire),
            sub_index: self.plugin_info.sub_index,
        }
    }

    /// Rename this plugin instance (display name only; does not affect the plugin file).
    pub fn rename(&self, new_name: String) {
        *self.display_name.write() = if new_name.is_empty() {
            None
        } else {
            Some(new_name)
        };
    }

    /// Set parameter value and forward to the VST3 edit controller.
    pub fn set_parameter(&self, param_id: u32, value: f64) {
        let normalized = {
            let mut params = self.parameters.write();
            if let Some(param) = params.iter_mut().find(|p| p.id == param_id) {
                param.value = value.clamp(param.min, param.max);
                // Add-on: hand the value to the audio thread (see `run_addon`).
                // Add-on param ids are 0..len <= 64 (`check_params`).
                if self.addon.is_some() {
                    self.param_values[param_id as usize].store((param.value as f32).to_bits(), Ordering::Relaxed);
                    self.param_dirty.fetch_or(1 << param_id, Ordering::Release);
                }
                if param.max > param.min {
                    (param.value - param.min) / (param.max - param.min)
                } else {
                    0.0
                }
            } else {
                return; // Unknown parameter — no-op
            }
        };
        // Forward normalized value to the actual VST3 edit controller — a
        // UI-thread call, so on the plugin host thread. Never called from
        // the audio thread; waits at most one block for it.
        if self.plugin_info.format == PluginFormat::VST3 {
            on_host(|| {
                if let Some(guard) = self.vst3_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
                    if let Some(ref proc) = *guard {
                        proc.set_param_normalized(param_id, normalized);
                    }
                }
            });
        }
    }

    /// Applies saved parameters (preset, session, autosave). `saved_version`
    /// is the `plugin_version` stored with them, not the current one.
    pub fn restore_parameters(&self, saved_version: Option<&str>, saved: &[crate::domain::preset::PresetParameter]) {
        if let Some(d) = self.addon {
            let pairs = saved.iter().map(|p| (p.id, p.value as f32)).collect();
            for (id, v) in migrated_params(d, saved_version, pairs) {
                self.set_parameter(id, crate::plugins::addons::f(v));
            }
        } else if self.plugin_info.format == PluginFormat::Builtin {
            // Add-on removed: keep what was saved so a re-save loses nothing.
            *self.parameters.write() = saved
                .iter()
                .map(|p| PluginParameter { id: p.id, name: p.name.clone(), value: p.value, min: p.value, max: p.value, default: p.value })
                .collect();
            self.crash_protection.lock().status = crash_protection::PluginStatus::Error(format!(
                "add-on '{}' is not available in this version",
                self.plugin_info.path
            ));
        } else {
            for p in saved {
                self.set_parameter(p.id, p.value);
            }
        }
    }

    /// Open the plugin's native GUI editor using the existing VST3 processor.
    ///
    /// This reuses the same VST3 instance used for audio processing, ensuring
    /// that parameter changes in the GUI automatically sync with the audio processor.
    /// Only one GUI window can be open per instance at a time.
    pub fn open_gui(&self) -> Result<()> {
        // Check if GUI is already open
        if self.gui_open.load(Ordering::Acquire) {
            return Err(anyhow::anyhow!("GUI window already open for this plugin"));
        }

        // Set flag to prevent multiple opens
        if self.gui_open.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return Err(anyhow::anyhow!("GUI window already opening"));
        }

        let gui_flag = self.gui_open.clone();
        let crash_protection = self.crash_protection.clone();

        // ── VST3 GUI ─────────────────────────────────────────────────────────
        // Use try_lock_for with a short timeout so the Tauri command thread is
        // never blocked indefinitely when the audio callback holds the mutex.
        {
            // Only the controller is taken under the processor mutex (like
            // CLAP below): attaching an editor can take hundreds of ms, and
            // the audio thread's try_lock would fail — dry audio — meanwhile.
            let controller = match self.vst3_processor.try_lock_for(std::time::Duration::from_millis(200)) {
                Some(g) => g.as_ref().map(|p| p.editor_controller()),
                None => {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("Plugin processor busy — try again"));
                }
            };
            if let Some(controller) = controller {
                let plugin_name = self.plugin_info.name.clone();
                let Some(controller) = controller else {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("Plugin '{}' has no IEditController (GUI not supported)", plugin_name));
                };
                let gui_hwnd    = self.gui_hwnd.clone();
                // Synchronous, on the plugin host thread.
                let result = crash_protection::protected_call(AssertUnwindSafe(|| {
                    crate::plugins::gui::vst3::win::open_gui_window(&controller, &plugin_name, gui_flag.clone(), gui_hwnd)
                }));
                match result {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(e)) => {
                        gui_flag.store(false, Ordering::Release);
                        return Err(e);
                    }
                    Err(crash_msg) => {
                        log::error!("Plugin crashed during GUI opening: {}", crash_msg);
                        if let Some(mut prot) = crash_protection.try_lock() {
                            prot.mark_crashed(crash_msg.clone());
                        }
                        // Wait for any partially-started GUI thread to exit before
                        // clearing the flag, so a second open_gui call can't race it.
                        self.request_close_gui(Duration::from_secs(1));
                        gui_flag.store(false, Ordering::Release);
                        return Err(anyhow::anyhow!("Plugin crashed: {}", crash_msg));
                    }
                }
            }
        }

        // ── VST2 GUI ─────────────────────────────────────────────────────────
        {
            let guard = match self.vst2_processor.try_lock_for(std::time::Duration::from_millis(200)) {
                Some(g) => g,
                None => {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("VST2 plugin processor busy — try again"));
                }
            };
            if let Some(ref proc) = *guard {
                let plugin_name = self.plugin_info.name.clone();
                let gui_hwnd    = self.gui_hwnd.clone();
                match proc.open_gui(&plugin_name, gui_flag.clone(), gui_hwnd) {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        gui_flag.store(false, Ordering::Release);
                        return Err(e);
                    }
                }
            }
        }
        // ── CLAP GUI ────────────────────────────────────────────────────────────────────
        {
            // Only the GUI handles are taken under the processor mutex: the
            // editor is created on the plugin host thread (CLAP's main thread)
            // and that can take a while — holding the mutex meanwhile would
            // make the audio thread's try_lock fail and play dry audio.
            let handles = match self.clap_processor.try_lock_for(std::time::Duration::from_millis(200)) {
                Some(g) => g.as_ref().map(|p| p.gui_handles()),
                None => {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("CLAP plugin processor busy — try again"));
                }
            };
            if let Some(handles) = handles {
                let plugin_name = self.plugin_info.name.clone();
                let Some((plugin_raw, gui_raw)) = handles else {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("'{}' has no CLAP GUI extension", plugin_name));
                };
                let gui_hwnd = self.gui_hwnd.clone();
                let result = crash_protection::protected_call(AssertUnwindSafe(|| {
                    crate::plugins::gui::clap::open_clap_gui(plugin_raw, gui_raw, &plugin_name, gui_flag.clone(), gui_hwnd)
                }));
                match result {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(e)) => {
                        gui_flag.store(false, Ordering::Release);
                        return Err(e);
                    }
                    Err(crash_msg) => {
                        log::error!("CLAP plugin crashed during GUI opening: {}", crash_msg);
                        if let Some(mut prot) = crash_protection.try_lock() {
                            prot.mark_crashed(crash_msg.clone());
                        }
                        self.request_close_gui(Duration::from_secs(1));
                        gui_flag.store(false, Ordering::Release);
                        return Err(anyhow::anyhow!("CLAP plugin crashed: {}", crash_msg));
                    }
                }
            }
        }
        // No processor loaded for this plugin (it failed to load).
        self.gui_open.store(false, Ordering::Release);
        let reason = match self.get_crash_status() {
            crash_protection::PluginStatus::Error(e) => e,
            _ => format!("'{}' is not loaded; remove it and add it again", self.plugin_info.name),
        };
        Err(anyhow::anyhow!("Cannot open the editor: {}", reason))
    }
    
    /// Get crash protection status
    pub fn get_crash_status(&self) -> crash_protection::PluginStatus {
        let status = self.crash_protection.lock().status.clone();
        match (&status, self.addon_status()) {
            // Loading is transient, not a failure: the chain shows it as Ok.
            (crash_protection::PluginStatus::Ok, AddonStatus::Error(m)) => crash_protection::PluginStatus::Error(m.to_string()),
            _ => status,
        }
    }
    
    /// Reset crash protection status
    pub fn reset_crash_protection(&self) {
        self.crash_protection.lock().reset();
    }
    
}

impl Drop for PluginInstance {
    fn drop(&mut self) {
        // Clean drop = clean shutdown for this plugin's crash marker (see
        // crash_marker docs) — a native crash never reaches here at all,
        // which is exactly how the marker tells the two apart on next launch.
        if self.plugin_info.format == PluginFormat::VST3 {
            crate::core::crash_marker::mark_inactive(&self.plugin_info.path);
        }

        // If GUI teardown gets stuck, leaking processors is safer than dropping
        // while plugin UI threads may still execute through freed code/vtables.
        if !self.request_close_gui(Duration::from_secs(3)) {
            log::error!("GUI thread did not release in time; skipping processor teardown to avoid heap corruption");
            // Safety fallback: leaking the processor is better than dropping it
            // while plugin GUI thread is still alive (can trigger heap corruption).
            if let Some(proc) = self.vst3_processor.lock().take() {
                std::mem::forget(proc);
            }
            if let Some(proc) = self.vst2_processor.lock().take() {
                std::mem::forget(proc);
            }
            if let Some(proc) = self.clap_processor.lock().take() {
                std::mem::forget(proc);
            }
            if let Some(proc) = self.addon_proc.get_mut().take() {
                std::mem::forget(proc);
            }
        }
    }
}

/// `AddonStatus` in one word so the audio thread publishes it without a lock
/// or allocation: bits 62-63 kind (0 Ok, 1 Loading, 2 Error, 3 fixed
/// "add-on error" with no pointer), 48-61 message length, 0-47 message
/// pointer. A message whose pointer or length doesn't fit becomes kind 3.
fn pack_status(s: AddonStatus) -> u64 {
    match s {
        AddonStatus::Ok => 0,
        AddonStatus::Loading => 1 << 62,
        AddonStatus::Error(m) => {
            let (ptr, len) = (m.as_ptr() as u64, m.len() as u64);
            if ptr >> 48 != 0 || len >> 14 != 0 {
                return 3 << 62;
            }
            (2 << 62) | (len << 48) | ptr
        }
    }
}

fn unpack_status(v: u64) -> AddonStatus {
    match v >> 62 {
        1 => AddonStatus::Loading,
        2 => {
            let (ptr, len) = ((v & ((1 << 48) - 1)) as *const u8, ((v >> 48) & 0x3fff) as usize);
            // SAFETY: a kind-2 word is only ever written by `pack_status`, from a
            // `&'static str` (pointer and length of a live, valid UTF-8 static),
            // so rebuilding it is sound. The pointer goes through an integer and
            // back with `as` casts, which relies on exposed provenance.
            AddonStatus::Error(unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) })
        }
        3 => AddonStatus::Error("add-on error"),
        _ => AddonStatus::Ok,
    }
}

/// Splits job indices into (sequential, parallel) groups. VST3 jobs go
/// in `sequential` unless `parallel_vst3` is true, in which case every
/// job (VST3 included) goes in `parallel` — this is the entire behavior
/// difference the setting controls; everything downstream just iterates
/// whichever group a job landed in.
fn split_load_groups(is_vst3: &[bool], parallel_vst3: bool) -> (Vec<usize>, Vec<usize>) {
    let mut sequential = Vec::new();
    let mut parallel = Vec::new();
    for (idx, &vst3) in is_vst3.iter().enumerate() {
        if vst3 && !parallel_vst3 {
            sequential.push(idx);
        } else {
            parallel.push(idx);
        }
    }
    (sequential, parallel)
}

/// Waits (bounded) until `arc` is referenced only by the caller, i.e. the
/// audio thread has finished the block that was using the chain snapshot it
/// came from. arc-swap converts readers' outstanding debts into real
/// references during `store`/`swap`, so `strong_count` sees them.
fn wait_until_unshared<T>(arc: &Arc<T>) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Arc::strong_count(arc) > 1 {
        if Instant::now() >= deadline {
            log::warn!("Removed plugin chain entry still referenced after 1 s; dropping anyway");
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Manager for all plugin instances — lock-free RCU for audio processing.
pub struct PluginInstanceManager {
    instances: ArcSwap<Vec<Arc<PluginInstance>>>,
    modify_lock: Mutex<()>,
    /// Sample rate the current chain was created with (None = no plugin
    /// loaded yet). Block size isn't tracked: every processor already
    /// chunks to the max block it was prepared with.
    prepared_rate: Mutex<Option<f64>>,
}

impl PluginInstanceManager {
    pub fn new() -> Self {
        Self {
            instances: ArcSwap::from_pointee(Vec::new()),
            modify_lock: Mutex::new(()),
            prepared_rate: Mutex::new(None),
        }
    }

    /// Load a plugin and create an instance.
    ///
    /// `sample_rate` / `block_size` are forwarded to the VST3 processor for
    /// `setupProcessing`, matching LightHost's `graph.getSampleRate()` /
    /// `graph.getBlockSize()` passed to `createPluginInstance`.
    pub fn load_plugin(&self, plugin_info: PluginInfo, sample_rate: f64, block_size: usize) -> Result<String> {
        let instance = Arc::new(PluginInstance::new(plugin_info, sample_rate, block_size)?);
        let instance_id = instance.instance_id().to_string();
        let _guard = self.modify_lock.lock();
        *self.prepared_rate.lock() = Some(sample_rate);
        let current = self.instances.load();
        let mut list = (**current).clone();
        list.push(instance);
        self.instances.store(Arc::new(list));
        Ok(instance_id)
    }

    /// Load many plugins for preset/session restore. Preserves chain order.
    ///
    /// VST3 factories are initialised **sequentially** by default (COM +
    /// host stability), unless `parallel_vst3` is true, in which case they
    /// join the same Rayon batch as CLAP/VST2/built-ins. See
    /// `Vst3Processor::load()`'s `FACTORY_CREATE_LOCK` for what's still
    /// serialized even when this is on. Off by default; see
    /// `ConfigManager::get_parallel_vst3_loading`.
    ///
    /// Return value aligns 1:1 with `infos`: `Ok(id)` on success, `Err` when
    /// that plugin failed to construct (same semantics as skipping a failed
    /// `load_plugin` in a loop).
    ///
    /// CAVEAT (review finding, "Important #1"): callers (session restore,
    /// preset load) run on the plugin host thread (see core::host_thread).
    /// With `parallel_vst3` off (default), every VST3 plugin is therefore
    /// created on that one thread, like a single-plugin load. With it on and more than one VST3 plugin, those
    /// `createInstance`/`initialize()` calls move to Rayon worker threads
    /// instead. Some plugin frameworks (JUCE is the common one) assume the
    /// thread that first creates a plugin is *the* UI/message thread and set
    /// up internal timers or later editor calls relative to it; moving that
    /// identity for such a plugin is a real, undocumented-by-this-comment-
    /// until-now risk this feature does not otherwise account for. No code
    /// fix ships for this — doing so would mean either not actually
    /// parallelizing plugin creation, or verifying against real JUCE
    /// plugins, neither of which this change attempts. Before ever
    /// defaulting this setting on: test with real JUCE-based plugins and
    /// watch the `thread=` field in debug logs during `createInstance`/
    /// `initialize()` for exactly this class of hang.
    pub fn load_plugins_parallel_results(
        &self,
        infos: Vec<PluginInfo>,
        sample_rate: f64,
        block_size: usize,
        parallel_vst3: bool,
    ) -> Vec<Result<String, Error>> {
        let n = infos.len();
        if n == 0 {
            return Vec::new();
        }
        *self.prepared_rate.lock() = Some(sample_rate);
        if n == 1 {
            let info = infos.into_iter().next().unwrap();
            let name = info.name.clone();
            let result = match PluginInstance::new(info, sample_rate, block_size) {
                Ok(p) => {
                    let arc = Arc::new(p);
                    let id = arc.instance_id().to_string();
                    let _guard = self.modify_lock.lock();
                    let current = self.instances.load();
                    let mut list = (**current).clone();
                    list.push(arc);
                    self.instances.store(Arc::new(list));
                    Ok(id)
                }
                Err(e) => Err(e),
            };
            // See the multi-plugin path below for why this is a separate
            // event from the ones `fetchChain()` reacts to.
            crate::app_events::emit_plugin_chain_changed("restore_progress", Some(&name));
            return vec![result];
        }

        #[derive(Clone)]
        struct Job {
            idx: usize,
            info: PluginInfo,
            /// Not a built-in: must be created on the plugin host thread.
            on_host: bool,
        }

        let jobs: Vec<Job> = infos
            .into_iter()
            .enumerate()
            .map(|(idx, info)| {
                let on_host = info.format != PluginFormat::Builtin;
                Job { idx, info, on_host }
            })
            .collect();

        let mut slots: Vec<Option<Result<Arc<PluginInstance>, Error>>> = (0..n).map(|_| None).collect();

        // Progress feedback for the frontend's "Preparing plugins... N/total"
        // indicator. The manager's own instance list (what `fetchChain()`
        // reads) only gets these plugins appended once the WHOLE batch below
        // finishes, so it can't show incremental progress on its own — a
        // sequentially-loaded heavy VST3 (e.g. a plugin that loads its own
        // multi-second ML model in initialize()) would otherwise leave the
        // restore screen looking frozen for its entire load time. This event
        // fires per-plugin, independently of the batch commit, specifically
        // so the frontend can count it live instead of waiting.
        let needs_host: Vec<bool> = jobs.iter().map(|j| j.on_host).collect();
        // Every VST3/VST2/CLAP plugin is created here, on the plugin host
        // thread, whatever `parallel_vst3` says: JUCE binds its message
        // thread to the thread that creates the plugin (an editor/controller
        // driven from any other thread crashes: Xvox, Clear), and CLAP
        // init/activate are main-thread calls. Only built-ins go to Rayon —
        // a worker creating a plugin would wait on this (busy) thread in
        // `on_host` and deadlock. See the CAVEAT above.
        let _ = parallel_vst3;
        let (sequential_idx, parallel_idx) = split_load_groups(&needs_host, false);

        for &idx in &sequential_idx {
            let j = &jobs[idx];
            slots[j.idx] = Some(PluginInstance::new(j.info.clone(), sample_rate, block_size).map(Arc::new));
            crate::app_events::emit_plugin_chain_changed("restore_progress", Some(&j.info.name));
        }

        let parallel_jobs: Vec<Job> = parallel_idx.iter().map(|&idx| jobs[idx].clone()).collect();
        if !parallel_jobs.is_empty() {
            let filled: Vec<(usize, Result<Arc<PluginInstance>, Error>)> = parallel_jobs
                .into_par_iter()
                .map(|j| {
                    let idx = j.idx;
                    let name = j.info.name.clone();
                    let r = PluginInstance::new(j.info, sample_rate, block_size).map(Arc::new);
                    crate::app_events::emit_plugin_chain_changed("restore_progress", Some(&name));
                    (idx, r)
                })
                .collect();
            for (idx, r) in filled {
                slots[idx] = Some(r);
            }
        }

        let mut to_extend = Vec::new();
        let mut out = Vec::with_capacity(n);
        for (i, slot) in slots.into_iter().enumerate() {
            let r = slot.unwrap_or_else(|| Err(anyhow::anyhow!("internal: empty plugin slot {}", i)));
            match r {
                Ok(arc) => {
                    let id = arc.instance_id().to_string();
                    to_extend.push(arc);
                    out.push(Ok(id));
                }
                Err(e) => out.push(Err(e)),
            }
        }

        let _guard = self.modify_lock.lock();
        let current = self.instances.load();
        let mut list = (**current).clone();
        list.extend(to_extend);
        self.instances.store(Arc::new(list));
        out
    }

    /// Remove a plugin instance
    pub fn remove_instance(&self, instance_id: &str) -> Result<()> {
        // Proactively close GUI before removal to prevent teardown races.
        if let Some(inst) = self.get_instance(instance_id) {
            if inst.gui_open.load(Ordering::Acquire)
                && !inst.request_close_gui(Duration::from_secs(3))
            {
                return Err(anyhow::anyhow!(
                    "Cannot remove plugin: GUI did not close in time"
                ));
            }
        }

        // Extract the Arc while holding the modify lock, but do NOT drop it
        // inside the lock. PluginInstance::drop() can block waiting for GUI
        // cleanup if a window was just closed.
        let instance = {
            let _guard = self.modify_lock.lock();
            let current = self.instances.load();
            let mut list = (**current).clone();
            let pos = list
                .iter()
                .position(|i| i.instance_id() == instance_id)
                .ok_or_else(|| anyhow::anyhow!("Instance not found: {}", instance_id))?;
            let inst = list.remove(pos);
            self.instances.store(Arc::new(list));
            log::info!("Removed plugin instance: {}", instance_id);
            inst
            // modify lock drops here
        };
        // PluginInstance::drop() runs here, outside the lock — and only
        // after the audio thread has let go of the old chain snapshot, so
        // the drop (file I/O, GUI wait, plugin teardown) never lands on it.
        wait_until_unshared(&instance);
        drop(instance);
        Ok(())
    }

    /// Total delay the chain adds, in samples (plugins run in series).
    pub fn chain_latency_samples(&self) -> u32 {
        self.instances.load().iter().map(|i| i.latency_samples()).sum()
    }

    /// Get all instances
    pub fn get_instances(&self) -> Vec<PluginInstanceInfo> {
        self.instances
            .load()
            .iter()
            .map(|i| i.get_info())
            .collect()
    }

    /// Get all instances as their `Arc` handles, preserving chain order.
    /// Lets callers pair each instance with its `PluginInstanceInfo` by index
    /// (single pass) instead of looking each one up again by id.
    pub fn get_instances_arc(&self) -> Vec<Arc<PluginInstance>> {
        (**self.instances.load()).clone()
    }

    pub fn get_crash_statuses(&self) -> Vec<(String, crash_protection::PluginStatus)> {
        self.instances
            .load()
            .iter()
            .map(|i| {
                if i.crash_notice_pending.swap(false, Ordering::AcqRel) {
                    crate::app_events::emit_plugin_chain_changed("crash_limit_exceeded", Some(i.instance_id()));
                }
                (i.instance_id().to_string(), i.get_crash_status())
            })
            .collect()
    }

    /// Get specific instance
    pub fn get_instance(&self, instance_id: &str) -> Option<Arc<PluginInstance>> {
        self.instances
            .load()
            .iter()
            .find(|i| i.instance_id() == instance_id)
            .cloned()
    }

    /// Process stereo audio in-place through the entire plugin chain.
    ///
    /// Mirrors LightHost's `loadActivePlugins` graph routing:
    ///   INPUT → (non-bypassed) plugin 1 → plugin 2 → … → OUTPUT
    /// Real-time safety: 100% Lock-Free RCU pointer load (O(1), ~2-5ns).
    /// Audio thread NEVER blocks, waits, or drops audio due to UI operations.
    #[inline(always)]
    pub fn process_chain_stereo(&self, left: &mut [f32], right: &mut [f32]) {
        let instances = self.instances.load();
        for instance in instances.iter() {
            instance.process_stereo(left, right);
        }
    }

    /// Re-creates every plugin at `sample_rate` when the chain was prepared
    /// at a different one — plugins bake the rate in at load (VST3
    /// `setupProcessing`, VST2 `effSetSampleRate`, CLAP `activate`, built-in
    /// coefficients). Keeps order, bypass, display name, parameters and
    /// binary state. Returns whether a reload happened.
    ///
    /// Plugins are created one at a time on the calling thread (same as a
    /// sequential session restore); a plugin that fails to load at the new
    /// rate is dropped from the chain with a warning.
    // ponytail: full reload rather than per-format re-setup — one code path
    // that is correct for all four formats; add in-place re-setup if reload
    // time on large chains becomes a complaint.
    pub fn reprepare_if_rate_changed(&self, sample_rate: f64, block_size: usize) -> bool {
        let _guard = self.modify_lock.lock();
        {
            let mut prepared = self.prepared_rate.lock();
            if self.instances.load().is_empty() {
                *prepared = Some(sample_rate);
                return false;
            }
            if *prepared == Some(sample_rate) {
                return false;
            }
            *prepared = Some(sample_rate);
        }

        let old_list = self.instances.load_full();
        let mut new_list = Vec::with_capacity(old_list.len());
        for old in old_list.iter() {
            old.request_close_gui(crate::timing::GUI_CLOSE_TIMEOUT);
            let state = old.get_state_binary();
            let fresh = match PluginInstance::new(old.plugin_info.clone(), sample_rate, block_size) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("Reload at {sample_rate} Hz dropped '{}': {e}", old.plugin_info.name);
                    continue;
                }
            };
            fresh.set_bypassed(old.is_bypassed());
            *fresh.display_name.write() = old.display_name.read().clone();
            let params = old.parameters.read().clone();
            *fresh.parameters.write() = params.clone();
            for p in &params {
                fresh.set_parameter(p.id, p.value);
            }
            if !state.is_empty() {
                fresh.set_state_binary(&state);
            }
            new_list.push(Arc::new(fresh));
        }

        let old = self.instances.swap(Arc::new(new_list));
        drop(old_list);
        wait_until_unshared(&old);
        drop(old);
        log::info!("Plugin chain reloaded at {sample_rate} Hz");
        true
    }

    /// Clear all instances
    pub fn clear(&self) {
        let old = {
            let _guard = self.modify_lock.lock();
            self.instances.swap(Arc::new(Vec::new()))
        };
        // Same reason as remove_instance: drop here, never on the audio thread.
        // The reader holds the old Vec (not each instance), so wait on it.
        wait_until_unshared(&old);
        drop(old);
    }

    /// Moves `instance_id` to `to_index`. The id is resolved under the
    /// modify lock, so a chain change since the UI's snapshot can't move
    /// the wrong plugin.
    pub fn reorder(&self, instance_id: &str, to_index: usize) -> Result<()> {
        let _guard = self.modify_lock.lock();
        let current = self.instances.load();
        let mut list = (**current).clone();
        let len = list.len();
        let from_index = position_of(&list, instance_id)?;
        if to_index >= len {
            return Err(anyhow::anyhow!("Index out of bounds: to={}, len={}", to_index, len));
        }
        let item = list.remove(from_index);
        list.insert(to_index, item);
        self.instances.store(Arc::new(list));
        log::info!("Reordered plugin chain: {} -> {}", from_index, to_index);
        Ok(())
    }

    /// Swaps two instances, resolved by id under the modify lock.
    pub fn swap(&self, first_id: &str, second_id: &str) -> Result<()> {
        let _guard = self.modify_lock.lock();
        let current = self.instances.load();
        let mut list = (**current).clone();
        let first_index = position_of(&list, first_id)?;
        let second_index = position_of(&list, second_id)?;
        if first_index == second_index {
            return Ok(());
        }
        list.swap(first_index, second_index);
        self.instances.store(Arc::new(list));
        log::info!("Swapped plugin chain: {} <-> {}", first_index, second_index);
        Ok(())
    }
}

impl Default for PluginInstanceManager {
    fn default() -> Self {
        Self::new()
    }
}


fn position_of(list: &[Arc<PluginInstance>], instance_id: &str) -> Result<usize> {
    list.iter()
        .position(|i| i.instance_id() == instance_id)
        .ok_or_else(|| anyhow::anyhow!("Instance not found: {}", instance_id))
}

/// User-facing status text for a plugin whose processor failed to load.
fn load_error_message(info: &PluginInfo, err: &str) -> String {
    if !std::path::Path::new(&info.path).exists() {
        format!(
            "'{}' is not installed (file not found: {}), reinstall it; settings are kept",
            info.name, info.path
        )
    } else {
        format!("'{}' failed to load: {} — passing audio through", info.name, err)
    }
}

/// Runs `f` on the plugin host thread — every VST3 call but process() is a
/// UI-thread call, and the UI thread is the one that created the plugin —
/// and waits for it. Inline when already there.
fn on_host<'a, R: Send + 'static>(f: impl FnOnce() -> R + Send + 'a) -> R {
    let f: Box<dyn FnOnce() -> R + Send + 'a> = Box::new(f);
    // SAFETY: run_blocking returns only after the job has run (or been
    // dropped unrun), so whatever `f` borrows outlives it.
    let f: Box<dyn FnOnce() -> R + Send + 'static> = unsafe { std::mem::transmute(f) };
    crate::core::host_thread::run_blocking(f)
}

/// Process-unique instance id suffix (a counter — ids only need to be
/// unique within one run; presets identify plugins by path, not by id).
fn next_instance_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{:016x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Saved `(id, value)` pairs brought up to the add-on's current version.
/// Non-numeric or missing saved version counts as 1.
fn migrated_params(d: &AddonDescriptor, saved_version: Option<&str>, mut saved: Vec<(u32, f32)>) -> Vec<(u32, f32)> {
    let from = saved_version.and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
    if let (true, Some(migrate)) = (from < d.version, d.migrate) {
        migrate(from, &mut saved);
    }
    saved
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn on_host_runs_borrowing_work_on_the_host_thread() {
        let caller = std::thread::current().id();
        let mut seen = Vec::new();
        let host = on_host(|| {
            seen.push(1); // borrows a caller local
            std::thread::current().id()
        });
        assert_eq!(seen, [1]);
        assert_ne!(host, caller);
        assert_eq!(host, crate::core::host_thread::run_blocking(|| std::thread::current().id()));
    }

    #[test]
    fn reorder_and_swap_follow_ids_not_stale_indices() {
        let manager = PluginInstanceManager::new();
        let info = compressor().plugin_info.clone();
        let a = manager.load_plugin(info.clone(), 48_000.0, 512).unwrap();
        let b = manager.load_plugin(info.clone(), 48_000.0, 512).unwrap();
        let c = manager.load_plugin(info, 48_000.0, 512).unwrap();
        // The UI saw [a, b, c]; `a` was removed before its "move b" arrived.
        manager.remove_instance(&a).unwrap();
        manager.reorder(&b, 1).unwrap();
        let ids = |m: &PluginInstanceManager| m.get_instances().into_iter().map(|i| i.instance_id).collect::<Vec<_>>();
        assert_eq!(ids(&manager), [c.clone(), b.clone()]);
        manager.swap(&b, &c).unwrap();
        assert_eq!(ids(&manager), [b.clone(), c]);
        assert!(manager.reorder(&a, 0).is_err(), "removed plugin");
    }

    #[test]
    fn missing_plugin_file_shows_a_reinstall_error() {
        let info = PluginInfo {
            name: "Gone".into(),
            path: "C:/definitely/not/here/Gone.dll".into(),
            format: PluginFormat::VST, // VST3 would touch the real crash-marker file
            ..compressor().plugin_info.clone()
        };
        let inst = PluginInstance::new(info, 48_000.0, 512).unwrap();
        match inst.get_crash_status() {
            crash_protection::PluginStatus::Error(e) => assert!(e.contains("not installed"), "{e}"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    fn compressor() -> Arc<PluginInstance> {
        let info = PluginInfo {
            id: "builtin::compressor".into(),
            name: "Compressor".into(),
            vendor: String::new(),
            version: String::new(),
            path: relight_addon_compressor::ID.into(),
            format: PluginFormat::Builtin,
            category: String::new(),
            sub_index: 0,
        };
        Arc::new(PluginInstance::new(info, 48_000.0, 512).unwrap())
    }

    #[test]
    fn builtin_that_cannot_run_at_this_rate_reports_error_and_passes_audio_through() {
        let info = PluginInfo {
            path: relight_addon_deep_filter::ID.into(),
            format: PluginFormat::Builtin,
            ..compressor().plugin_info.clone()
        };
        let inst = PluginInstance::new(info, 44_100.0, 512).unwrap();
        assert!(matches!(inst.get_crash_status(), crash_protection::PluginStatus::Error(_)));
        let (mut l, mut r) = (vec![0.25f32; 32], vec![-0.25f32; 32]);
        inst.process_stereo(&mut l, &mut r);
        assert_eq!((l[31], r[31]), (0.25, -0.25));
        assert!(matches!(inst.get_crash_status(), crash_protection::PluginStatus::Error(_)), "status must survive processing");
    }

    struct FakeAddon(Arc<AtomicU32>);
    impl AddonProcessor for FakeAddon {
        fn process(&mut self, _: &mut [f32], _: &mut [f32], m: &mut [f32]) {
            m.fill(0.0);
        }
        fn set_parameter(&mut self, _: u32, _: f32) {}
        fn status(&self) -> AddonStatus {
            match self.0.load(Ordering::Relaxed) {
                0 => AddonStatus::Loading,
                1 => AddonStatus::Ok,
                _ => AddonStatus::Error("noise suppressor worker stopped unexpectedly"),
            }
        }
    }

    #[test]
    fn status_packing_round_trips_without_recursing() {
        for st in [AddonStatus::Ok, AddonStatus::Loading, AddonStatus::Error("model failed to load")] {
            assert_eq!(unpack_status(pack_status(st)), st);
        }
        let long: &'static str = Box::leak("x".repeat(1 << 14).into_boxed_str());
        assert_eq!(unpack_status(pack_status(AddonStatus::Error(long))), AddonStatus::Error("add-on error"));
        assert_eq!(unpack_status(3 << 62), AddonStatus::Error("add-on error"));
    }

    #[test]
    fn deep_filter_worker_panic_shows_error() {
        let code = Arc::new(AtomicU32::new(0));
        let inst = compressor();
        *inst.addon_proc.lock() = Some(Box::new(FakeAddon(Arc::clone(&code))));
        let (mut l, mut r) = (vec![0.0f32; 32], vec![0.0f32; 32]);
        inst.process_stereo(&mut l, &mut r);
        assert_eq!(inst.addon_status(), AddonStatus::Loading);
        assert!(matches!(inst.get_crash_status(), crash_protection::PluginStatus::Ok), "loading is not a crash");
        code.store(1, Ordering::Relaxed);
        inst.process_stereo(&mut l, &mut r);
        assert_eq!(inst.addon_status(), AddonStatus::Ok);
        code.store(2, Ordering::Relaxed); // the worker panicked
        inst.process_stereo(&mut l, &mut r);
        match inst.get_crash_status() {
            crash_protection::PluginStatus::Error(e) => assert_eq!(e, "noise suppressor worker stopped unexpectedly"),
            s => panic!("status {s:?}"),
        }
    }

    #[test]
    fn scope_shows_the_plugin_input_and_output_levels() {
        let inst = compressor();
        inst.set_parameter(4, 12.0); // +12 dB makeup, signal stays under threshold
        let (mut l, mut r) = (vec![0.1f32; 480], vec![0.1f32; 480]);
        inst.process_stereo(&mut l, &mut r);
        let snap = inst.scope_snapshot();
        assert_eq!(snap.len(), 1);
        assert!((snap[0].0 - 0.1).abs() < 1e-3, "pre {}", snap[0].0);
        assert!((snap[0].1 - 0.398).abs() < 0.01, "post {}", snap[0].1);
    }

    #[test]
    fn every_addon_loads_and_processes() {
        let mut s = 12345u32;
        for d in crate::plugins::addons::ADDONS {
            let info = crate::plugins::addons::plugin_info(d);
            let inst = PluginInstance::new(info, 48_000.0, 480).unwrap();
            // Sentinels: values each add-on's real meters cannot take (vad 0..=1, in_db <= -6,
            // limiter_gr_db >= 0), so the per-add-on asserts below prove the meter was published.
            inst.meter_values[0].store((-1.0f32).to_bits(), Ordering::Relaxed);
            inst.meter_values[1].store(1.0f32.to_bits(), Ordering::Relaxed); // gate_db is always <= 0
            inst.meter_values[2].store(1.0f32.to_bits(), Ordering::Relaxed); // compressor out_db is always <= 0
            for _ in 0..200 {
                let mut noise = || {
                    (0..480).map(|_| { s = s.wrapping_mul(1664525).wrapping_add(1013904223); (s >> 8) as f32 / 16_777_216.0 - 0.5 }).collect::<Vec<f32>>()
                };
                let (mut l, mut r) = (noise(), noise());
                inst.process_stereo(&mut l, &mut r);
                assert!(l.iter().chain(&r).all(|v| v.is_finite()), "{}", d.id);
            }
            let want = match d.id {
                "builtin::noise_suppressor" | "builtin::deep_filter" => 960, // deep_filter: 960 + n − gcd(n, 480), no margin at n = 480
                _ => 0,
            };
            assert_eq!(inst.latency_samples(), want, "{}", d.id);
            match d.id {
                "builtin::noise_suppressor" => {
                    let meters = inst.addon_meters();
                    assert_eq!(meters.len(), 2, "{}", d.id);
                    assert!((0.0..=1.0).contains(&meters[0]), "{}: vad {} never published", d.id, meters[0]);
                    assert!(meters[1].is_finite() && meters[1] <= 0.0, "{}: gate_db {}", d.id, meters[1]);
                }
                "builtin::deep_filter" => {
                    let meters = inst.addon_meters();
                    assert_eq!(meters.len(), 1, "{}", d.id);
                    assert!((0.0..=1.0).contains(&meters[0]), "{}: vad {} never published", d.id, meters[0]);
                }
                "builtin::compressor" => {
                    let m = inst.addon_meters();
                    assert_eq!(m.len(), 3, "{}", d.id);
                    assert!(m[0] <= -6.0, "{}: in_db {} never published", d.id, m[0]);
                    assert!(m[1].is_finite() && m[1] >= 0.0 && m[1] != 1.0, "{}: gr_db {}", d.id, m[1]);
                    assert!(m[2] <= 0.0, "{}: out_db {} never published", d.id, m[2]);
                }
                "builtin::voice" => {
                    let m = inst.addon_meters();
                    assert_eq!(m.len(), 1, "{}", d.id);
                    assert!(m[0].is_finite() && m[0] >= 0.0, "{}: limiter_gr_db {} never published", d.id, m[0]);
                }
                other => panic!("no meter check for {other}"),
            }
        }
    }

    #[test]
    fn chain_latency_sums_active_plugins_and_skips_bypassed_ones() {
        let manager = PluginInstanceManager::new();
        let ns = PluginInfo {
            path: relight_addon_noise_suppressor::ID.into(),
            ..compressor().plugin_info.clone()
        };
        let ns_id = manager.load_plugin(ns, 48_000.0, 512).unwrap();
        manager.load_plugin(compressor().plugin_info.clone(), 48_000.0, 512).unwrap();
        assert_eq!(manager.chain_latency_samples(), 960); // noise suppressor: 2 RNNoise frames
        manager.get_instance(&ns_id).unwrap().set_bypassed(true);
        assert_eq!(manager.chain_latency_samples(), 0);
    }

    #[test]
    fn get_state_falls_back_to_last_known_state_when_live_read_is_unavailable() {
        let inst = compressor();
        *inst.last_state.write() = Some(vec![1, 2, 3]);
        assert_eq!(inst.get_state_binary(), vec![1, 2, 3]);
    }

    fn noise_suppressor() -> Arc<PluginInstance> {
        let info = crate::plugins::addons::plugin_info(&relight_addon_noise_suppressor::ADDON);
        Arc::new(PluginInstance::new(info, 48_000.0, 512).unwrap())
    }

    #[test]
    fn set_parameter_never_needs_the_processor_lock() {
        let inst = compressor();
        let guard = inst.addon_proc.lock();
        inst.set_parameter(4, 30.0); // Makeup Gain +30 dB
        drop(guard);

        // 0.001 is far below threshold → output = input × makeup (≈31.6×).
        let (mut l, mut r) = (vec![0.001f32; 64], vec![0.001f32; 64]);
        inst.process_stereo(&mut l, &mut r);
        assert!(l[63] > 0.02, "makeup gain never reached the processor: {}", l[63]);
    }

    #[test]
    fn latency_and_meters_read_without_the_lock() {
        let inst = noise_suppressor();
        let _guard = inst.addon_proc.lock();
        assert_eq!(inst.latency_samples(), 960);
        assert_eq!(inst.addon_meters().len(), 2);
    }

    #[test]
    fn knob_moves_never_skip_an_addon_block() {
        let inst = compressor();
        inst.set_parameter(4, 12.0);
        let (mut l, mut r) = (vec![0.1f32; 4800], vec![0.1f32; 4800]);
        inst.process_stereo(&mut l, &mut r); // 100 ms warm-up: envelope settles below threshold

        let start = Arc::new(std::sync::Barrier::new(2));
        let knob = {
            let (inst, start) = (Arc::clone(&inst), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                for i in 0..10_000 {
                    inst.set_parameter(4, 6.0 + (i % 25) as f64); // makeup ∈ [6, 30] dB
                }
            })
        };
        let (lo, hi) = (0.1 * 10f32.powf(6.0 / 20.0), 0.1 * 10f32.powf(30.0 / 20.0));
        start.wait();
        for block in 0..2_000 {
            let (mut l, mut r) = ([0.1f32; 64], [0.1f32; 64]);
            inst.process_stereo(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|v| (lo..=hi).contains(v)), "block {block} skipped the add-on: {l:?}");
        }
        knob.join().unwrap();
    }

    #[test]
    fn unbypass_does_not_replay_stale_audio() {
        let inst = noise_suppressor();
        inst.set_parameter(0, 0.0); // mix 0: dry path only
        for _ in 0..4 {
            let (mut l, mut r) = ([0.5f32; 256], [0.5f32; 256]);
            inst.process_stereo(&mut l, &mut r);
        }
        inst.set_bypassed(true);
        inst.set_bypassed(false);
        let (mut out_l, mut out_r) = (Vec::new(), Vec::new());
        for _ in 0..4 {
            let (mut l, mut r) = ([0.0f32; 256], [0.0f32; 256]);
            inst.process_stereo(&mut l, &mut r);
            out_l.extend(l);
            out_r.extend(r);
        }
        let stale = out_l.iter().chain(&out_r).filter(|&&v| v != 0.0).count();
        assert_eq!(stale, 0, "{stale} of 2048 samples replayed pre-bypass audio");
    }

    /// Stands in for the audio thread: holds the chain snapshot (as
    /// `process_chain_stereo` does for a whole block) while `run` executes.
    fn with_reader_holding_chain(manager: &Arc<PluginInstanceManager>, run: impl FnOnce()) {
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let reader = {
            let manager = Arc::clone(manager);
            std::thread::spawn(move || {
                let _chain = manager.instances.load();
                held_tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(50));
            })
        };
        held_rx.recv().unwrap();
        run();
        reader.join().unwrap();
    }

    #[test]
    fn reprepare_reloads_the_chain_at_a_new_rate_keeping_user_settings() {
        let manager = PluginInstanceManager::new();
        let old_id = manager.load_plugin(compressor().plugin_info.clone(), 48_000.0, 512).unwrap();
        let old = manager.get_instance(&old_id).unwrap();
        old.set_parameter(4, 12.0);
        old.set_bypassed(true);
        old.rename("Vocal comp".into());
        drop(old);

        assert!(!manager.reprepare_if_rate_changed(48_000.0, 512), "same rate must not reload");
        assert!(manager.reprepare_if_rate_changed(44_100.0, 512));

        let chain = manager.get_instances();
        assert_eq!(chain.len(), 1);
        assert_ne!(chain[0].instance_id, old_id);
        assert!(chain[0].bypassed);
        assert_eq!(chain[0].name, "Vocal comp");
        assert_eq!(chain[0].parameters.iter().find(|p| p.id == 4).unwrap().value, 12.0);
        assert!(!manager.reprepare_if_rate_changed(44_100.0, 512));
    }

    #[test]
    fn remove_instance_drops_the_plugin_on_the_caller_not_the_reader() {
        let manager = Arc::new(PluginInstanceManager::new());
        let id = manager.load_plugin(compressor().plugin_info.clone(), 48_000.0, 512).unwrap();
        let weak = Arc::downgrade(&manager.get_instance(&id).unwrap());

        with_reader_holding_chain(&manager, || {
            manager.remove_instance(&id).unwrap();
            assert!(weak.upgrade().is_none(), "instance still alive after remove — its last ref (and Drop) is left to the reader");
        });
    }

    #[test]
    fn clear_drops_plugins_on_the_caller_not_the_reader() {
        let manager = Arc::new(PluginInstanceManager::new());
        let id = manager.load_plugin(compressor().plugin_info.clone(), 48_000.0, 512).unwrap();
        let weak = Arc::downgrade(&manager.get_instance(&id).unwrap());

        with_reader_holding_chain(&manager, || {
            manager.clear();
            assert!(weak.upgrade().is_none(), "instance still alive after clear — its last ref (and Drop) is left to the reader");
        });
    }
}

#[cfg(test)]
mod parallel_flag_tests {
    use super::split_load_groups;

    #[test]
    fn flag_off_routes_all_vst3_to_sequential() {
        let is_vst3 = vec![true, false, true, false];
        let (seq, par) = split_load_groups(&is_vst3, false);
        assert_eq!(seq, vec![0, 2]);
        assert_eq!(par, vec![1, 3]);
    }

    #[test]
    fn flag_on_routes_everything_to_parallel() {
        let is_vst3 = vec![true, false, true, false];
        let (seq, par) = split_load_groups(&is_vst3, true);
        assert!(seq.is_empty());
        assert_eq!(par, vec![0, 1, 2, 3]);
    }

    #[test]
    fn empty_input_produces_empty_groups_either_way() {
        let is_vst3: Vec<bool> = vec![];
        assert_eq!(split_load_groups(&is_vst3, false), (vec![], vec![]));
        assert_eq!(split_load_groups(&is_vst3, true), (vec![], vec![]));
    }

    #[test]
    fn single_vst3_job_respects_the_flag() {
        let is_vst3 = vec![true];
        assert_eq!(split_load_groups(&is_vst3, false), (vec![0], vec![]));
        assert_eq!(split_load_groups(&is_vst3, true), (vec![], vec![0]));
    }

    #[test]
    fn migrate_runs_only_from_older_versions() {
        use relight_addon_api::{AddonDescriptor, ParamSpec, Scale};
        #[allow(clippy::ptr_arg)] // must match MigrateFn
        fn double(_from: u32, p: &mut Vec<(u32, f32)>) {
            for x in p.iter_mut() {
                x.1 *= 2.0;
            }
        }
        static P: [ParamSpec; 1] = [ParamSpec { id: 0, name: "p", min: 0.0, max: 10.0, default: 0.0, unit: "", scale: Scale::Linear, step: 0.0, choices: &[] }];
        static D: AddonDescriptor = AddonDescriptor {
            id: "t", name: "t", category: "", version: 2, params: &P, meters: &[], sample_rates: &[],
            create: |_| None, migrate: Some(double),
        };
        let run = |v: Option<&str>| super::migrated_params(&D, v, vec![(0, 1.0)]);
        assert_eq!(run(Some("2.8.0")), vec![(0, 2.0)]);
        assert_eq!(run(Some("1")), vec![(0, 2.0)]);
        assert_eq!(run(None), vec![(0, 2.0)]);
        assert_eq!(run(Some("2")), vec![(0, 1.0)]);
        assert_eq!(run(Some("3")), vec![(0, 1.0)]);
    }
}
