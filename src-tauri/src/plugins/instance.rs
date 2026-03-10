use std::sync::Arc;
use parking_lot::RwLock;
use crate::plugins::types::{PluginInfo, PluginInstanceInfo, PluginParameter};
use anyhow::Result;

/// Represents a loaded plugin instance
#[allow(dead_code)]
pub struct PluginInstance {
    instance_id: String,
    plugin_info: PluginInfo,
    bypassed: Arc<RwLock<bool>>,
    parameters: Arc<RwLock<Vec<PluginParameter>>>,
}

impl PluginInstance {
    /// Create a new plugin instance
    pub fn new(plugin_info: PluginInfo) -> Result<Self> {
        let instance_id = format!("instance_{}", uuid::Uuid::new_v4());
        
        log::info!("Creating plugin instance: {} ({})", plugin_info.name, instance_id);

        // Create default parameters for demonstration
        let default_params = vec![
            PluginParameter { id: 0, name: "Gain".to_string(), value: 0.0, min: -12.0, max: 12.0, default: 0.0 },
            PluginParameter { id: 1, name: "Mix".to_string(), value: 100.0, min: 0.0, max: 100.0, default: 100.0 },
            PluginParameter { id: 2, name: "Output".to_string(), value: 0.0, min: -12.0, max: 12.0, default: 0.0 },
        ];

        Ok(Self {
            instance_id,
            plugin_info,
            bypassed: Arc::new(RwLock::new(false)),
            parameters: Arc::new(RwLock::new(default_params)),
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

    /// Process audio buffer
    #[allow(dead_code)]
    pub fn process(&self, buffer: &mut [f32]) {
        if self.is_bypassed() {
            return; // Pass through without processing
        }

        // TODO: Actual plugin processing using clack-host
        // For now, just silence
        for sample in buffer.iter_mut() {
            *sample *= 0.5; // Reduce volume as placeholder
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

    /// Load a plugin and create an instance
    pub fn load_plugin(&self, plugin_info: PluginInfo) -> Result<String> {
        let instance = Arc::new(PluginInstance::new(plugin_info)?);
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

    /// Process audio through all instances in chain
    #[allow(dead_code)]
    pub fn process_chain(&self, buffer: &mut [f32]) {
        let instances = self.instances.read();
        for instance in instances.iter() {
            instance.process(buffer);
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
