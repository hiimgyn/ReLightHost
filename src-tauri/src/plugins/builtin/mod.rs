/// Built-in add-on DSP compiled into ReLightHost. Each file exports
/// `ADDON: AddonDescriptor`; the host registry is `plugins::addons`.
pub mod noise_suppressor;
pub mod deep_filter;
pub mod compressor;
pub mod voice;
#[cfg(test)]
mod golden_tests;
