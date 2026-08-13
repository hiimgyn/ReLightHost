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
    // Single lock acquisition; instances and their info stay in chain order,
    // so we can zip by position instead of re-searching by instance_id per item.
    let instances = plugin_manager.read().get_instances_arc();
    let chain: Vec<_> = instances.iter().map(|i| i.get_info()).collect();
    let mut preset = Preset::new(name.into(), chain);

    for (preset_plugin, instance) in preset.plugin_chain.iter_mut().zip(instances.iter()) {
        let blob = instance.get_state_binary();
        if !blob.is_empty() {
            preset_plugin.vst3_state = Some(blob);
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