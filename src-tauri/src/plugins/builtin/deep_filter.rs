use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use ringbuf::{HeapRb, HeapProd, HeapCons, traits::{Producer, Consumer, Split, Observer}};
use df::tract::{DfParams, DfTract, RuntimeParams};
use tract_core::ndarray::Array2;

use relight_addon_api::{AddonDescriptor, AddonProcessor, ParamSpec, Scale};

const fn p(id: u32, name: &'static str, min: f32, max: f32, default: f32, unit: &'static str, scale: Scale) -> ParamSpec {
    ParamSpec { id, name, min, max, default, unit, scale, step: 0.0, choices: &[] }
}

pub const ID: &str = "builtin::deep_filter";

static ADDON_PARAMS: &[ParamSpec] = &[
    p(0, "Max Attenuation", 0.0, 60.0, 24.0, "dB", Scale::Db),
    p(1, "Post-filter Threshold", 0.0, 1.0, 0.2, "", Scale::Linear),
    p(2, "Mix", 0.0, 1.0, 1.0, "", Scale::Linear),
    p(3, "Output Gain", -24.0, 12.0, 0.0, "dB", Scale::Db),
];
static ADDON_METERS: &[&str] = &["vad"];

pub static ADDON: AddonDescriptor = AddonDescriptor {
    id: ID,
    name: "AI Noise Suppressor (DeepFilterNet)",
    category: "Noise Reduction",
    version: 1,
    params: ADDON_PARAMS,
    meters: ADDON_METERS,
    sample_rates: &[48000],
    create: |sr| DeepFilterProcessor::new(sr).map(|p| Box::new(p) as Box<dyn AddonProcessor>),
    migrate: None,
};
const _: () = assert!(relight_addon_api::check_params(ADDON_PARAMS, ADDON_METERS));
const HOP_SIZE: usize = 480; // 10ms frame at 48kHz
const RB_CAPACITY: usize = 48000; // 1 second buffer

/// Pipeline depth (dry entries) beyond the current block above which whole
/// incoming hops are dropped: normal is ~2 hops + block + inference time,
/// so this allows ~3 hops of backlog before latency is clawed back.
const MAX_PIPELINE: usize = HOP_SIZE * 5;

fn primed_dry() -> VecDeque<f32> {
    let mut q = VecDeque::with_capacity(RB_CAPACITY + HOP_SIZE);
    q.resize(HOP_SIZE, 0.0);
    q
}

pub struct DeepFilterProcessor {
    prod_to_worker: HeapProd<[f32; 2]>,
    /// Worker output, one frame per sample: `[clean_l, clean_r, dry_l, dry_r]`
    /// — the enhanced sample together with the exact dry sample the model was
    /// fed for it, so wet/dry mixing stays in phase (see `process_stereo`).
    cons_from_worker: HeapCons<[f32; 4]>,
    /// Keeps worker-output backlog (from late inference hops) from turning
    /// into permanent extra latency.
    trimmer: relight_addon_api::rt::BacklogTrimmer,

    /// Dry copy of everything in the pipeline: one entry per worker output
    /// sample still to come (primed with the hop of zeros the worker's first
    /// output is aligned to), popped in step with `cons_from_worker`. Its
    /// length is the current latency, and it supplies the aligned dry
    /// continuation played when the worker underruns.
    dry_l: VecDeque<f32>,
    dry_r: VecDeque<f32>,
    /// Underrun samples played from `dry_*` since the last worker sample.
    dry_ahead: usize,
    /// Incoming samples still to drop (whole hops) to shed backlog.
    drop_in: usize,
    /// The worker has produced its first sample (before that: latency silence).
    started: bool,

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
                let _mmcss = relight_addon_api::mmcss::boost_current_thread_to_pro_audio();

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
                        // Tells process_stereo to pass audio through instead
                        // of mixing against a wet signal that will never come.
                        worker_running.store(false, Ordering::Relaxed);
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
                // The STFT overlap-add makes `enh` reconstruct the *previous*
                // hop, so the dry signal for the mix is the previous `noisy`.
                let mut prev_noisy: Array2<f32> = Array2::zeros((2, HOP_SIZE));

