use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::domain::preset::Preset;
use crate::plugins::PluginInstanceManager;

/// Build a preset snapshot from a plugin manager directly.
pub fn build_chain_preset_from_manager(
    plugin_manager: &Arc<RwLock<PluginInstanceManager>>,
    name: impl Into<String>,
) -> Preset {
    let chain = plugin_manager.read().get_instances();
    let mut preset = Preset::new(name.into(), chain.clone());

    let manager = plugin_manager.read();
    for (preset_plugin, info) in preset.plugin_chain.iter_mut().zip(chain.iter()) {
        if let Some(instance) = manager.get_instance(&info.instance_id) {
            let blob = instance.get_state_binary();
            if !blob.is_empty() {
                preset_plugin.vst3_state = Some(blob);
            }
        }
    }

    preset
}

/// Stable hash of a preset snapshot for autosave dedupe.
pub fn preset_hash_bytes(preset: &Preset) -> Option<u64> {
    let bytes = serde_json::to_vec(preset).ok()?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}