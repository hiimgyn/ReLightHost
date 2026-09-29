use std::sync::Arc;


use crate::domain::preset::Preset;
use crate::plugins::PluginInstanceManager;

/// Build a preset snapshot from a plugin manager directly.
pub fn build_chain_preset_from_manager(
    plugin_manager: &Arc<PluginInstanceManager>,
    name: impl Into<String>,
) -> Preset {
    // Single lock acquisition; instances and their info stay in chain order,
    // so we can zip by position instead of re-searching by instance_id per item.
    let instances = plugin_manager.get_instances_arc();
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
