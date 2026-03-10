use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::sync::Mutex;
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleRate, StreamConfig};
use ringbuf::{HeapRb, traits::{Producer, Consumer, Split}};

use crate::audio::types::{AudioStatus, AudioConfig};
use crate::audio::device::AudioDevice;
use crate::audio::vu_meter::VUMeter;

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
    /// VU meter for output level monitoring
    vu_meter:    Arc<VUMeter>,
    /// Output mute — when true the output callback writes silence.
    muted:       Arc<AtomicBool>,
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            config:      Arc::new(RwLock::new(AudioConfig::default())),
            status:      Arc::new(RwLock::new(AudioStatus::default())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            monitoring:  Mutex::new(None),
            process_fn:  Arc::new(Mutex::new(None)),
            vu_meter:    Arc::new(VUMeter::new()),
            muted:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Register the plugin-chain callback.
    pub fn set_process_callback<F>(&self, f: F)
    where
        F: Fn(&mut [f32], &mut [f32]) + Send + 'static,
    {
        *self.process_fn.lock().unwrap() = Some(Box::new(f));
    }

    /// Start audio engine
    pub fn start(&self) -> Result<()> {
        let config = self.config.read().clone();
        
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

    /// Toggle real-time input monitoring (routes input device → plugin chain → output device).
    ///
    /// # ASIO note
    /// ASIO is full-duplex: input and output are driven by a single driver
    /// callback at the exact same buffer size.  We honour the configured
    /// `buffer_size` and `sample_rate` in the StreamConfig instead of falling
    /// back to `default_*_config()` so the driver doesn't refuse the request.
    ///
    /// The ring buffer capacity is set to 4× the configured buffer size
    /// (stereo samples) — enough for two full blocks without adding noticeable
    /// latency.
    pub fn toggle_monitoring(&self, enabled: bool) -> Result<()> {
        if !enabled {
            *self.monitoring.lock().unwrap() = None;
            self.status.write().is_monitoring = false;
            log::info!("Input monitoring stopped");
            return Ok(());
        }

        // Hold the monitoring mutex for the entire setup so that a second call
        // (e.g. React StrictMode double-effect) cannot race and create duplicate
        // streams — which caused STATUS_ACCESS_VIOLATION when the first set of
        // streams was dropped mid-callback.
        let mut monitoring_guard = self.monitoring.lock().unwrap();
        if monitoring_guard.is_some() {
            log::info!("Audio stream already running — ignoring duplicate start");
            return Ok(());
        }

        let config = self.config.read().clone();

        // -----------------------------------------------------------------
        // Resolve cpal devices
        //
        // Full-duplex ASIO insert (e.g. Voicemeeter insert): when the same
        // ASIO device name is used for both input AND output we MUST obtain
        // both Device objects from the SAME cpal::Host instance.  Using two
        // separate host instances (as find_input_device / find_output_device
        // each do) creates two independent ASIO driver singletons with
        // separate bufferSwitch callbacks — input data never reaches the
        // output ring buffer, producing silence.
        // -----------------------------------------------------------------
        let input_is_asio = config.input_device_id.as_deref()
            .map(|id| id.starts_with("asio_")).unwrap_or(false);
        let output_is_asio = config.output_device_id.as_deref()
            .map(|id| id.starts_with("asio_")).unwrap_or(false);

        let in_asio_name  = config.input_device_id.as_deref().and_then(|id| id.strip_prefix("asio_"));
        let out_asio_name = config.output_device_id.as_deref().and_then(|id| id.strip_prefix("asio_"));
        let same_asio_device = input_is_asio && output_is_asio && in_asio_name == out_asio_name;

        let (input_device, output_device) = if same_asio_device {
            let asio_name = in_asio_name.unwrap(); // safe: guarded above
            log::info!("ASIO full-duplex insert mode: using shared host for '{}'", asio_name);
            AudioDevice::find_asio_device_pair(asio_name)
                .ok_or_else(|| anyhow::anyhow!("ASIO device '{}' not found for insert mode", asio_name))?
        } else {
            let inp = config.input_device_id.as_deref()
                .and_then(AudioDevice::find_input_device)
                .or_else(|| cpal::default_host().default_input_device())
                .ok_or_else(|| anyhow::anyhow!("No input device available"))?;
            let out = config.output_device_id.as_deref()
                .and_then(AudioDevice::find_output_device)
                .or_else(|| cpal::default_host().default_output_device())
                .ok_or_else(|| anyhow::anyhow!("No output device available"))?;
            (inp, out)
        };

        // -----------------------------------------------------------------
        // Build StreamConfigs.
        //
        // For ASIO: the driver owns the sample rate and buffer size — we MUST
        // use whatever the driver reports via default_*_config(), otherwise
        // build_input_stream / build_output_stream will return an
        // "unsupported stream config" error (this is the case with VoiceMeeter
        // Virtual ASIO which is typically locked at 44100 Hz in the driver).
        //
        // For WASAPI / other hosts: use the user-configured sample rate as a hint.
        // -----------------------------------------------------------------
        let build_config = |device: &cpal::Device, is_input: bool, is_asio: bool| -> Result<(StreamConfig, usize)> {
            let default_cfg = if is_input {
                device.default_input_config()
            } else {
                device.default_output_config()
            }.map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

            let channels = default_cfg.channels() as usize;
            // ASIO: let the driver decide sample rate (it controls the HW clock).
            // Non-ASIO: pass the user-configured rate as a preference.
            let sample_rate = if is_asio {
                let driver_rate = default_cfg.sample_rate().0;
                if driver_rate != config.sample_rate {
                    log::warn!(
                        "ASIO driver sample rate {} Hz differs from configured {} Hz; \
                         using driver rate. Change VoiceMeeter or ASIO panel to {} Hz \
                         if you want them to match.",
                        driver_rate, config.sample_rate, config.sample_rate
                    );
                }
                driver_rate
            } else {
                config.sample_rate
            };
            let stream_cfg = StreamConfig {
                channels: default_cfg.channels(),
                sample_rate: SampleRate(sample_rate),
                // BufferSize::Default lets the ASIO driver report its own block size;
                // WASAPI treats it as a hint. The output callback handles variable
                // frame counts via left_buf/right_buf dynamic resizing.
                buffer_size: BufferSize::Default,
            };
            Ok((stream_cfg, channels))
        };

        let (in_cfg, input_channels)  = build_config(&input_device,  true,  input_is_asio)?;
        let (out_cfg, output_channels) = build_config(&output_device, false, output_is_asio)?;

        // -----------------------------------------------------------------
        // Lock-free SPSC ring buffer — stereo, sized by mode:
        //  • Same-ASIO full-duplex: input fires synchronously before output
        //    within the same bufferSwitch → 2 stereo frames is enough.
        //    Keep a small margin (4×) to absorb any block-size discrepancy.
        //  • Cross-device (WASAPI or different ASIO drivers): clocks can
        //    drift; keep the existing 8× safety margin.
        // Producer  → input callback  (audio thread, no alloc, no lock)
        // Consumer  → output callback (audio thread, no alloc, no lock)
        // -----------------------------------------------------------------
        let buf_capacity = if same_asio_device {
            (config.buffer_size as usize).max(2048) * 4 * 2  // 4 frames × stereo
        } else {
            (config.buffer_size as usize).max(4096) * 8 * 2  // 8 frames × stereo
        };
        let rb = HeapRb::<f32>::new(buf_capacity);
        let (mut producer, mut consumer) = rb.split();

        // -----------------------------------------------------------------
        // Input stream — de-interleave and push into ring buffer
        // -----------------------------------------------------------------
        let in_stream = input_device
            .build_input_stream(
                &in_cfg,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    for chunk in data.chunks(input_channels.max(1)) {
                        // Always produce exactly 2 samples (L, R) per frame
                        let l = chunk.first().copied().unwrap_or(0.0);
                        let r = if input_channels >= 2 {
                            chunk.get(1).copied().unwrap_or(0.0)
                        } else {
                            l  // mono → duplicate to both channels
                        };
                        // Non-blocking: if the ring buffer is full we drop the
                        // frame rather than blocking the realtime thread.
                        let _ = producer.try_push(l);
                        let _ = producer.try_push(r);
                    }
                },
                |err| log::error!("Input stream error: {err}"),
                None,
            )
            .map_err(|e| anyhow::anyhow!("Failed to build input stream: {e}"))?;

        // -----------------------------------------------------------------
        // Output stream — read ring buffer, process through plugin chain,
        //                 write to output.
        // Mirrors LightHost's AudioProcessorGraph:
        //   INPUT node -> plugin chain -> OUTPUT node
        // -----------------------------------------------------------------
        let process_fn = Arc::clone(&self.process_fn);
        let vu_meter = Arc::clone(&self.vu_meter);
        let muted = Arc::clone(&self.muted);
        let mut left_buf  = vec![0.0f32; config.buffer_size as usize];
        let mut right_buf = vec![0.0f32; config.buffer_size as usize];

        let out_stream = output_device
            .build_output_stream(
                &out_cfg,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frames = data.len() / output_channels.max(1);

                    // Grow bounce buffers if needed (happens at most once per
                    // session when the driver uses a larger block than configured).
                    if left_buf.len()  < frames { left_buf.resize(frames,  0.0); }
                    if right_buf.len() < frames { right_buf.resize(frames, 0.0); }

                    // Step 1: Drain ring buffer → L/R bounce buffers.
                    // Ring buffer samples are already interleaved as [L, R] pairs.
                    for frame in 0..frames {
                        left_buf[frame]  = consumer.try_pop().unwrap_or(0.0);
                        right_buf[frame] = consumer.try_pop().unwrap_or(0.0);
                    }

                    // Step 2: Run plugin chain (non-blocking try_lock).
                    // If the lock is contended (parameter update from UI thread)
                    // audio passes through unchanged — same as LightHost bypass.
                    if let Ok(guard) = process_fn.try_lock() {
                        if let Some(ref f) = *guard {
                            f(&mut left_buf[..frames], &mut right_buf[..frames]);
                        }
                    }

                    // Step 2.5: Update VU meter with processed audio
                    vu_meter.update(&left_buf[..frames], &right_buf[..frames]);

                    // Step 3: Re-interleave L/R → CPAL output buffer.
                    // If muted, write silence so the VU meter still shows levels
                    // but speakers hear nothing.
                    let is_muted = muted.load(Ordering::Relaxed);
                    for frame in 0..frames {
                        for ch in 0..output_channels {
                            data[frame * output_channels + ch] = if is_muted { 0.0 } else {
                                if ch % 2 == 0 { left_buf[frame] } else { right_buf[frame] }
                            };
                        }
                    }
                },
                |err| log::error!("Output stream error: {err}"),
                None,
            )
            .map_err(|e| anyhow::anyhow!("Failed to build output stream: {e}"))?;

        in_stream
            .play()
            .map_err(|e| anyhow::anyhow!("Failed to start input stream: {e}"))?;
        out_stream
            .play()
            .map_err(|e| anyhow::anyhow!("Failed to start output stream: {e}"))?;

        *monitoring_guard = Some(MonitoringStreams {
            _input: in_stream,
            _output: out_stream,
        });
        self.status.write().is_monitoring = true;
        log::info!("Input monitoring started ({}Hz, {} samples)", config.sample_rate, config.buffer_size);
        Ok(())
    }

    /// Set output mute state.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        log::info!("Audio output {}", if muted { "muted" } else { "unmuted" });
    }

    /// Get current mute state.
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
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
    
    /// Get current VU meter data
    pub fn get_vu_data(&self) -> crate::audio::vu_meter::VUData {
        self.vu_meter.get_data()
    }

    /// Set output device
    pub fn set_output_device(&self, device_id: Option<String>) -> Result<()> {
        self.config.write().output_device_id = device_id;
        log::info!("Output device updated");
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set input device
    pub fn set_input_device(&self, device_id: Option<String>) -> Result<()> {
        self.config.write().input_device_id = device_id;
        log::info!("Input device updated");
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set sample rate
    pub fn set_sample_rate(&self, rate: u32) -> Result<()> {
        self.config.write().sample_rate = rate;
        log::info!("Sample rate set to {}Hz", rate);
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }

    /// Set buffer size
    pub fn set_buffer_size(&self, size: u32) -> Result<()> {
        self.config.write().buffer_size = size;
        log::info!("Buffer size set to {} samples", size);
        if self.status.read().is_monitoring {
            self.toggle_monitoring(false)?;
            self.toggle_monitoring(true)?;
        }
        Ok(())
    }
}

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}

