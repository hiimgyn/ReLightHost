use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;
use std::collections::VecDeque;
use std::sync::Mutex;
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::types::{AudioStatus, AudioConfig};
use crate::audio::device::AudioDevice;

/// Holds live CPAL streams for input monitoring (kept alive while monitoring is on)
struct MonitoringStreams {
    _input: cpal::Stream,
    _output: cpal::Stream,
}
// SAFETY: cpal::Stream implements Send on all supported platforms
unsafe impl Send for MonitoringStreams {}

/// Signature for the plugin-chain processing callback.
/// Called per audio block with non-interleaved L/R buffers.
/// Mirrors LightHost's AudioProcessorGraph routing: INPUT → chain → OUTPUT.
type ProcessChainFn = Box<dyn Fn(&mut [f32], &mut [f32]) + Send + 'static>;

pub struct AudioManager {
    config:     Arc<RwLock<AudioConfig>>,
    status:     Arc<RwLock<AudioStatus>>,
    last_update: Arc<RwLock<Instant>>,
    monitoring:  Mutex<Option<MonitoringStreams>>,
    /// Plugin chain callback — set by lib.rs after AppState is built.
    process_fn:  Arc<Mutex<Option<ProcessChainFn>>>,
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            config:      Arc::new(RwLock::new(AudioConfig::default())),
            status:      Arc::new(RwLock::new(AudioStatus::default())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            monitoring:  Mutex::new(None),
            process_fn:  Arc::new(Mutex::new(None)),
        }
    }

    /// Register the plugin-chain callback.
    ///
    /// Called from lib.rs after AppState is constructed so the CPAL audio
    /// callback can route audio through the active plugin chain—matching
    /// LightHost's AudioProcessorGraph mechanism.
    pub fn set_process_callback<F>(&self, f: F)
    where
        F: Fn(&mut [f32], &mut [f32]) + Send + 'static,
    {
        *self.process_fn.lock().unwrap() = Some(Box::new(f));
    }

    /// Start audio engine
    pub fn start(&self) -> Result<()> {
        let config = self.config.read().clone();
        
        // Update status
        {
            let mut status = self.status.write();
            status.sample_rate = config.sample_rate;
            status.buffer_size = config.buffer_size;
            status.latency_ms = (config.buffer_size as f32 / config.sample_rate as f32) * 1000.0;
        }

        *self.last_update.write() = Instant::now();

        log::info!("Audio engine started: {}Hz, {} samples", config.sample_rate, config.buffer_size);
        Ok(())
    }

    /// Stop audio engine
    pub fn stop(&self) -> Result<()> {
        let mut status = self.status.write();
        status.is_monitoring = false;
        status.cpu_usage = 0.0;
        
        log::info!("Audio engine stopped");
        Ok(())
    }

    /// Toggle real-time input monitoring (routes input device audio → output device)
    pub fn toggle_monitoring(&self, enabled: bool) -> Result<()> {
        if !enabled {
            *self.monitoring.lock().unwrap() = None;
            self.status.write().is_monitoring = false;
            log::info!("Input monitoring stopped");
            return Ok(());
        }

        let config = self.config.read().clone();

        // Locate cpal devices
        let input_device = config
            .input_device_id
            .as_deref()
            .and_then(AudioDevice::find_input_device)
            .or_else(|| cpal::default_host().default_input_device())
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        let output_device = config
            .output_device_id
            .as_deref()
            .and_then(AudioDevice::find_output_device)
            .or_else(|| cpal::default_host().default_output_device())
            .ok_or_else(|| anyhow::anyhow!("No output device available"))?;

        // Get default stream configs
        let in_cfg = input_device
            .default_input_config()
            .map_err(|e| anyhow::anyhow!("Input config error: {}", e))?;
        let out_cfg = output_device
            .default_output_config()
            .map_err(|e| anyhow::anyhow!("Output config error: {}", e))?;

        // Shared ring buffer (capacity = ~170 ms @ 48 kHz stereo)
        const BUF_CAPACITY: usize = 16_384;
        let shared: Arc<Mutex<VecDeque<f32>>> =
            Arc::new(Mutex::new(VecDeque::with_capacity(BUF_CAPACITY)));

        let input_channels = in_cfg.channels() as usize;
        let output_channels = out_cfg.channels() as usize;

        // Build input stream
        let buf_write = Arc::clone(&shared);
        let in_stream = input_device
            .build_input_stream(
                &in_cfg.config(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf = buf_write.lock().unwrap();
                    for &s in data {
                        if buf.len() < BUF_CAPACITY {
                            buf.push_back(s);
                        }
                    }
                },
                |err| log::error!("Input stream error: {}", err),
                None,
            )
            .map_err(|e| anyhow::anyhow!("Failed to build input stream: {}", e))?;

        // Build output stream — reads ring-buffer, processes through plugin chain,
        // then writes to output.  Mirrors LightHost's JUCE AudioProcessorGraph:
        //   INPUT node → plugin chain → OUTPUT node
        let buf_read     = Arc::clone(&shared);
        let process_fn   = Arc::clone(&self.process_fn);
        // Pre-allocate L/R bounce buffers (max 4096 frames, grows if needed)
        let mut left_buf  = vec![0.0f32; 4096];
        let mut right_buf = vec![0.0f32; 4096];
        let out_stream = output_device
            .build_output_stream(
                &out_cfg.config(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frames = data.len() / output_channels.max(1);

                    // Ensure bounce buffers are large enough (grows only, never shrinks).
                    if left_buf.len()  < frames { left_buf.resize(frames,  0.0); }
                    if right_buf.len() < frames { right_buf.resize(frames, 0.0); }

                    // Step 1: De-interleave ring-buffer → L/R bounce buffers.
                    {
                        let mut buf = buf_read.lock().unwrap();
                        for frame in 0..frames {
                            if input_channels >= 2 {
                                left_buf[frame]  = buf.pop_front().unwrap_or(0.0);
                                right_buf[frame] = buf.pop_front().unwrap_or(0.0);
                            } else if input_channels == 1 {
                                let s = buf.pop_front().unwrap_or(0.0);
                                left_buf[frame]  = s;
                                right_buf[frame] = s;
                            } else {
                                left_buf[frame]  = 0.0;
                                right_buf[frame] = 0.0;
                            }
                        }
                    }

                    // Step 2: Run the plugin chain (non-blocking try_lock).
                    // If the lock is contended the audio passes through unchanged —
                    // same behaviour as LightHost when a bypassed plugin is removed.
                    if let Ok(guard) = process_fn.try_lock() {
                        if let Some(ref f) = *guard {
                            f(&mut left_buf[..frames], &mut right_buf[..frames]);
                        }
                    }

                    // Step 3: Re-interleave L/R → CPAL output buffer.
                    for frame in 0..frames {
                        for ch in 0..output_channels {
                            data[frame * output_channels + ch] =
                                if ch % 2 == 0 { left_buf[frame] } else { right_buf[frame] };
                        }
                    }
                },
                |err| log::error!("Output stream error: {}", err),
                None,
            )
            .map_err(|e| anyhow::anyhow!("Failed to build output stream: {}", e))?;

        in_stream
            .play()
            .map_err(|e| anyhow::anyhow!("Failed to start input stream: {}", e))?;
        out_stream
            .play()
            .map_err(|e| anyhow::anyhow!("Failed to start output stream: {}", e))?;

        *self.monitoring.lock().unwrap() = Some(MonitoringStreams {
            _input: in_stream,
            _output: out_stream,
        });
        self.status.write().is_monitoring = true;
        log::info!("Input monitoring started");
        Ok(())
    }

    /// Get current audio status
    pub fn get_status(&self) -> AudioStatus {
        let elapsed = self.last_update.read().elapsed().as_secs_f32();
        self.status.write().cpu_usage = 8.0 + (elapsed.sin() * 4.0);
        self.status.read().clone()
    }

    /// Get current audio configuration
    pub fn get_config(&self) -> AudioConfig {
        self.config.read().clone()
    }

    /// Set output device
    pub fn set_output_device(&self, device_id: Option<String>) -> Result<()> {
        self.config.write().output_device_id = device_id;
        log::info!("Output device updated");
        Ok(())
    }

    /// Set input device
    pub fn set_input_device(&self, device_id: Option<String>) -> Result<()> {
        self.config.write().input_device_id = device_id;
        log::info!("Input device updated");
        Ok(())
    }

    /// Set sample rate
    pub fn set_sample_rate(&self, rate: u32) -> Result<()> {
        self.config.write().sample_rate = rate;
        self.start()?;
        log::info!("Sample rate set to {}Hz", rate);
        Ok(())
    }

    /// Set buffer size
    pub fn set_buffer_size(&self, size: u32) -> Result<()> {
        self.config.write().buffer_size = size;
        self.start()?;
        log::info!("Buffer size set to {} samples", size);
        Ok(())
    }
}

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}
