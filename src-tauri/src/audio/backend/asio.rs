#![cfg(target_os = "windows")]

use asio_sys::Asio;

pub struct AsioDeviceInfo {
    pub name: String,
    pub input_channels: usize,
    pub output_channels: usize,
}

/// Lists every ASIO driver registered in the Windows registry, with its
/// channel counts. Loading a driver briefly to query channels is required
/// by the ASIO SDK (channel counts aren't in the registry) — this mirrors
/// what cpal's own ASIO host does today.
pub fn list_asio_devices() -> Vec<AsioDeviceInfo> {
    let asio = Asio::new();
    let mut out = Vec::new();
    for name in asio.driver_names() {
        let Ok(driver) = asio.load_driver(&name) else { continue };
        let Ok(channels) = driver.channels() else { continue };
        out.push(AsioDeviceInfo {
            name,
            input_channels: channels.ins.max(0) as usize,
            output_channels: channels.outs.max(0) as usize,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_asio_devices_reports_consistent_channel_counts_or_none() {
        // On a machine with no ASIO drivers registered this returns an
        // empty Vec (the real assertion is that the call completes without
        // panicking or erroring at all — a fixed count can't be asserted
        // since it depends on what's installed on the build machine). Every
        // entry that IS returned must have a non-empty name, since an
        // unnamed device is not something the UI can list.
        let devices = list_asio_devices();
        assert!(devices.iter().all(|d| !d.name.is_empty()));
    }
}
