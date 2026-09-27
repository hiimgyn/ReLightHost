#![cfg(target_os = "windows")]

use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, DEVICE_STATE_ACTIVE, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// Decodes a COM-allocated wide string and frees the CoTaskMem allocation
/// the caller owns — `IMMDevice::GetId` and `PropVariantToStringAlloc` both
/// hand back a `PWSTR` the caller must release, and `windows-rs`'s `PWSTR`
/// has no `Drop` impl that does this automatically.
fn decode_and_free_pwstr(p: windows::core::PWSTR) -> Option<String> {
    let s = unsafe { p.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

pub struct WasapiDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub input_channels: usize,
    pub output_channels: usize,
}

fn ensure_com_initialized() {
    // Idempotent per-thread: CoInitializeEx returns S_FALSE (still Ok) if
    // already initialized on this thread; ignore "already initialized"
    // failures from a prior different concurrency model since enumeration
    // itself doesn't require a specific one beyond MULTITHREADED here.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Enumerates active WASAPI render (output) and capture (input) endpoints
/// via `IMMDeviceEnumerator`. Enumeration only — no stream is opened, so
/// channel counts are a conservative stereo default (see note below).
pub fn list_wasapi_devices() -> Vec<WasapiDeviceInfo> {
    ensure_com_initialized();
    let mut out = Vec::new();
    let enumerator: windows::core::Result<IMMDeviceEnumerator> =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) };
    let Ok(enumerator) = enumerator else {
        return out;
    };

    for (flow, is_output) in [(eRender, true), (eCapture, false)] {
        let Ok(collection) = (unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) })
        else {
            continue;
        };
        let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }
            .ok()
            .and_then(|d| unsafe { d.GetId() }.ok())
            .and_then(decode_and_free_pwstr);

        let count = unsafe { collection.GetCount() }.unwrap_or(0);
        for i in 0..count {
            let Ok(device) = (unsafe { collection.Item(i) }) else {
                continue;
            };
            let Ok(id_pwstr) = (unsafe { device.GetId() }) else {
                continue;
            };
            let Some(id) = decode_and_free_pwstr(id_pwstr) else {
                continue;
            };
            let name = device_friendly_name(&device).unwrap_or_else(|| "<unknown>".to_string());
            let is_default = default_id.as_deref() == Some(id.as_str());
            out.push(WasapiDeviceInfo {
                id,
                name,
                is_default,
                input_channels: if is_output { 0 } else { 2 },
                output_channels: if is_output { 2 } else { 0 },
            });
        }
    }
    out
}

fn device_friendly_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
    let store = unsafe { device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ) }.ok()?;
    // GetValue's PROPVARIANT owns COM-allocated memory (e.g. the string
    // buffer behind its VT_LPWSTR variant) that must be released via
    // PropVariantClear regardless of what happens below — done via the
    // `mut` binding freed at the end of this function, not by any Drop impl.
    let mut prop = unsafe { store.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _) }.ok()?;
    let name = unsafe { PropVariantToStringAlloc(&prop) }
        .ok()
        .and_then(decode_and_free_pwstr);
    unsafe {
        let _ = PropVariantClear(&mut prop);
    }
    name
}

#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn list_wasapi_devices_returns_at_least_the_default_render_device() {
        // Every Windows dev/CI machine has at least a default render
        // endpoint (even if it's a dummy/HDMI one) — this is a real,
        // no-mock check that CoInitializeEx + enumeration succeed.
        let devices = list_wasapi_devices();
        assert!(
            devices.iter().any(|d| d.output_channels > 0),
            "expected at least one render endpoint"
        );
    }
}