                // `atten_lim` is a *linear* factor inside DfTract — go through the
                // setters (they convert dB), and only on change (they log).
                let (mut last_atten, mut last_post) = (f32::NAN, f32::NAN);
                while worker_running.load(Ordering::Relaxed) {
                    let current_atten = f32::from_bits(worker_atten_lim.load(Ordering::Relaxed));
                    let current_post = f32::from_bits(worker_post_filter.load(Ordering::Relaxed));
                    if current_atten != last_atten {
                        model.set_atten_lim(current_atten);
                        last_atten = current_atten;
                    }
                    if current_post != last_post {
                        model.set_pf_beta(current_post);
                        last_post = current_post;
                    }

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
                                    let _ = prod_from_worker.try_push([enh[[0, i]], enh[[1, i]], prev_noisy[[0, i]], prev_noisy[[1, i]]]);
                                }
                                prev_noisy.assign(&noisy);
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
            // Headroom: half a hop — inference time varies from hop to hop.
            trimmer: relight_addon_api::rt::BacklogTrimmer::new(48_000, HOP_SIZE / 2),
            dry_l: primed_dry(),
            dry_r: primed_dry(),
            dry_ahead: 0,
            drop_in: 0,
            started: false,

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

impl AddonProcessor for DeepFilterProcessor {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], meters: &mut [f32]) {
        meters[0] = self.get_vad();
        let n = left.len().min(right.len());
        if n == 0 || !self.running.load(Ordering::Relaxed) {
            return; // empty block, or the model failed to load: pass through
        }

        let mix = self.mix;
        let gain = self.output_gain;

        // Inference slower than real time piles input up in the pipeline
        // (each underrun sample adds one sample of latency). Past
        // MAX_PIPELINE, drop whole incoming hops so latency comes back down
        // instead of ratcheting toward the 1 s ring size.
        for i in 0..n {
            if self.drop_in == 0 && self.dry_l.len() > n + MAX_PIPELINE {
                self.drop_in = HOP_SIZE;
            }
            if self.drop_in > 0 {
                self.drop_in -= 1;
                continue;
            }
            // `dry_*` never outgrows its preallocation (no RT allocation),
            // and only samples the worker accepted are mirrored.
            if self.dry_l.len() < self.dry_l.capacity() && self.prod_to_worker.try_push([left[i], right[i]]).is_ok() {
                self.dry_l.push_back(left[i]);
                self.dry_r.push_back(right[i]);
            }
        }

