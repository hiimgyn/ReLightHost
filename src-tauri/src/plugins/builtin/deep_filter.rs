use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use ringbuf::{HeapRb, HeapProd, HeapCons, traits::{Producer, Consumer, Split, Observer}};
use df::tract::{DfParams, DfTract, RuntimeParams};
use tract_core::ndarray::Array2;

use super::BuiltinProcessor;

pub const ID: &str = "builtin::deep_filter";
const HOP_SIZE: usize = 480; // 10ms frame at 48kHz
const RB_CAPACITY: usize = 48000; // 1 second buffer

pub struct DeepFilterProcessor {
    prod_to_worker_l: HeapProd<f32>,
    prod_to_worker_r: HeapProd<f32>,
    cons_from_worker_l: HeapCons<f32>,
    cons_from_worker_r: HeapCons<f32>,

    dry_l: VecDeque<f32>,
    dry_r: VecDeque<f32>,

    // Parameters (shared with worker thread)
    atten_lim_bits: Arc<AtomicU32>,
    post_filter_bits: Arc<AtomicU32>,
    mix: f32,
    output_gain: f32,

    // Live Metrics (reported from worker thread)
    last_vad_bits: Arc<AtomicU32>,

    // Worker thread lifecycle
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DeepFilterProcessor {
    pub fn new(sample_rate: f32) -> Option<Self> {
        if (sample_rate - 48000.0).abs() > 1.0 {
            log::warn!(
                "DeepFilterProcessor requires 48 kHz; got {sample_rate} Hz — \
                 plugin will run in pass-through mode to avoid pitch/speed artifacts"
            );
            return None;
        }

        // Set up lock-free SPSC ring buffers connecting the real-time audio thread
        // to the background neural network inference worker thread.
        let rb_in_l = HeapRb::<f32>::new(RB_CAPACITY);
        let (prod_to_worker_l, mut cons_to_worker_l) = rb_in_l.split();

        let rb_in_r = HeapRb::<f32>::new(RB_CAPACITY);
        let (prod_to_worker_r, mut cons_to_worker_r) = rb_in_r.split();

        let rb_out_l = HeapRb::<f32>::new(RB_CAPACITY);
        let (mut prod_from_worker_l, cons_from_worker_l) = rb_out_l.split();

        let rb_out_r = HeapRb::<f32>::new(RB_CAPACITY);
        let (mut prod_from_worker_r, cons_from_worker_r) = rb_out_r.split();

        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);

        let atten_lim_bits = Arc::new(AtomicU32::new(24.0_f32.to_bits()));
        let worker_atten_lim = Arc::clone(&atten_lim_bits);

        let post_filter_bits = Arc::new(AtomicU32::new(0.2_f32.to_bits()));
        let worker_post_filter = Arc::clone(&post_filter_bits);

        let last_vad_bits = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
        let worker_last_vad = Arc::clone(&last_vad_bits);

        // Spawn dedicated worker thread for DeepFilterNet inference
        let worker_handle = std::thread::Builder::new()
            .name("relight-deepfilter-worker".to_string())
            .spawn(move || {
                // Boost thread priority on Windows
                #[cfg(target_os = "windows")]
                let _mmcss = crate::audio::mmcss::boost_current_thread_to_pro_audio();

                // Initialize DfTract entirely inside the worker thread (DfTract is !Send, so owning
                // it solely on this thread satisfies Rust safety guarantees with zero unsafe code).
                let mut r_params = RuntimeParams::default_with_ch(2);
                r_params.atten_lim_db = 24.0;
                r_params.post_filter_beta = 0.2;
                r_params.post_filter = true;

                let df_params = DfParams::default();
                let mut model = match DfTract::new(df_params, &r_params) {
                    Ok(m) => m,
                    Err(e) => {
                        log::error!("Failed to initialize DeepFilterNet model in worker: {e}");
                        return;
                    }
                };

                let mut raw_buf = vec![0.0f32; 2 * HOP_SIZE];
                let mut enh = Array2::zeros((2, HOP_SIZE));

                while worker_running.load(Ordering::Relaxed) {
                    // Update runtime parameters if modified
                    let current_atten = f32::from_bits(worker_atten_lim.load(Ordering::Relaxed));
                    let current_post = f32::from_bits(worker_post_filter.load(Ordering::Relaxed));
                    model.atten_lim = Some(current_atten);
                    model.post_filter_beta = current_post;
                    model.post_filter = current_post > 0.0;

                    // When a complete HOP_SIZE (480 samples = 10ms) is ready in both channels, process it
                    if cons_to_worker_l.occupied_len() >= HOP_SIZE && cons_to_worker_r.occupied_len() >= HOP_SIZE {
                        for i in 0..HOP_SIZE {
                            raw_buf[i] = cons_to_worker_l.try_pop().unwrap_or(0.0);
                        }
                        for i in 0..HOP_SIZE {
                            raw_buf[HOP_SIZE + i] = cons_to_worker_r.try_pop().unwrap_or(0.0);
                        }

                        if let Ok(noisy) = Array2::from_shape_vec((2, HOP_SIZE), raw_buf.clone()) {
                            match model.process(noisy.view(), enh.view_mut()) {
                                Ok(snr) => {
                                    // Map SNR in dB (approx -15 to +25 dB) to VAD confidence [0.0, 1.0]
                                    let vad = 1.0 / (1.0 + (-(snr + 5.0) / 4.0).exp());
                                    worker_last_vad.store(vad.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);

                                    for i in 0..HOP_SIZE {
                                        let _ = prod_from_worker_l.try_push(enh[[0, i]]);
                                        let _ = prod_from_worker_r.try_push(enh[[1, i]]);
                                    }
                                }
                                Err(e) => {
                                    log::warn!("DeepFilterNet inference error: {e}");
                                    for i in 0..HOP_SIZE {
                                        let _ = prod_from_worker_l.try_push(raw_buf[i]);
                                        let _ = prod_from_worker_r.try_push(raw_buf[HOP_SIZE + i]);
                                    }
                                }
                            }
                        }
                    } else {
                        // Sleep briefly (1ms) to yield CPU when waiting for more audio frames
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            });

        let worker = match worker_handle {
            Ok(h) => Some(h),
            Err(e) => {
                log::error!("Failed to spawn DeepFilterNet worker thread: {e}");
                return None;
            }
        };

        Some(Self {
            prod_to_worker_l,
            prod_to_worker_r,
            cons_from_worker_l,
            cons_from_worker_r,
            dry_l: VecDeque::with_capacity(4096),
            dry_r: VecDeque::with_capacity(4096),

            atten_lim_bits,
            post_filter_bits,
            mix: 1.0,
            output_gain: 1.0,

            last_vad_bits,
            running,
            worker,
        })
    }
}

impl Drop for DeepFilterProcessor {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl BuiltinProcessor for DeepFilterProcessor {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }

        let mix = self.mix;
        let gain = self.output_gain;

        // Push raw incoming audio into worker ring buffer & keep dry copies
        for i in 0..n {
            let _ = self.prod_to_worker_l.try_push(left[i]);
            let _ = self.prod_to_worker_r.try_push(right[i]);
            self.dry_l.push_back(left[i]);
            self.dry_r.push_back(right[i]);
        }

        // Pop available enhanced frames from worker
        for i in 0..n {
            let dry_s_l = self.dry_l.pop_front().unwrap_or(left[i]);
            let dry_s_r = self.dry_r.pop_front().unwrap_or(right[i]);

            if let (Some(clean_l), Some(clean_r)) = (self.cons_from_worker_l.try_pop(), self.cons_from_worker_r.try_pop()) {
                left[i] = (dry_s_l * (1.0 - mix) + clean_l * mix) * gain;
                right[i] = (dry_s_r * (1.0 - mix) + clean_r * mix) * gain;
            } else {
                // Latency warmup or worker catching up: pass through dry audio (zero clicks or silence)
                left[i] = dry_s_l * gain;
                right[i] = dry_s_r * gain;
            }
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        match id {
            0 => {
                // Max Attenuation in dB (0.0 to 60.0)
                let val = value.clamp(0.0, 60.0);
                self.atten_lim_bits.store(val.to_bits(), Ordering::Relaxed);
            }
            1 => {
                // Post-filter threshold beta (0.0 to 1.0)
                let val = value.clamp(0.0, 1.0);
                self.post_filter_bits.store(val.to_bits(), Ordering::Relaxed);
            }
            2 => {
                // Mix (0.0 to 1.0)
                self.mix = value.clamp(0.0, 1.0);
            }
            3 => {
                // Output Gain (-24.0 to +12.0 dB)
                let db = value.clamp(-24.0, 12.0);
                self.output_gain = 10.0_f32.powf(db / 20.0);
            }
            _ => {}
        }
    }

    fn get_vad(&self) -> f32 {
        f32::from_bits(self.last_vad_bits.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_filter_rejects_non_48k() {
        assert!(DeepFilterProcessor::new(44100.0).is_none());
        assert!(DeepFilterProcessor::new(96000.0).is_none());
    }

    #[test]
    fn deep_filter_initializes_and_processes() {
        let mut df = DeepFilterProcessor::new(48000.0).expect("DeepFilterProcessor should initialize at 48kHz");
        
        // Test parameters
        df.set_parameter(0, 30.0); // max atten
        df.set_parameter(1, 0.5);  // post filter
        df.set_parameter(2, 0.8);  // mix
        df.set_parameter(3, -6.0); // gain

        // Process a block
        let mut left = vec![0.05f32; 480];
        let mut right = vec![-0.05f32; 480];
        df.process_stereo(&mut left, &mut right);

        assert_eq!(left.len(), 480);
        assert_eq!(right.len(), 480);
        // Ensure no NaN or infinity produced
        for s in left.iter().chain(right.iter()) {
            assert!(s.is_finite(), "Output sample must be finite");
        }

        let vad = df.get_vad();
        assert!((0.0..=1.0).contains(&vad), "VAD must be in [0.0, 1.0]");
    }
}

