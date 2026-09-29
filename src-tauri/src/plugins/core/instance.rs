use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};
use crate::plugins::types::{PluginInfo, PluginInstanceInfo, PluginParameter, PluginFormat};
use crate::plugins::processor::clap::ClapProcessor;
use crate::plugins::processor::vst2::Vst2Processor;
use crate::plugins::processor::vst3::Vst3Processor;
use crate::plugins::builtin::BuiltinProcessor;
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
    /// Built-in processor — present when format == Builtin.
    builtin_processor: Mutex<Option<Box<dyn BuiltinProcessor>>>,
    /// False after a restored state has already been applied; prevents
    /// replaying the same state into the VST3 controller when the GUI opens.
    vst3_gui_state_sync_pending: Arc<AtomicBool>,
    /// Latest restored VST3 binary state blob. Used when the GUI opens so we
    /// can sync the controller from the exact restored bytes instead of asking
    /// the live component to reserialize itself during editor startup.
    vst3_restored_state: Arc<RwLock<Option<Vec<u8>>>>,
    /// Last state blob successfully read from (or written to) the plugin.
    /// Returned by `get_state_binary` when a live read isn't possible right
    /// now (processor busy, VST3 GUI session holding `com_access_lock`) so an
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
        // Failure is non-fatal; the instance still works in pass-through mode.
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
            match Vst3Processor::load_class(&plugin_info.path, plugin_info.sub_index, sample_rate, block_size) {
                Ok(proc) => {
                    log::info!("{} VST3 processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("{} VST3 audio processor failed for '{}': {}", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, e);
                    crate::core::crash_marker::mark_inactive(&plugin_info.path);
                    None
                }
            }
        } else {
            None
        };

        let vst2_processor = if plugin_info.format == PluginFormat::VST {
            match Vst2Processor::load(&plugin_info.path, sample_rate, block_size) {
                Ok(proc) => {
                    log::info!("{} VST2 processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("{} VST2 audio processor failed for '{}': {}", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, e);
                    None
                }
            }
        } else {
            None
        };

        let clap_processor = if plugin_info.format == PluginFormat::CLAP {
            match ClapProcessor::load_index(&plugin_info.path, plugin_info.sub_index, sample_rate, block_size) {
                Ok(proc) => {
                    log::info!("{} CLAP processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("{} CLAP audio processor failed for '{}': {}", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, e);
                    None
                }
            }
        } else {
            None
        };

        let builtin_processor: Option<Box<dyn BuiltinProcessor>> =
            if plugin_info.format == PluginFormat::Builtin {
                let mut proc = crate::plugins::builtin::create_builtin(
                    &plugin_info.path, sample_rate as f32,
                );
                if proc.is_some() {
                    log::info!("{} Built-in processor ready for '{}'", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name);
                } else {
                    log::warn!("{} Built-in '{}' ({}) unavailable at {} Hz", crate::core::threading::thread_prefix("plugin/create"), plugin_info.name, plugin_info.path, sample_rate);
                }
                // Apply default parameter values to the processor so its internal
                // state matches what the frontend shows from the first frame.
                if let Some(ref mut p) = proc {
                    for param in crate::plugins::builtin::builtin_initial_params(&plugin_info.path) {
                        p.set_parameter(param.id, param.value as f32);
                    }
                }
                proc
            } else {
                None
            };

        // Pre-populate parameters for built-in plugins.
        let initial_params = if plugin_info.format == PluginFormat::Builtin {
            crate::plugins::builtin::builtin_initial_params(&plugin_info.path)
        } else {
            Vec::new()
        };

        let crash_protection = crash_protection::create_shared();
        if plugin_info.format == PluginFormat::Builtin && builtin_processor.is_none() {
            crash_protection.lock().status = crash_protection::PluginStatus::Error(format!(
                "'{}' cannot run at {} Hz (needs 48000 Hz) — passing audio through",
                plugin_info.name, sample_rate
            ));
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
            builtin_processor: Mutex::new(builtin_processor),
            vst3_gui_state_sync_pending: Arc::new(AtomicBool::new(true)),
            vst3_restored_state: Arc::new(RwLock::new(None)),
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

    /// Set bypass state
    pub fn set_bypassed(&self, bypassed: bool) {
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
            std::thread::sleep(Duration::from_millis(5));
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
            PluginFormat::Builtin => self.builtin_processor.try_lock_for(t).and_then(|g| g.as_ref().map(|p| p.latency_samples())),
        };
        reported.unwrap_or(0)
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
                if let Some(mut guard) = $guard {
                    if let Some(ref mut proc) = *guard {
                        if let Err(crash_msg) = crash_protection::protected_call(AssertUnwindSafe(|| {
                            proc.process_stereo(left, right);
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
            PluginFormat::Builtin => run_processor!(self.builtin_processor.try_lock(), "Built-in"),
            PluginFormat::CLAP    => run_processor!(self.clap_processor.try_lock(),    "CLAP"),
        }
    }

    /// Serialize plugin state as raw bytes (mirrors LightHost's `getStateInformation`).
    ///
    /// Never called from the audio thread, so it may wait briefly for the
    /// processor mutex the audio callback holds for one block. When a live
    /// read still isn't possible (busy, or a VST3 GUI session holding
    /// `com_access_lock`), returns the last known state instead of nothing.
    pub fn get_state_binary(&self) -> Vec<u8> {
        let live = match self.plugin_info.format {
            PluginFormat::VST3 => self.vst3_processor.try_lock_for(STATE_LOCK_TIMEOUT)
                .and_then(|g| g.as_ref().map(|p| p.get_state())),
            PluginFormat::VST => self.vst2_processor.try_lock_for(STATE_LOCK_TIMEOUT)
                .and_then(|mut g| g.as_mut().map(|p| p.get_state())),
            PluginFormat::CLAP => self.clap_processor.try_lock_for(STATE_LOCK_TIMEOUT)
                .and_then(|g| g.as_ref().map(|p| p.get_state())),
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
            PluginFormat::VST3 => {
                if let Some(guard) = self.vst3_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
                    if let Some(ref proc) = *guard {
                        *self.vst3_restored_state.write() = Some(data.to_vec());
                        self.vst3_gui_state_sync_pending.store(true, Ordering::Release);
                        proc.set_state(data);
                    }
                }
            }
            PluginFormat::VST => {
                if let Some(mut guard) = self.vst2_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
                    if let Some(ref mut proc) = *guard {
                        proc.set_state(data);
                    }
                }
            }
            PluginFormat::CLAP => {
                if let Some(guard) = self.clap_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
                    if let Some(ref proc) = *guard {
                        proc.set_state(data);
                    }
                }
            }
            PluginFormat::Builtin => {}
        }
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
                if param.max > param.min {
                    (param.value - param.min) / (param.max - param.min)
                } else {
                    0.0
                }
            } else {
                return; // Unknown parameter — no-op
            }
        };
        // Forward normalized value to the actual VST3 edit controller.
        // Never called from the audio thread; waits at most one block for it.
        if let Some(guard) = self.vst3_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
            if let Some(ref proc) = *guard {
                proc.set_param_normalized(param_id, normalized);
            }
        }
        // Built-ins receive the raw (clamped) value; internal unit conversion is per-processor.
        if let Some(mut guard) = self.builtin_processor.try_lock_for(STATE_LOCK_TIMEOUT) {
            if let Some(ref mut proc) = *guard {
                // Re-read raw value from the now-updated parameters list.
                let raw = self.parameters.read()
                    .iter()
                    .find(|p| p.id == param_id)
                    .map(|p| p.value as f32)
                    .unwrap_or(0.0);
                proc.set_parameter(param_id, raw);
            }
        }
    }

    /// Return the last voice-activity probability from the built-in noise suppressor
    /// (0.0 = silence / noise, 1.0 = clear speech).  Returns 0.0 for non-builtin plugins.
    pub fn get_builtin_vad(&self) -> f32 {
        if let Some(guard) = self.builtin_processor.try_lock() {
            if let Some(ref proc) = *guard {
                return proc.get_vad();
            }
        }
        0.0
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
            let guard = match self.vst3_processor.try_lock_for(std::time::Duration::from_millis(200)) {
                Some(g) => g,
                None => {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("Plugin processor busy — try again"));
                }
            };
            if let Some(ref proc) = *guard {
                let plugin_name = self.plugin_info.name.clone();
                let gui_hwnd    = self.gui_hwnd.clone();
                // Replay the restored state into the GUI's controller exactly once per
                // restore — only the first open after a set_state_binary() call needs
                // this; reopening later would otherwise replay stale bytes on top of
                // whatever the user has since changed via the GUI itself.
                let sync_component_state = self.vst3_gui_state_sync_pending.load(Ordering::Acquire);
                let restored_state_blob = if sync_component_state {
                    self.vst3_restored_state.read().clone()
                } else {
                    None
                };
                let result = crash_protection::protected_call(AssertUnwindSafe(|| {
                    proc.open_gui(&plugin_name, gui_flag.clone(), gui_hwnd, sync_component_state, restored_state_blob)
                }));
                if result.is_ok() {
                    self.vst3_gui_state_sync_pending.store(false, Ordering::Release);
                }
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
            let guard = match self.clap_processor.try_lock_for(std::time::Duration::from_millis(200)) {
                Some(g) => g,
                None => {
                    gui_flag.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!("CLAP plugin processor busy — try again"));
                }
            };
            if let Some(ref proc) = *guard {
                let plugin_name = self.plugin_info.name.clone();
                let gui_hwnd    = self.gui_hwnd.clone();
                let result = crash_protection::protected_call(AssertUnwindSafe(|| {
                    proc.open_gui(&plugin_name, gui_flag.clone(), gui_hwnd)
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
        // No processor loaded for this plugin.
        self.gui_open.store(false, Ordering::Release);
        Err(anyhow::anyhow!("No audio processor available for GUI"))
    }
    
    /// Get crash protection status
    pub fn get_crash_status(&self) -> crash_protection::PluginStatus {
        self.crash_protection.lock().status.clone()
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
            if let Some(proc) = self.builtin_processor.lock().take() {
                std::mem::forget(proc);
            }
        }
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
    /// CAVEAT (review finding, "Important #1"): `restore_session` — the only
    /// caller — is a plain synchronous Tauri command, which Tauri 2 runs on
    /// the main/UI thread. With `parallel_vst3` off (default), every VST3
    /// plugin is therefore created on that same main thread, same as before
    /// this feature existed. With it on and more than one VST3 plugin, those
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
            vst3: bool,
        }

        let jobs: Vec<Job> = infos
            .into_iter()
            .enumerate()
            .map(|(idx, info)| {
                let vst3 = info.format == PluginFormat::VST3;
                Job { idx, info, vst3 }
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
        let is_vst3: Vec<bool> = jobs.iter().map(|j| j.vst3).collect();
        let (sequential_idx, parallel_idx) = split_load_groups(&is_vst3, parallel_vst3);

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

    /// Reorder instances in the chain
    pub fn reorder(&self, from_index: usize, to_index: usize) -> Result<()> {
        let _guard = self.modify_lock.lock();
        let current = self.instances.load();
        let mut list = (**current).clone();
        let len = list.len();
        if from_index >= len || to_index >= len {
            return Err(anyhow::anyhow!(
                "Index out of bounds: from={}, to={}, len={}",
                from_index, to_index, len
            ));
        }
        let item = list.remove(from_index);
        list.insert(to_index, item);
        self.instances.store(Arc::new(list));
        log::info!("Reordered plugin chain: {} -> {}", from_index, to_index);
        Ok(())
    }

    /// Swap two instances in the chain
    pub fn swap(&self, first_index: usize, second_index: usize) -> Result<()> {
        let _guard = self.modify_lock.lock();
        let current = self.instances.load();
        let mut list = (**current).clone();
        let len = list.len();
        if first_index >= len || second_index >= len {
            return Err(anyhow::anyhow!(
                "Index out of bounds: first={}, second={}, len= {}",
                first_index, second_index, len
            ));
        }
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


/// Process-unique instance id suffix (a counter — ids only need to be
/// unique within one run; presets identify plugins by path, not by id).
fn next_instance_id() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{:016x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod state_tests {
    use super::*;

    fn compressor() -> Arc<PluginInstance> {
        let info = PluginInfo {
            id: "builtin::compressor".into(),
            name: "Compressor".into(),
            vendor: String::new(),
            version: String::new(),
            path: crate::plugins::builtin::compressor::ID.into(),
            format: PluginFormat::Builtin,
            category: String::new(),
            sub_index: 0,
        };
        Arc::new(PluginInstance::new(info, 48_000.0, 512).unwrap())
    }

    #[test]
    fn builtin_that_cannot_run_at_this_rate_reports_error_and_passes_audio_through() {
        let info = PluginInfo {
            path: crate::plugins::builtin::deep_filter::ID.into(),
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
    fn chain_latency_sums_active_plugins_and_skips_bypassed_ones() {
        let manager = PluginInstanceManager::new();
        let ns = PluginInfo {
            path: crate::plugins::builtin::noise_suppressor::ID.into(),
            ..compressor().plugin_info.clone()
        };
        let ns_id = manager.load_plugin(ns, 48_000.0, 512).unwrap();
        manager.load_plugin(compressor().plugin_info.clone(), 48_000.0, 512).unwrap();
        assert_eq!(manager.chain_latency_samples(), 480);
        manager.get_instance(&ns_id).unwrap().set_bypassed(true);
        assert_eq!(manager.chain_latency_samples(), 0);
    }

    #[test]
    fn get_state_falls_back_to_last_known_state_when_live_read_is_unavailable() {
        let inst = compressor();
        *inst.last_state.write() = Some(vec![1, 2, 3]);
        assert_eq!(inst.get_state_binary(), vec![1, 2, 3]);
    }

    #[test]
    fn set_parameter_reaches_processor_despite_brief_lock_contention() {
        let inst = compressor();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let holder = {
            let inst = Arc::clone(&inst);
            std::thread::spawn(move || {
                let _g = inst.builtin_processor.lock();
                locked_tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(20));
            })
        };
        locked_rx.recv().unwrap();
        inst.set_parameter(4, 30.0); // Makeup Gain +30 dB
        holder.join().unwrap();

        // 0.001 is far below threshold → output = input × makeup (≈31.6×).
        let mut l = vec![0.001f32; 64];
        let mut r = vec![0.001f32; 64];
        inst.process_stereo(&mut l, &mut r);
        assert!(l[63] > 0.02, "makeup gain never reached the processor: {}", l[63]);
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
}
