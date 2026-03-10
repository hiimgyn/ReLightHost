use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use crate::plugins::types::{PluginInfo, PluginInstanceInfo, PluginParameter};
use crate::plugins::vst3_processor::Vst3Processor;
use anyhow::Result;

/// Represents a loaded plugin instance with an optional real VST3 audio processor.
///
/// Mirrors LightHost's per-node model in AudioProcessorGraph:
///   instance_id  ↔  node id
///   vst3_processor.process_stereo  ↔  AudioPlugin::processBlock
///   vst3_processor.get_state/set_state  ↔  getStateInformation/setStateInformation
pub struct PluginInstance {
    instance_id:    String,
    plugin_info:    PluginInfo,
    bypassed:       Arc<RwLock<bool>>,
    parameters:     Arc<RwLock<Vec<PluginParameter>>>,
    /// The real VST3 audio processor — None if loading failed or format is non-VST3.
    vst3_processor: Mutex<Option<Vst3Processor>>,
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

        // Try to load a real VST3 audio processor (only for VST3 format).
        // Failure is non-fatal; the instance still works in pass-through mode.
        let vst3_processor = if plugin_info.format == crate::plugins::types::PluginFormat::VST3 {
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

        Ok(Self {
            instance_id,
            plugin_info,
            bypassed:       Arc::new(RwLock::new(false)),
            parameters:     Arc::new(RwLock::new(Vec::new())),
            vst3_processor: Mutex::new(vst3_processor),
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
    pub fn process_stereo(&self, left: &mut [f32], right: &mut [f32]) {
        if self.is_bypassed() {
            return; // Pass through unchanged — matches LightHost bypass logic
        }
        if let Some(mut guard) = self.vst3_processor.try_lock() {
            if let Some(ref mut proc) = *guard {
                proc.process_stereo(left, right);
            }
            // No processor loaded → pass through (e.g. VST2 or CLAP — not yet supported)
        }
        // Lock contended → pass through non-blocking (real-time safe)
    }

    /// Serialize plugin state as raw bytes (mirrors LightHost's `getStateInformation`).
    #[allow(dead_code)]
    pub fn get_state_binary(&self) -> Vec<u8> {
        if let Some(guard) = self.vst3_processor.try_lock() {
            if let Some(ref proc) = *guard {
                return proc.get_state();
            }
        }
        Vec::new()
    }

    /// Restore plugin state from raw bytes (mirrors LightHost's `setStateInformation`).
    #[allow(dead_code)]
    pub fn set_state_binary(&self, data: &[u8]) {
        if let Some(guard) = self.vst3_processor.try_lock() {
            if let Some(ref proc) = *guard {
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

    /// Set parameter value
    pub fn set_parameter(&self, param_id: u32, value: f64) {
        let mut params = self.parameters.write();
        if let Some(param) = params.iter_mut().find(|p| p.id == param_id) {
            param.value = value.clamp(param.min, param.max);
            // TODO: Send to actual plugin
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
