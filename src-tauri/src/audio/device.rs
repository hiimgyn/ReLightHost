use cpal::traits::{DeviceTrait, HostTrait};
use crate::audio::types::AudioDeviceInfo;
use anyhow::Result;

pub struct AudioDevice;

/// ASIO host IDs — these drivers are full-duplex: one device handles both
/// input and output simultaneously with a single callback.
fn is_asio_host(host_id: cpal::HostId) -> bool {
    format!("{host_id:?}").contains("Asio")
}

impl AudioDevice {
    /// List all available audio devices from ALL available hosts (WASAPI, ASIO, DirectSound, etc.)
    ///
    /// ASIO devices are full-duplex: a single device appears in both the
    /// `input_devices()` and `output_devices()` iterators with the same name.
    /// We merge those into one entry so the UI shows a single ASIO device with
    /// both input_channels and output_channels populated.
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>> {
        let mut devices = Vec::new();
        let available_hosts = cpal::available_hosts();

        for host_id in available_hosts {
            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            let host_type = format!("{host_id:?}");
            let asio = is_asio_host(host_id);

            let default_output_name = host
                .default_output_device()
                .and_then(|d| d.name().ok());
            let default_input_name = host
                .default_input_device()
                .and_then(|d| d.name().ok());

            if asio {
                // For ASIO, merge input + output sides by device name.
                // Some virtual ASIO drivers (e.g. VoiceMeeter) only appear in
                // input_devices() OR output_devices() — enumerate BOTH and union
                // by name so every ASIO device is visible to the user.
                let mut seen: std::collections::HashMap<String, (usize, usize)> =
                    std::collections::HashMap::new();

                if let Ok(outputs) = host.output_devices() {
                    for device in outputs {
                        let Ok(name) = device.name() else { continue };
                        let out_ch = device
                            .supported_output_configs().ok()
                            .and_then(|mut it| it.next())
                            .map(|c| c.channels() as usize)
                            .unwrap_or(2);
                        seen.entry(name).or_insert((0, 0)).1 = out_ch;
                    }
                }

                if let Ok(inputs) = host.input_devices() {
                    for device in inputs {
                        let Ok(name) = device.name() else { continue };
                        let in_ch = device
                            .supported_input_configs().ok()
                            .and_then(|mut it| it.next())
                            .map(|c| c.channels() as usize)
                            .unwrap_or(2);
                        seen.entry(name).or_insert((0, 0)).0 = in_ch;
                    }
                }

                for (name, (in_channels, out_channels)) in seen {
                    let is_default = Some(&name) == default_output_name.as_ref()
                        || Some(&name) == default_input_name.as_ref();
                    // Shared ID: "asio_{name}" — no in_/out_ prefix so both
                    // find_input_device and find_output_device resolve to the
                    // same cpal device.
                    devices.push(AudioDeviceInfo {
                        id: format!("asio_{name}"),
                        name: name.clone(),
                        is_default,
                        input_channels: in_channels,
                        output_channels: out_channels,
                        host_type: host_type.clone(),
                    });
                }
            } else {
                // Non-ASIO (WASAPI, DirectSound) — keep separate in/out entries.

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
                            id: format!("out_{name}"),
                            name: format!("{name} (Output)"),
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
                            id: format!("in_{name}"),
                            name: format!("{name} (Input)"),
                            is_default,
                            input_channels: channels,
                            output_channels: 0,
                            host_type: host_type.clone(),
                        });
                    }
                }
            }
        }

        // Fallback to default WASAPI output if no devices were found.
        if devices.is_empty() {
            let host = cpal::default_host();
            let host_type = format!("{:?}", host.id());
            if let Some(Ok(name)) = host.default_output_device().map(|d| d.name()) {
                devices.push(AudioDeviceInfo {
                    id: format!("out_{name}"),
                    name: format!("{name} (Output)"),
                    is_default: true,
                    input_channels: 0,
                    output_channels: 2,
                    host_type,
                });
            }
        }

        Ok(devices)
    }

    /// Find a cpal input device by ID.
    ///
    /// Supports:
    /// - `"asio_{name}"` — full-duplex ASIO device (returned via the ASIO host's
    ///   input_devices iterator).
    /// - `"in_{name}"` — regular non-ASIO input device.
    pub fn find_input_device(device_id: &str) -> Option<cpal::Device> {
        let (prefix, device_name) = if let Some(n) = device_id.strip_prefix("asio_") {
            ("asio", n)
        } else if let Some(n) = device_id.strip_prefix("in_") {
            ("in", n)
        } else {
            ("in", device_id)
        };

        for host_id in cpal::available_hosts() {
            // For ASIO IDs only search ASIO hosts; for in_ IDs skip ASIO hosts.
            if prefix == "asio" && !is_asio_host(host_id) { continue; }
            if prefix == "in"   &&  is_asio_host(host_id) { continue; }

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

    /// Find a cpal output device by ID.
    ///
    /// Supports:
    /// - `"asio_{name}"` — full-duplex ASIO device.
    /// - `"out_{name}"` — regular non-ASIO output device.
    pub fn find_output_device(device_id: &str) -> Option<cpal::Device> {
        let (prefix, device_name) = if let Some(n) = device_id.strip_prefix("asio_") {
            ("asio", n)
        } else if let Some(n) = device_id.strip_prefix("out_") {
            ("out", n)
        } else {
            ("out", device_id)
        };

        for host_id in cpal::available_hosts() {
            if prefix == "asio" && !is_asio_host(host_id) { continue; }
            if prefix == "out"  &&  is_asio_host(host_id) { continue; }

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

    /// For ASIO full-duplex insert (e.g. Voicemeeter insert): find BOTH the input
    /// and output `cpal::Device` for the same ASIO driver from a **single host
    /// instance**.  This is required because cpal's ASIO backend keys the
    /// underlying driver singleton on the `cpal::Host` — using two separate hosts
    /// (which `find_input_device` + `find_output_device` each create internally)
    /// gives two independent ASIO instances with unrelated bufferSwitch callbacks,
    /// so input data never reaches the output ring buffer.
    pub fn find_asio_device_pair(device_name: &str) -> Option<(cpal::Device, cpal::Device)> {
        for host_id in cpal::available_hosts() {
            if !is_asio_host(host_id) { continue; }
            let Ok(host) = cpal::host_from_id(host_id) else { continue };

            let input = host.input_devices().ok()
                .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(device_name)));
            let output = host.output_devices().ok()
                .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(device_name)));

            if let (Some(inp), Some(out)) = (input, output) {
                return Some((inp, out));
            }
        }
        None
    }
}

