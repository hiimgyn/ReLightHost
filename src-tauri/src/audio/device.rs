use cpal::traits::{DeviceTrait, HostTrait};
use crate::audio::types::AudioDeviceInfo;
use anyhow::Result;

pub struct AudioDevice;

impl AudioDevice {
    /// List all available audio devices from ALL available hosts (WASAPI, ASIO, DirectSound, etc.)
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>> {
        let mut devices = Vec::new();
        let available_hosts = cpal::available_hosts();

        for host_id in available_hosts {
            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            let host_type = format!("{:?}", host_id);

            let default_output_name = host
                .default_output_device()
                .and_then(|d| d.name().ok());
            let default_input_name = host
                .default_input_device()
                .and_then(|d| d.name().ok());

            // Output devices
            if let Ok(outputs) = host.output_devices() {
                for device in outputs {
                    let Ok(name) = device.name() else { continue };
                    let is_default = Some(&name) == default_output_name.as_ref();
                    let channels = device
                        .supported_output_configs()
                        .ok()
                        .and_then(|mut it| it.next())
                        .map(|c| c.channels() as usize)
                        .unwrap_or(2);
                    devices.push(AudioDeviceInfo {
                        id: format!("out_{}", name),
                        name: format!("{} (Output)", name),
                        is_default,
                        input_channels: 0,
                        output_channels: channels,
                        host_type: host_type.clone(),
                    });
                }
            }

            // Input devices
            if let Ok(inputs) = host.input_devices() {
                for device in inputs {
                    let Ok(name) = device.name() else { continue };
                    let is_default = Some(&name) == default_input_name.as_ref();
                    let channels = device
                        .supported_input_configs()
                        .ok()
                        .and_then(|mut it| it.next())
                        .map(|c| c.channels() as usize)
                        .unwrap_or(2);
                    devices.push(AudioDeviceInfo {
                        id: format!("in_{}", name),
                        name: format!("{} (Input)", name),
                        is_default,
                        input_channels: channels,
                        output_channels: 0,
                        host_type: host_type.clone(),
                    });
                }
            }
        }

        // If no hosts returned devices (unlikely), fall back to default WASAPI host
        if devices.is_empty() {
            let host = cpal::default_host();
            let host_type = format!("{:?}", host.id());
            if let Some(Ok(name)) = host.default_output_device().map(|d| d.name()) {
                devices.push(AudioDeviceInfo {
                    id: format!("out_{}", name),
                    name: format!("{} (Output)", name),
                    is_default: true,
                    input_channels: 0,
                    output_channels: 2,
                    host_type: host_type.clone(),
                });
            }
        }

        Ok(devices)
    }

    /// Find an input cpal::Device by its stored ID ("in_{device_name}")
    pub fn find_input_device(device_id: &str) -> Option<cpal::Device> {
        let device_name = device_id.strip_prefix("in_").unwrap_or(device_id);
        for host_id in cpal::available_hosts() {
            let Ok(host) = cpal::host_from_id(host_id) else { continue };
            if let Ok(inputs) = host.input_devices() {
                for dev in inputs {
                    if dev.name().ok().as_deref() == Some(device_name) {
                        return Some(dev);
                    }
                }
            }
        }
        None
    }

    /// Find an output cpal::Device by its stored ID ("out_{device_name}")
    pub fn find_output_device(device_id: &str) -> Option<cpal::Device> {
        let device_name = device_id.strip_prefix("out_").unwrap_or(device_id);
        for host_id in cpal::available_hosts() {
            let Ok(host) = cpal::host_from_id(host_id) else { continue };
            if let Ok(outputs) = host.output_devices() {
                for dev in outputs {
                    if dev.name().ok().as_deref() == Some(device_name) {
                        return Some(dev);
                    }
                }
            }
        }
        None
    }
}
