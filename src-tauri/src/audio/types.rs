use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStatus {
    pub is_monitoring: bool,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub cpu_usage: f32,
    pub latency_ms: f32,
}

impl Default for AudioStatus {
    fn default() -> Self {
        Self {
            is_monitoring: false,
            sample_rate: 48000,
            buffer_size: 1024,
            cpu_usage: 0.0,
            latency_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub input_channels: usize,
    pub output_channels: usize,
    pub host_type: String, // ASIO, WASAPI, DirectSound, CoreAudio, ALSA, JACK
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub output_device_id: Option<String>,
    pub input_device_id: Option<String>,
    /// Optional second output device (e.g. VB-Audio Virtual Cable / VAIO).
    /// When set, processed audio is mirrored to this device simultaneously
    /// with the primary output — useful for routing to OBS / Discord while
    /// also monitoring through speakers.
    pub virtual_output_device_id: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 1024,
            output_device_id: None,
            input_device_id: None,
            virtual_output_device_id: None,
        }
    }
}
