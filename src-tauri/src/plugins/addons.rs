//! Host registry of built-in add-ons.
use crate::plugins::types::{PluginFormat, PluginInfo, PluginParameter};
use crate::plugins::builtin::{compressor, deep_filter, noise_suppressor, voice};
use relight_addon_api::AddonDescriptor;

pub static ADDONS: &[&AddonDescriptor] = &[
    &noise_suppressor::ADDON,
    &deep_filter::ADDON,
    &compressor::ADDON,
    &voice::ADDON,
];

pub fn find(id: &str) -> Option<&'static AddonDescriptor> {
    ADDONS.iter().copied().find(|d| d.id == id)
}

/// f32 -> f64 through its shortest decimal form, so 0.1 stays 0.1.
fn f(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}

pub fn initial_params(d: &AddonDescriptor) -> Vec<PluginParameter> {
    d.params
        .iter()
        .map(|p| PluginParameter {
            id: p.id,
            name: p.name.into(),
            value: f(p.default),
            min: f(p.min),
            max: f(p.max),
            default: f(p.default),
        })
        .collect()
}

pub fn plugin_info(d: &AddonDescriptor) -> PluginInfo {
    PluginInfo {
        id: d.id.into(),
        name: d.name.into(),
        vendor: "ReLightHost".into(),
        version: d.version.to_string(),
        path: d.id.into(),
        format: PluginFormat::Builtin,
        category: d.category.into(),
        sub_index: 0,
    }
}