        // Every worker sample carries the exact dry sample the model was fed
        // for it, so wet/dry mixing stays in phase at any `mix` (mixing
        // against dry from a different point in time comb-filters).
        //
        // Underrun (the worker is late): play the dry continuation at the
        // current latency, not silence. The late hop still plays when it
        // arrives (latency grows by the gap), bounded by MAX_PIPELINE above.
        let trimmed = self.trimmer.before_pop(&mut self.cons_from_worker, n).min(self.dry_l.len());
        self.dry_l.drain(..trimmed);
        self.dry_r.drain(..trimmed);
        self.dry_ahead = self.dry_ahead.saturating_sub(trimmed);
        for i in 0..n {
            match self.cons_from_worker.try_pop() {
                Some([wl, wr, dl, dr]) => {
                    self.started = true;
                    self.dry_ahead = 0;
                    self.dry_l.pop_front();
                    self.dry_r.pop_front();
                    left[i] = (dl * (1.0 - mix) + wl * mix) * gain;
                    right[i] = (dr * (1.0 - mix) + wr * mix) * gain;
                }
                None if self.started => {
                    // Past the end of the pipeline (worker stalled longer
                    // than it holds): the live input, still not silence.
                    let (dl, dr) = match (self.dry_l.get(self.dry_ahead), self.dry_r.get(self.dry_ahead)) {
                        (Some(&l), Some(&r)) => {
                            self.dry_ahead += 1;
                            (l, r)
                        }
                        _ => (left[i], right[i]),
                    };
                    left[i] = dl * gain;
                    right[i] = dr * gain;
                }
                None => {
                    // Warmup (model still loading / first hop in flight).
                    left[i] = 0.0;
                    right[i] = 0.0;
                }
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

    /// One hop of input buffering plus the STFT window overlap of the
    /// low-latency model (no conv/DF lookahead) ≈ 20 ms at 48 kHz. Inference
    /// hiccups can add more; BacklogTrimmer takes that back out.
    fn latency_samples(&self) -> u32 { (HOP_SIZE * 2) as u32 }
}

impl DeepFilterProcessor {
    fn get_vad(&self) -> f32 {
        f32::from_bits(self.last_vad_bits.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn golden_deep_filter() {
        use super::super::golden_tests::*;
        for (suffix, params) in [("default", &[][..]), ("custom", CUSTOM_DEEP)] {
            let mut df = DeepFilterProcessor::new(48000.0).unwrap();
            for &(id, v) in params {
                df.set_parameter(id, v);
            }
            // Wait for the worker after each block so timing can't change the output.
            let (mut l, mut r) = golden_input();
            for (cl, cr) in l.chunks_mut(480).zip(r.chunks_mut(480)) {
                df.process(cl, cr, &mut [0.0]);
                let t = std::time::Instant::now();
                while df.cons_from_worker.occupied_len() < 480 {
                    assert!(t.elapsed() < Duration::from_secs(5), "worker stalled");
                    std::thread::sleep(Duration::from_micros(200));
                }
            }
            let out: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
            check_or_write(&format!("deep_filter_{suffix}"), &out);
        }
    }

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
        df.process(&mut left, &mut right, &mut [0.0]);

        assert_eq!(left.len(), 480);
        assert_eq!(right.len(), 480);
        // Ensure no NaN or infinity produced
        for s in left.iter().chain(right.iter()) {
            assert!(s.is_finite(), "Output sample must be finite");
        }

        let vad = df.get_vad();
        assert!((0.0..=1.0).contains(&vad), "VAD must be in [0.0, 1.0]");
    }

    /// The RT side wired to rings the test drives by hand, in place of the
    /// model thread, so the worker's speed is deterministic.
    fn rt_only() -> (DeepFilterProcessor, HeapCons<[f32; 2]>, HeapProd<[f32; 4]>) {
        let (prod_to_worker, to_worker) = HeapRb::<[f32; 2]>::new(RB_CAPACITY).split();
        let (from_worker, cons_from_worker) = HeapRb::<[f32; 4]>::new(RB_CAPACITY).split();
        let bits = |v: f32| Arc::new(AtomicU32::new(v.to_bits()));
        let df = DeepFilterProcessor {
            prod_to_worker,
            cons_from_worker,
            trimmer: relight_addon_api::rt::BacklogTrimmer::new(48_000, HOP_SIZE / 2),
            dry_l: primed_dry(),
            dry_r: primed_dry(),
            dry_ahead: 0,
            drop_in: 0,
            started: false,
            atten_lim_bits: bits(24.0),
            post_filter_bits: bits(0.2),
            mix: 1.0,
            output_gain: 1.0,
            last_vad_bits: bits(0.0),
            running: Arc::new(AtomicBool::new(true)),
            worker: None,
        };
        (df, to_worker, from_worker)
    }

    /// Inference at ~60% of real time: underruns must play dry (never
    /// silence at mix=1) and the pipeline must stay bounded, not ratchet.
    #[test]
    fn a_worker_slower_than_real_time_neither_gaps_nor_piles_up_latency() {
        let (mut df, mut to_worker, mut from_worker) = rt_only();
        let block = 256;
        for k in 0..2000 {
            let input: Vec<f32> = (0..block).map(|i| 0.5 + 0.1 * (((k * block + i) as f32) * 0.01).sin()).collect();
            let (mut l, mut r) = (input.clone(), input);
            df.process(&mut l, &mut r, &mut [0.0]);
            if df.started {
                assert!(l.iter().all(|&s| s > 0.3), "block {k}: gap in the output");
            }
            assert!(df.dry_l.len() <= 2 * block + MAX_PIPELINE, "block {k}: latency piled up to {}", df.dry_l.len());
            // Fake worker: one identity hop every third block (480 per 768 samples).
            if k % 3 == 0 && to_worker.occupied_len() >= HOP_SIZE {
                for _ in 0..HOP_SIZE {
                    let [a, b] = to_worker.try_pop().unwrap();
                    from_worker.try_push([a, b, a, b]).unwrap();
                }
            }
        }
        assert!(df.started);
    }
}

