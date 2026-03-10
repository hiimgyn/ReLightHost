use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::panic::AssertUnwindSafe;
use crate::plugins::types::{PluginInfo, PluginInstanceInfo, PluginParameter, PluginFormat};
use crate::plugins::vst3_processor::Vst3Processor;
use crate::plugins::vst2_processor::Vst2Processor;
use crate::plugins::builtin_processor::NoiseSuppressor;
use crate::plugins::crash_protection::{self, SharedCrashProtection};
use anyhow::Result;

/// Represents a loaded plugin instance with an optional real audio processor.
///
/// Mirrors LightHost's per-node model in AudioProcessorGraph:
///   instance_id  ↔  node id
///   processor.process_stereo  ↔  AudioPlugin::processBlock
///   processor.get_state/set_state  ↔  getStateInformation/setStateInformation
pub struct PluginInstance {
    instance_id:    String,
    plugin_info:    PluginInfo,
    bypassed:       Arc<RwLock<bool>>,
    parameters:     Arc<RwLock<Vec<PluginParameter>>>,
    /// VST3 audio processor (vst3-rs) — present when format == VST3.
    vst3_processor: Mutex<Option<Vst3Processor>>,
    /// VST2 audio processor (vst-rs) — present when format == VST.
    vst2_processor: Mutex<Option<Vst2Processor>>,
    /// Built-in processor — present when format == Builtin.
    builtin_processor: Mutex<Option<NoiseSuppressor>>,
    /// Track if GUI window is currently open (prevents multiple windows)
    gui_open:       Arc<AtomicBool>,
    /// HWND (as isize) of the open GUI window; 0 when none.
    /// Set by the GUI thread after CreateWindowExW, cleared on exit.
    /// Lets Drop post WM_CLOSE so the GUI thread finishes before
    /// Vst3Processor::drop calls terminate() — prevents STATUS_ACCESS_VIOLATION.
    gui_hwnd:       Arc<AtomicIsize>,
    /// Crash protection state
    crash_protection: SharedCrashProtection,
}

