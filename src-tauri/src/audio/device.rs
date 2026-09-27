use crate::audio::backend::{asio, wasapi};
use crate::audio::types::AudioDeviceInfo;
use anyhow::Result;

pub struct AudioDevice;

impl AudioDevice {
    /// List all available audio devices — ASIO drivers (registry-enumerated,
    /// full-duplex, one merged entry per driver, id `"asio_{name}"`) plus
    /// every active WASAPI render/capture endpoint (split into separate
    /// `"out_{id}"` / `"in_{id}"` entries). Matches the pre-existing id
    /// scheme exactly, so the frontend/commands layer sees no difference
    /// from the old cpal-based enumeration.
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>> {
        let mut devices = Vec::new();

        for d in asio::list_asio_devices() {
            devices.push(AudioDeviceInfo {
                id: format!("asio_{}", d.name),
                name: d.name,
                // ASIO has no registry notion of a "default" driver.
                is_default: false,
                input_channels: d.input_channels,
                output_channels: d.output_channels,
                host_type: "ASIO".to_string(),
            });
        }

        for d in wasapi::list_wasapi_devices() {
            if d.output_channels > 0 {
                devices.push(AudioDeviceInfo {
                    id: format!("out_{}", d.id),
                    name: format!("{} (Output)", d.name),
                    is_default: d.is_default,
                    input_channels: 0,
                    output_channels: d.output_channels,
                    host_type: "WASAPI".to_string(),
                });
            }
            if d.input_channels > 0 {
                devices.push(AudioDeviceInfo {
                    id: format!("in_{}", d.id),
                    name: format!("{} (Input)", d.name),
                    is_default: d.is_default,
                    input_channels: d.input_channels,
                    output_channels: 0,
                    host_type: "WASAPI".to_string(),
                });
            }
        }

        Ok(devices)
    }

    /// Resolves and re-validates an input device id (`"asio_{name}"`,
    /// `"in_{id}"`, or a bare WASAPI endpoint id) against the current
    /// enumeration, returning the canonical prefixed id if it still exists.
    /// Returning the id (rather than a device handle, as the old cpal-based
    /// version did) is enough — `manager.rs` re-derives ASIO-vs-WASAPI from
    /// this resolved id's prefix and calls the matching backend directly.
    pub fn find_input_device(device_id: &str) -> Option<String> {
        if let Some(name) = device_id.strip_prefix("asio_") {
            return asio::list_asio_devices()
                .into_iter()
                .any(|d| d.name == name && d.input_channels > 0)
                .then(|| format!("asio_{name}"));
        }
        let raw = device_id.strip_prefix("in_").unwrap_or(device_id);
        wasapi::list_wasapi_devices()
            .into_iter()
            .any(|d| d.id == raw && d.input_channels > 0)
            .then(|| format!("in_{raw}"))
    }

    /// Output counterpart of [`find_input_device`].
    pub fn find_output_device(device_id: &str) -> Option<String> {
        if let Some(name) = device_id.strip_prefix("asio_") {
            return asio::list_asio_devices()
                .into_iter()
                .any(|d| d.name == name && d.output_channels > 0)
                .then(|| format!("asio_{name}"));
        }
        let raw = device_id.strip_prefix("out_").unwrap_or(device_id);
        wasapi::list_wasapi_devices()
            .into_iter()
            .any(|d| d.id == raw && d.output_channels > 0)
            .then(|| format!("out_{raw}"))
    }

    /// Validates that `device_name` names a real ASIO driver supporting both
    /// directions — used for full-duplex insert mode (e.g. a Voicemeeter
    /// insert). Unlike the old cpal-based version there is only one real
    /// "handle" to return (a driver name, not a `cpal::Device` per
    /// direction), so both elements of the returned pair are the same
    /// canonical id; kept as a pair so callers don't need to change shape.
    pub fn find_asio_device_pair(device_name: &str) -> Option<(String, String)> {
        let dev = asio::list_asio_devices()
            .into_iter()
            .find(|d| d.name == device_name)?;
        if dev.input_channels > 0 && dev.output_channels > 0 {
            let id = format!("asio_{device_name}");
            Some((id.clone(), id))
        } else {
            None
        }
    }

    /// Canonical id of the default active WASAPI input device, if any —
    /// replaces the old `cpal::default_host().default_input_device()`
    /// fallback used when no device is configured, or the configured one no
    /// longer resolves.
    pub fn default_input_device_id() -> Option<String> {
        wasapi::list_wasapi_devices()
            .into_iter()
            .find(|d| d.is_default && d.input_channels > 0)
            .map(|d| format!("in_{}", d.id))
    }

    /// Output counterpart of [`default_input_device_id`].
    pub fn default_output_device_id() -> Option<String> {
        wasapi::list_wasapi_devices()
            .into_iter()
            .find(|d| d.is_default && d.output_channels > 0)
            .map(|d| format!("out_{}", d.id))
    }
}
