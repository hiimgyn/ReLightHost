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
    prod_to_worker: HeapProd<[f32; 2]>,
    /// Worker output, one frame per sample: `[clean_l, clean_r, dry_l, dry_r]`
    /// — the enhanced sample together with the exact dry sample the model was
    /// fed for it, so wet/dry mixing stays in phase (see `process_stereo`).
    cons_from_worker: HeapCons<[f32; 4]>,
    /// Keeps worker-output backlog (from late inference hops) from turning
    /// into permanent extra latency.
    trimmer: crate::audio::mixer::BacklogTrimmer,

    /// Immediate (near-zero-latency) copy of the incoming signal, used only
    /// as the dry-only fallback while the worker hasn't produced anything
    /// aligned yet (startup warmup or a transient underrun).
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
        let (prod_to_worker, mut cons_to_worker) = HeapRb::<[f32; 2]>::new(RB_CAPACITY).split();
        let (mut prod_from_worker, cons_from_worker) = HeapRb::<[f32; 4]>::new(RB_CAPACITY).split();

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

                // Reused across every hop — no heap allocation in the 10ms
                // inference loop. `noisy` used to be rebuilt each hop via
                // `Array2::from_shape_vec(.., raw_buf.clone())`, cloning a
                // fresh Vec (and allocating a new Array2) 100x/sec; writing
                // straight into a persistent buffer removes that entirely.
                let mut noisy = Array2::zeros((2, HOP_SIZE));
                let mut enh = Array2::zeros((2, HOP_SIZE));

                while worker_running.load(Ordering::Relaxed) {
                    // Update runtime parameters if modified
                    let current_atten = f32::from_bits(worker_atten_lim.load(Ordering::Relaxed));
                    let current_post = f32::from_bits(worker_post_filter.load(Ordering::Relaxed));
                    model.atten_lim = Some(current_atten);
                    model.post_filter_beta = current_post;
                    model.post_filter = current_post > 0.0;

                    // When a complete HOP_SIZE (480 samples = 10ms) is ready in both channels, process it
                    if cons_to_worker.occupied_len() >= HOP_SIZE {
                        for i in 0..HOP_SIZE {
                            let [l, r] = cons_to_worker.try_pop().unwrap_or([0.0, 0.0]);
                            noisy[[0, i]] = l;
                            noisy[[1, i]] = r;
                        }

                        match model.process(noisy.view(), enh.view_mut()) {
                            Ok(snr) => {
                                // Map SNR in dB (approx -15 to +25 dB) to VAD confidence [0.0, 1.0]
                                let vad = 1.0 / (1.0 + (-(snr + 5.0) / 4.0).exp());
                                worker_last_vad.store(vad.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);

                                for i in 0..HOP_SIZE {
                                    let _ = prod_from_worker.try_push([enh[[0, i]], enh[[1, i]], noisy[[0, i]], noisy[[1, i]]]);
                                }
                            }
                            Err(e) => {
                                log::warn!("DeepFilterNet inference error: {e}");
                                for i in 0..HOP_SIZE {
                                    let (l, r) = (noisy[[0, i]], noisy[[1, i]]);
                                    let _ = prod_from_worker.try_push([l, r, l, r]);
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
            prod_to_worker,
            cons_from_worker,
            trimmer: crate::audio::mixer::BacklogTrimmer::new(48_000),
            dry_l: VecDeque::with_capacity(RB_CAPACITY),
            dry_r: VecDeque::with_capacity(RB_CAPACITY),

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
            let _ = self.prod_to_worker.try_push([left[i], right[i]]);
            self.dry_l.push_back(left[i]);
            self.dry_r.push_back(right[i]);
        }

        // Pop available enhanced frames from worker.
        //
        // The worker has real, unavoidable pipeline latency (hop batching +
        // the model's own lookahead), so its output is legitimately empty
        // for the first several blocks after start, and can briefly starve
        // again any time the inference thread falls behind real-time.
        //
        // The clean sample and its *aligned* dry counterpart (the exact
        // input sample the worker fed the model to produce it) are always
        // popped together — `cons_dry_aligned_*` is filled by the worker in
        // the same loop iteration as `cons_from_worker_*` (see the worker
        // above), so the two can never drift apart. Mixing against this
        // aligned pair (instead of "whatever dry sample is at the front of
        // the queue right now") is what keeps wet/dry blending in phase at
        // any `mix` other than 0 or 1 — mixing a wet sample against a dry
        // sample from a different point in time is audible as comb-filtering.
        //
        // When nothing aligned is ready yet (warmup/underrun), fall back to
        // the immediate (near-zero-latency) `dry_l`/`dry_r` queue. Missing
        // "clean" audio is silence, not dry, so it's treated as 0 in the mix
        // — a starved block plays back at most the dry proportion the user
        // dialed in (0 at the default mix=1.0, i.e. true silence instead of
        // a raw noise leak).
        self.trimmer.before_pop(&mut self.cons_from_worker, n);
        for i in 0..n {
            let immediate_dry_l = self.dry_l.pop_front().unwrap_or(left[i]);
            let immediate_dry_r = self.dry_r.pop_front().unwrap_or(right[i]);

            let (dry_s_l, clean_l, dry_s_r, clean_r) = match self.cons_from_worker.try_pop() {
                Some([wl, wr, dl, dr]) => (dl, wl, dr, wr),
                None => (immediate_dry_l, 0.0, immediate_dry_r, 0.0),
            };
            left[i] = (dry_s_l * (1.0 - mix) + clean_l * mix) * gain;
            right[i] = (dry_s_r * (1.0 - mix) + clean_r * mix) * gain;
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