impl PluginInstance {
    /// Create a new plugin instance.
    ///
    /// `sample_rate` and `block_size` are used to initialize the VST3 audio
    /// processor — matching LightHost's `deviceManager.initialise` values passed
    /// to `formatManager.createPluginInstance`.
    pub fn new(plugin_info: PluginInfo, sample_rate: f64, block_size: usize) -> Result<Self> {
        let instance_id = format!("instance_{}", uuid::Uuid::new_v4());

        log::info!("Creating plugin instance: {} ({})", plugin_info.name, instance_id);

        // Load the appropriate audio processor for the plugin format.
        // Failure is non-fatal; the instance still works in pass-through mode.
        let vst3_processor = if plugin_info.format == PluginFormat::VST3 {
            match Vst3Processor::load(&plugin_info.path, sample_rate, block_size) {
                Ok(proc) => {
                    log::info!("VST3 processor ready for '{}'", plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("VST3 audio processor failed for '{}': {}", plugin_info.name, e);
                    None
                }
            }
        } else {
            None
        };

        let vst2_processor = if plugin_info.format == PluginFormat::VST {
            match Vst2Processor::load(&plugin_info.path, sample_rate, block_size) {
                Ok(proc) => {
                    log::info!("VST2 processor ready for '{}'", plugin_info.name);
                    Some(proc)
                }
                Err(e) => {
                    log::warn!("VST2 audio processor failed for '{}': {}", plugin_info.name, e);
                    None
                }
            }
        } else {
            None
        };

        let builtin_processor = if plugin_info.format == PluginFormat::Builtin {
            log::info!("Built-in processor ready for '{}'", plugin_info.name);
            Some(NoiseSuppressor::new())
        } else {
            None
        };

        // Pre-populate parameters for built-in plugins.
        let initial_params = if plugin_info.format == PluginFormat::Builtin {
            vec![
                PluginParameter {
                    id:      0,
                    name:    "Mix".to_string(),
                    value:   1.0,
                    min:     0.0,
                    max:     1.0,
                    default: 1.0,
                },
            ]
        } else {
            Vec::new()
        };

        Ok(Self {
            instance_id,
            plugin_info,
            bypassed:          Arc::new(RwLock::new(false)),
            parameters:        Arc::new(RwLock::new(initial_params)),
            vst3_processor:    Mutex::new(vst3_processor),
            vst2_processor:    Mutex::new(vst2_processor),
            builtin_processor: Mutex::new(builtin_processor),
            gui_open:          Arc::new(AtomicBool::new(false)),
            gui_hwnd:          Arc::new(AtomicIsize::new(0)),
            crash_protection:  crash_protection::create_shared(),
        })
    }

    /// Get instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Get plugin info
    #[allow(dead_code)]
    pub fn plugin_info(&self) -> &PluginInfo {
        &self.plugin_info
    }

    /// Set bypass state
    pub fn set_bypassed(&self, bypassed: bool) {
        *self.bypassed.write() = bypassed;
    }

    /// Check if bypassed
    pub fn is_bypassed(&self) -> bool {
        *self.bypassed.read()
    }

    /// Process a stereo buffer in-place through the VST3 plugin.
    ///
    /// Mirrors LightHost's chain: INPUT → plugin → OUTPUT.
    /// Uses try_lock so the audio callback never blocks waiting for the mutex.
    /// Wrapped with crash protection to prevent plugin crashes from taking down the app.
    pub fn process_stereo(&self, left: &mut [f32], right: &mut [f32]) {
        if self.is_bypassed() {
            return; // Pass through unchanged — matches LightHost bypass logic
        }
        
        // Check if plugin is in crashed state
        if let Some(protection) = self.crash_protection.try_lock() {
            if !protection.is_healthy() {
                // Plugin crashed - fill with silence
                left.fill(0.0);
                right.fill(0.0);
                return;
            }
        }
        
        // Try VST3 processor first, then VST2.
        let processed = if let Some(mut guard) = self.vst3_processor.try_lock() {
            if let Some(ref mut proc) = *guard {
                let result = crash_protection::protected_call(AssertUnwindSafe(|| {
                    proc.process_stereo(left, right);
                }));
                if let Err(crash_msg) = result {
                    log::error!("VST3 plugin crashed during processing: {}", crash_msg);
                    if let Some(mut protection) = self.crash_protection.try_lock() {
                        protection.mark_crashed(crash_msg);
                    }
                    left.fill(0.0);
                    right.fill(0.0);
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        if !processed {
            if let Some(mut guard) = self.vst2_processor.try_lock() {
                if let Some(ref mut proc) = *guard {
                    let result = crash_protection::protected_call(AssertUnwindSafe(|| {
                        proc.process_stereo(left, right);
                    }));
                    if let Err(crash_msg) = result {
                        log::error!("VST2 plugin crashed during processing: {}", crash_msg);
                        if let Some(mut protection) = self.crash_protection.try_lock() {
                            protection.mark_crashed(crash_msg);
                        }
                        left.fill(0.0);
                        right.fill(0.0);
                    }
                    return;
                }
                // No VST2 processor → fall through to built-in check
            }

            // Built-in processors are crash-safe (pure Rust); no extra protection needed.
            if let Some(mut guard) = self.builtin_processor.try_lock() {
                if let Some(ref mut proc) = *guard {
                    proc.process_stereo(left, right);
                }
            }
            // Lock contended → pass through non-blocking (real-time safe)
        }
    }

    /// Serialize plugin state as raw bytes (mirrors LightHost's `getStateInformation`).
    pub fn get_state_binary(&self) -> Vec<u8> {
        if let Some(guard) = self.vst3_processor.try_lock() {
            if let Some(ref proc) = *guard {
                return proc.get_state();
            }
        }
        if let Some(mut guard) = self.vst2_processor.try_lock() {
            if let Some(ref mut proc) = *guard {
                return proc.get_state();
            }
        }
        Vec::new()
    }

    /// Restore plugin state from raw bytes (mirrors LightHost's `setStateInformation`).
    pub fn set_state_binary(&self, data: &[u8]) {
        if let Some(guard) = self.vst3_processor.try_lock() {
            if let Some(ref proc) = *guard {
                proc.set_state(data);
                return;
            }
        }
        if let Some(mut guard) = self.vst2_processor.try_lock() {
            if let Some(ref mut proc) = *guard {
                proc.set_state(data);
            }
        }
    }

    /// Get instance info for serialization
    pub fn get_info(&self) -> PluginInstanceInfo {
        PluginInstanceInfo {
            instance_id: self.instance_id.clone(),
            plugin_id: self.plugin_info.id.clone(),
            name: self.plugin_info.name.clone(),
            vendor: self.plugin_info.vendor.clone(),
            version: self.plugin_info.version.clone(),
            path: self.plugin_info.path.clone(),
            format: self.plugin_info.format,
            category: self.plugin_info.category.clone(),
            bypassed: self.is_bypassed(),
            parameters: self.parameters.read().clone(),
        }
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
        // Use try_lock so this is safe to call from any thread without blocking.
        if let Some(guard) = self.vst3_processor.try_lock() {
            if let Some(ref proc) = *guard {
                proc.set_param_normalized(param_id, normalized);
            }
        }
        // Forward to built-in processor (param 0 = Mix).
        if let Some(mut guard) = self.builtin_processor.try_lock() {
            if let Some(ref mut proc) = *guard {
                if param_id == 0 {
                    proc.set_mix(normalized as f32);
                }
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
        
        if let Some(guard) = self.vst3_processor.try_lock() {
            if let Some(ref proc) = *guard {
                // Wrap GUI opening with crash protection
                let plugin_name = self.plugin_info.name.clone();
                let gui_hwnd = self.gui_hwnd.clone();
                let result = crash_protection::protected_call(|| {
                    proc.open_gui(&plugin_name, gui_flag.clone(), gui_hwnd)
                });
                
                match result {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(e)) => {
                        gui_flag.store(false, Ordering::Release);
                        return Err(e);
                    }
                    Err(crash_msg) => {
                        log::error!("Plugin crashed during GUI opening: {}", crash_msg);
                        if let Some(mut protection) = crash_protection.try_lock() {
                            protection.mark_crashed(crash_msg.clone());
                        }
                        gui_flag.store(false, Ordering::Release);
                        return Err(anyhow::anyhow!("Plugin crashed: {}", crash_msg));
                    }
                }
            }
        }
        
        // Failed to get processor - clear flag
        self.gui_open.store(false, Ordering::Release);
        Err(anyhow::anyhow!("VST3 processor not available for GUI"))
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
        // If a native GUI window is still open, post WM_CLOSE and wait for the
        // GUI thread to exit *before* Vst3Processor drops and calls terminate().
        // Without this, terminate() races with the GUI message loop and causes
        // STATUS_ACCESS_VIOLATION (0xC0000005) on Windows.
        let hwnd = self.gui_hwnd.load(Ordering::Acquire);
        if hwnd != 0 {
            #[cfg(target_os = "windows")]
            unsafe {
                use windows_sys::Win32::Foundation::HWND;
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
                PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
            }
            // Spin-wait up to 2 s for the GUI thread to clear gui_open.
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_secs(2);
            while self.gui_open.load(Ordering::Acquire)
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if self.gui_open.load(Ordering::Acquire) {
                log::warn!("GUI thread did not exit within 2 s; proceeding with plugin teardown");
            }
        }
    }
}

/// Manager for all plugin instances
pub struct PluginInstanceManager {
    instances: Arc<RwLock<Vec<Arc<PluginInstance>>>>,
}

impl PluginInstanceManager {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(Vec::new())),
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
        self.instances.write().push(instance);
        Ok(instance_id)
    }

    /// Remove a plugin instance
    pub fn remove_instance(&self, instance_id: &str) -> Result<()> {
        let mut instances = self.instances.write();
        if let Some(pos) = instances.iter().position(|i| i.instance_id() == instance_id) {
            instances.remove(pos);
            log::info!("Removed plugin instance: {}", instance_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Instance not found: {}", instance_id))
        }
    }

    /// Get all instances
    pub fn get_instances(&self) -> Vec<PluginInstanceInfo> {
        self.instances
            .read()
            .iter()
            .map(|i| i.get_info())
            .collect()
    }

    /// Get specific instance
    pub fn get_instance(&self, instance_id: &str) -> Option<Arc<PluginInstance>> {
        self.instances
            .read()
            .iter()
            .find(|i| i.instance_id() == instance_id)
            .cloned()
    }

    /// Process stereo audio in-place through the entire plugin chain.
    ///
    /// Mirrors LightHost's `loadActivePlugins` graph routing:
    ///   INPUT → (non-bypassed) plugin 1 → plugin 2 → … → OUTPUT
    /// Called from the CPAL audio output callback via the process callback.
    pub fn process_chain_stereo(&self, left: &mut [f32], right: &mut [f32]) {
        let instances = self.instances.read();
        for instance in instances.iter() {
            instance.process_stereo(left, right);
        }
    }

    /// Clear all instances
    pub fn clear(&self) {
        self.instances.write().clear();
    }

    /// Reorder instances in the chain
    pub fn reorder(&self, from_index: usize, to_index: usize) -> Result<()> {
        let mut instances = self.instances.write();
        let len = instances.len();
        if from_index >= len || to_index >= len {
            return Err(anyhow::anyhow!(
                "Index out of bounds: from={}, to={}, len={}",
                from_index, to_index, len
            ));
        }
        let item = instances.remove(from_index);
        instances.insert(to_index, item);
        log::info!("Reordered plugin chain: {} -> {}", from_index, to_index);
        Ok(())
    }
}

impl Default for PluginInstanceManager {
    fn default() -> Self {
        Self::new()
    }
}


// Re-export for uuid
mod uuid {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    
    pub struct Uuid;
    
    impl Uuid {
        pub fn new_v4() -> String {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            format!("{:016x}", id)
        }
    }
}
