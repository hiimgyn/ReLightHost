use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use ringbuf::{HeapRb, HeapProd, HeapCons, traits::{Producer, Consumer, Split, Observer}};
use df::tract::{DfParams, DfTract, RuntimeParams};
use tract_core::ndarray::{Array2, ArrayView2, ArrayViewMut2};

use relight_addon_api::{AddonDescriptor, AddonProcessor, ParamSpec, Scale};

const fn p(id: u32, name: &'static str, min: f32, max: f32, default: f32, unit: &'static str, scale: Scale) -> ParamSpec {
    ParamSpec { id, name, min, max, default, unit, scale, step: 0.0, choices: &[] }
}

pub const ID: &str = "builtin::deep_filter";

const POST_FILTER_DEFAULT: f32 = 0.2;

static ADDON_PARAMS: &[ParamSpec] = &[
    p(0, "Max Attenuation", 0.0, 60.0, 24.0, "dB", Scale::Db),
    p(1, "Post-filter Threshold", 0.0, 1.0, POST_FILTER_DEFAULT, "", Scale::Linear),
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
/// Dry→wet crossfade when the worker output resumes after an underrun (2 ms).
const XFADE: usize = 96;

const fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Shortest latency at block size `n` that never underruns with an instant
/// worker: the model's hop, one hop of input buffering and the block, less
/// what the block grid shares with the hop grid.
const fn pipeline_latency(n: usize) -> usize {
    2 * HOP_SIZE + n - gcd(n, HOP_SIZE)
}

fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// Sets `running` to false when the worker thread ends, by return or panic,
/// so `process` falls back to passing audio through.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// The model as the worker drives it (libDF, or a fake in tests).
trait HopModel {
    /// One hop; the local SNR in dB, or None on an inference error.
    fn process_hop(&mut self, noisy: ArrayView2<f32>, enh: ArrayViewMut2<f32>) -> Option<f32>;
    /// Forgets all audio seen so far.
    fn restart(&mut self);
}

impl HopModel for DfTract {
    fn process_hop(&mut self, noisy: ArrayView2<f32>, enh: ArrayViewMut2<f32>) -> Option<f32> {
        self.process(noisy, enh).map_err(|e| log::warn!("DeepFilterNet inference error: {e}")).ok()
    }

    fn restart(&mut self) {
        // `init` alone keeps the STFT overlap and normalisation memory
        // (`df_states`); with them cleared it builds fresh ones.
        self.df_states.clear();
        if let Err(e) = self.init() {
            log::warn!("DeepFilterNet reset failed: {e}");
        }
    }
}

/// Pushes one hop of `[wet_l, wet_r, dry_l, dry_r]` frames. The STFT
/// overlap-add makes the model's output reconstruct the *previous* hop, so
/// `prev` is its dry; without a model output (inference error) `prev` is the
/// wet too, which keeps the stream aligned. Then `prev` becomes `noisy`.
fn emit_hop(prod: &mut HeapProd<[f32; 4]>, enh: Option<&Array2<f32>>, noisy: &Array2<f32>, prev: &mut Array2<f32>) {
    let wet = enh.unwrap_or(prev);
    for i in 0..HOP_SIZE {
        let _ = prod.try_push([wet[[0, i]], wet[[1, i]], prev[[0, i]], prev[[1, i]]]);
    }
    prev.assign(noisy);
}

/// Everything the worker thread owns apart from the model.
struct Worker {
    input: HeapCons<[f32; 2]>,
    output: HeapProd<[f32; 4]>,
    /// Set by `reset` (and from creation until the model has loaded): drop
    /// the queued input and restart the model, then clear it.
    flush: Arc<AtomicBool>,
    vad_bits: Arc<AtomicU32>,
    noisy: Array2<f32>,
    enh: Array2<f32>,
    prev: Array2<f32>,
}

impl Worker {
    /// A pending flush, else one hop if a full one is queued. False = idle.
    fn step(&mut self, model: &mut impl HopModel) -> bool {
        if self.flush.load(Ordering::Acquire) {
            // The audio side pushes nothing until the flag clears, so this
            // empties the input for good.
            self.input.clear();
            self.prev.fill(0.0);
            model.restart();
            // Release: a hop pushed before this is visible to the audio
            // thread once it sees the flag clear, so its last drain gets it.
            self.flush.store(false, Ordering::Release);
            return true;
        }
        if self.input.occupied_len() < HOP_SIZE {
            return false;
        }
        for i in 0..HOP_SIZE {
            let [l, r] = self.input.try_pop().unwrap_or_default();
            self.noisy[[0, i]] = l;
            self.noisy[[1, i]] = r;
        }
        let snr = model.process_hop(self.noisy.view(), self.enh.view_mut());
        if let Some(snr) = snr {
            // Map SNR in dB (approx -15 to +25 dB) to VAD confidence [0.0, 1.0]
            let vad = 1.0 / (1.0 + (-(snr + 5.0) / 4.0).exp());
            self.vad_bits.store(vad.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
        emit_hop(&mut self.output, snr.map(|_| &self.enh), &self.noisy, &mut self.prev);
        true
    }
}

pub struct DeepFilterProcessor {
    prod_to_worker: HeapProd<[f32; 2]>,
    /// Worker output, one frame per sample: `[clean_l, clean_r, dry_l, dry_r]`
    /// — the enhanced sample together with the exact dry sample the model was
    /// fed for it, so wet/dry mixing stays in phase (see `process`).
    cons_from_worker: HeapCons<[f32; 4]>,
    /// Keeps worker-output backlog (from late inference hops) from turning
    /// into permanent extra latency.
    trimmer: relight_addon_api::rt::BacklogTrimmer,

    /// Dry copy of everything in the pipeline: one entry per worker output
    /// sample still to come (primed with the hop of zeros the worker's first
    /// output is aligned to), popped in step with `cons_from_worker`. Its
    /// length less `dry_ahead` is the latency, and it supplies the aligned
    /// dry continuation played when the worker underruns.
    dry_l: VecDeque<f32>,
    dry_r: VecDeque<f32>,
    /// Underrun samples played from `dry_*`, skipped from the worker output
    /// when it resumes so nothing repeats and the latency doesn't grow.
    dry_ahead: usize,
    /// Start-up silence still to play before the first worker sample is due.
    warmup: usize,
    /// Wet samples left in the dry→wet crossfade after an underrun.
    xfade: usize,
    /// Incoming samples still to drop (whole hops) to shed backlog.
    drop_in: usize,
    /// Shared with the worker: see `Worker::flush`.
    flush: Arc<AtomicBool>,
    /// Waiting for the worker to finish a flush: push nothing, play silence.
    flushing: bool,
    latency: u32,

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

        let (mut this, mut w) = Self::unspawned();
        let running = Arc::clone(&this.running);
        let atten_lim = Arc::clone(&this.atten_lim_bits);
        let post_filter = Arc::clone(&this.post_filter_bits);

        // Spawn dedicated worker thread for DeepFilterNet inference
        let worker_handle = std::thread::Builder::new()
            .name("relight-deepfilter-worker".to_string())
            .spawn(move || {
                let _stop = StopOnDrop(Arc::clone(&running));
                // Boost thread priority on Windows
                #[cfg(target_os = "windows")]
                let _mmcss = relight_addon_api::mmcss::boost_current_thread_to_pro_audio();

                // Initialize DfTract entirely inside the worker thread (DfTract is !Send, so owning
                // it solely on this thread satisfies Rust safety guarantees with zero unsafe code).
                let mut r_params = RuntimeParams::default_with_ch(2);
                r_params.atten_lim_db = 24.0;
                r_params.post_filter_beta = POST_FILTER_DEFAULT;
                r_params.post_filter = true;

                let mut model = match DfTract::new(DfParams::default(), &r_params) {
                    Ok(m) => m,
                    Err(e) => {
                        // `_stop` makes process() pass audio through.
                        log::error!("Failed to initialize DeepFilterNet model in worker: {e}");
                        return;
                    }
                };

                // The hop buffers are reused, but libDF allocates per hop —
                // fine on this thread, never on the audio thread.
                // `atten_lim` is a *linear* factor inside DfTract — go through the
                // setters (they convert dB), and only on change (they log).
                let (mut last_atten, mut last_post) = (f32::NAN, f32::NAN);
                while running.load(Ordering::Relaxed) {
                    let current_atten = f32::from_bits(atten_lim.load(Ordering::Relaxed));
                    let current_post = f32::from_bits(post_filter.load(Ordering::Relaxed));
                    if current_atten != last_atten {
                        // Below 0.01 dB libDF switches to a zero-delay bypass,
                        // a hop out of step with every other setting.
                        model.set_atten_lim(current_atten.max(0.1));
                        last_atten = current_atten;
                    }
                    if current_post != last_post {
                        model.set_pf_beta(current_post);
                        last_post = current_post;
                    }
                    if !w.step(&mut model) {
                        // Sleep briefly (1ms) to yield CPU when waiting for more audio frames
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            });

        match worker_handle {
            Ok(h) => this.worker = Some(h),
            Err(e) => {
                log::error!("Failed to spawn DeepFilterNet worker thread: {e}");
                return None;
            }
        }
        Some(this)
    }

    /// The audio side and the worker state wired to it, no thread yet. It
    /// starts out flushing: nothing is queued until the model has loaded.
    fn unspawned() -> (Self, Worker) {
        // Lock-free SPSC rings between the audio thread and the worker.
        let (prod_to_worker, input) = HeapRb::<[f32; 2]>::new(RB_CAPACITY).split();
        let (output, cons_from_worker) = HeapRb::<[f32; 4]>::new(RB_CAPACITY).split();
        let bits = |v: f32| Arc::new(AtomicU32::new(v.to_bits()));
        let flush = Arc::new(AtomicBool::new(true));
        let last_vad_bits = bits(0.0);
        let worker = Worker {
            input,
            output,
            flush: Arc::clone(&flush),
            vad_bits: Arc::clone(&last_vad_bits),
            noisy: Array2::zeros((2, HOP_SIZE)),
            enh: Array2::zeros((2, HOP_SIZE)),
            prev: Array2::zeros((2, HOP_SIZE)),
        };
        let this = Self {
            prod_to_worker,
            cons_from_worker,
            // Headroom: half a hop — inference time varies from hop to hop.
            trimmer: relight_addon_api::rt::BacklogTrimmer::new(48_000, HOP_SIZE / 2),
            dry_l: VecDeque::with_capacity(RB_CAPACITY + HOP_SIZE),
            dry_r: VecDeque::with_capacity(RB_CAPACITY + HOP_SIZE),
            dry_ahead: 0,
            warmup: 0,
            xfade: 0,
            drop_in: 0,
            flush,
            flushing: true,
            latency: pipeline_latency(HOP_SIZE) as u32,

            atten_lim_bits: bits(24.0),
            post_filter_bits: bits(POST_FILTER_DEFAULT),
            mix: 1.0,
            output_gain: 1.0,

            last_vad_bits,
            running: Arc::new(AtomicBool::new(true)),
            worker: None,
        };
        (this, worker)
    }

    /// Fresh pipeline for block size `n`, right after a flush (the worker's
    /// input is empty). `dry_*` gets the hop of zeros the worker's first
    /// output is aligned to, `e` zeros go to both, and `warmup` covers the
    /// blocks before the first hop can be back: the latency is
    /// `pipeline_latency(n)` from the first block. The first worker sample
    /// is due on a fixed block, so a late one is an ordinary underrun.
    fn prime(&mut self, n: usize) {
        let warmup = HOP_SIZE.div_ceil(n) * n;
        let e = pipeline_latency(n) - HOP_SIZE - warmup;
        for q in [&mut self.dry_l, &mut self.dry_r] {
            q.clear();
            q.resize(HOP_SIZE + e, 0.0);
        }
        for _ in 0..e {
            let _ = self.prod_to_worker.try_push([0.0; 2]);
        }
        self.dry_ahead = 0;
        self.warmup = warmup;
        self.xfade = 0;
        self.drop_in = 0;
        self.trimmer = relight_addon_api::rt::BacklogTrimmer::new(48_000, HOP_SIZE / 2);
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
        if !self.running.load(Ordering::Relaxed) {
            self.latency = 0;
            return; // the model failed to load or the worker died: pass through
        }
        if n == 0 {
            return;
        }

        if self.flushing {
            // Load, then drain: once the flag reads clear, every hop the
            // worker pushed before clearing it is visible and goes here.
            let busy = self.flush.load(Ordering::Acquire);
            self.cons_from_worker.clear();
            if busy {
                left.fill(0.0);
                right.fill(0.0);
                self.latency = pipeline_latency(n) as u32;
                return;
            }
            self.flushing = false;
            self.prime(n);
        }

        let mix = self.mix;
        let gain = self.output_gain;

        // Inference slower than real time piles input up in the pipeline
        // (each underrun sample adds one dry entry). Past the normal
        // pipeline + this block + 3 hops, drop whole incoming hops so it
        // comes back down instead of ratcheting toward the 1 s ring size.
        let max_pipeline = n + 2 * HOP_SIZE + n + 3 * HOP_SIZE;
        for i in 0..n {
            // Sanitised in place: the live-input fallback below plays it.
            left[i] = finite_or_zero(left[i]);
            right[i] = finite_or_zero(right[i]);
            if self.drop_in == 0 && self.dry_l.len() > max_pipeline {
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
        // current latency, not silence. When the worker catches up, the
        // samples already played dry are skipped and the wet fades back in.
        let trimmed = self.trimmer.before_pop(&mut self.cons_from_worker, n).min(self.dry_l.len());
        self.dry_l.drain(..trimmed);
        self.dry_r.drain(..trimmed);
        self.dry_ahead = self.dry_ahead.saturating_sub(trimmed);
        for i in 0..n {
            if self.warmup > 0 {
                self.warmup -= 1;
                left[i] = 0.0;
                right[i] = 0.0;
                continue;
            }
            if self.dry_ahead > 0 {
                // Every worker sample has a dry entry, so `k` never outruns `dry_*`.
                let k = self.cons_from_worker.skip(self.dry_ahead);
                self.dry_l.drain(..k);
                self.dry_r.drain(..k);
                self.dry_ahead -= k;
                self.xfade = XFADE;
            }
            let frame = if self.dry_ahead == 0 { self.cons_from_worker.try_pop() } else { None };
            match frame {
                Some([wl, wr, dl, dr]) => {
                    self.dry_l.pop_front();
                    self.dry_r.pop_front();
                    let m = mix * (1.0 - self.xfade as f32 / XFADE as f32);
                    self.xfade = self.xfade.saturating_sub(1);
                    left[i] = (dl + m * (wl - dl)) * gain;
                    right[i] = (dr + m * (wr - dr)) * gain;
                }
                None => {
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
            }
        }
        self.latency = (self.dry_l.len() - self.dry_ahead + self.warmup) as u32;
        meters[0] = self.get_vad(); // after the block, so the meter isn't one block stale
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

    /// Drops everything queued (both rings, the dry copy, the model's
    /// memory) so un-bypassing doesn't replay stale audio. Doesn't wait for
    /// the worker: `process` plays silence until the worker has flushed.
    fn reset(&mut self) {
        self.flush.store(true, Ordering::Release);
        self.flushing = true;
        self.last_vad_bits.store(0.0f32.to_bits(), Ordering::Relaxed);
    }

    /// The pipeline length as of the last block: `pipeline_latency` of the
    /// block size; underruns don't add to it.
    fn latency_samples(&self) -> u32 { self.latency }
}

impl DeepFilterProcessor {
    fn get_vad(&self) -> f32 {
        f32::from_bits(self.last_vad_bits.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod golden_helpers;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn golden_deep_filter() {
        use crate::golden_helpers::*;
        for (suffix, params) in [("default", &[][..]), ("custom", CUSTOM_DEEP)] {
            let mut df = DeepFilterProcessor::new(48000.0).unwrap();
            for &(id, v) in params {
                df.set_parameter(id, v);
            }
            // Wait for the worker after each block so timing can't change the output.
            wait_for_model(&df);
            let (mut l, mut r) = golden_input();
            for (cl, cr) in l.chunks_mut(480).zip(r.chunks_mut(480)) {
                df.process(cl, cr, &mut [0.0]);
                settle(&df);
            }
            let out: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
            check_or_write(&format!("deep_filter_{suffix}"), &out);
        }
    }

    /// Blocks until the worker has loaded the model (its start-up flush done).
    fn wait_for_model(df: &DeepFilterProcessor) {
        let t = std::time::Instant::now();
        while df.flush.load(Ordering::Acquire) {
            assert!(t.elapsed() < Duration::from_secs(30), "model never loaded");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Blocks until the worker has turned every full hop queued so far into output.
    fn settle(df: &DeepFilterProcessor) {
        let t = std::time::Instant::now();
        loop {
            // Input first: a hop in flight has left the input but not reached the output yet.
            let queued = df.prod_to_worker.occupied_len();
            let done = df.cons_from_worker.occupied_len();
            if queued < HOP_SIZE && done == df.dry_l.len() - HOP_SIZE - queued {
                return;
            }
            assert!(t.elapsed() < Duration::from_secs(5), "worker stalled");
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// Max Attenuation 0 dB must not switch libDF to its zero-delay bypass:
    /// the output stays one model hop behind, where `latency_samples` says.
    #[test]
    #[ignore]
    fn zero_attenuation_keeps_hop_alignment() {
        use crate::golden_helpers::lcg;
        let n = 256;
        let mut df = DeepFilterProcessor::new(48000.0).unwrap();
        df.set_parameter(0, 0.0);
        df.set_parameter(2, 1.0);
        wait_for_model(&df);
        let input: Vec<f32> = lcg(7, 96_000).iter().map(|v| 0.1 * v).collect();
        let mut out = Vec::with_capacity(input.len());
        let t0 = std::time::Instant::now();
        for (k, chunk) in input.chunks(n).enumerate() {
            let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
            df.process(&mut l, &mut r, &mut [0.0]);
            out.extend(l);
            // Real-time pacing: block k+1 is due (k+1)·n samples after the start.
            let due = Duration::from_secs_f64(((k + 1) * n) as f64 / 48_000.0);
            if let Some(wait) = due.checked_sub(t0.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        let lat = df.latency_samples() as usize;
        // Cross-correlate the last 0.25 s of output against the input.
        let (from, to) = (out.len() - 12_000, out.len());
        let xcorr = |lag: usize| (from..to).map(|t| out[t] * input[t - lag]).sum::<f32>();
        let best = (0..4000).max_by(|&a, &b| xcorr(a).total_cmp(&xcorr(b))).unwrap();
        eprintln!("zero attenuation: xcorr lag {best}, latency_samples {lat}");
        assert!(best.abs_diff(lat) <= 4, "xcorr lag {best} vs latency_samples {lat}");
    }

    /// After a reset the real model holds no audio from before it (libDF's
    /// rolling spectra and STFT overlap included): silence in, silence out.
    #[test]
    #[ignore]
    fn real_model_reset_leaves_nothing_behind() {
        use crate::golden_helpers::lcg;
        let mut df = DeepFilterProcessor::new(48000.0).unwrap();
        wait_for_model(&df);
        for chunk in lcg(3, 48_000).chunks(480) {
            let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
            df.process(&mut l, &mut r, &mut [0.0]);
            settle(&df);
        }
        df.process(&mut [0.3; 480], &mut [0.3; 480], &mut [0.0]); // still queued at the reset
        df.reset();
        let mut peak = 0.0f32;
        for _ in 0..200 {
            let (mut l, mut r) = ([0.0f32; 480], [0.0f32; 480]);
            df.process(&mut l, &mut r, &mut [0.0]);
            peak = l.iter().chain(&r).fold(peak, |m, v| m.max(v.abs()));
            if df.flushing {
                std::thread::sleep(Duration::from_millis(2));
            } else {
                settle(&df);
            }
        }
        assert!(!df.flushing, "the worker never finished the flush");
        assert_eq!(peak, 0.0, "stale audio after reset");
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

    /// Fake model: its "enhanced" output is the previous hop, exactly what
    /// libDF's STFT delay gives, so the pipeline output is bit-exact.
    /// `mute` makes it output silence instead (a wet unlike the dry).
    struct PrevHop {
        prev: Array2<f32>,
        mute: bool,
    }

    impl HopModel for PrevHop {
        fn process_hop(&mut self, noisy: ArrayView2<f32>, mut enh: ArrayViewMut2<f32>) -> Option<f32> {
            enh.assign(&self.prev);
            if self.mute {
                enh.fill(0.0);
            }
            self.prev.assign(&noisy);
            Some(0.0)
        }
        fn restart(&mut self) {
            self.prev.fill(0.0);
        }
    }

    fn gcd(a: usize, b: usize) -> usize { if b == 0 { a } else { gcd(b, a % b) } }

    /// The shortest latency a block size allows with an instant worker: the
    /// hop the model delays by, plus a hop and a block, less what the block
    /// grid shares with the hop grid.
    fn want_latency(n: usize) -> usize { 960 + n - gcd(n, 480) }

    /// The audio side driven against the real worker loop with a `PrevHop`
    /// model, run by hand between blocks (an instant worker).
    struct Harness {
        df: DeepFilterProcessor,
        w: Worker,
        model: PrevHop,
    }

    impl Harness {
        fn new() -> Self {
            let (df, w) = DeepFilterProcessor::unspawned();
            let mut h = Self { df, w, model: PrevHop { prev: Array2::zeros((2, HOP_SIZE)), mute: false } };
            h.worker(); // the start-up flush: "model loaded"
            h
        }
        /// Runs the worker (flush included) until it is idle.
        fn worker(&mut self) {
            while self.w.step(&mut self.model) {}
        }
        /// Block `k` of a ramp (1, 2, 3, ... on L, negated on R), then the
        /// worker unless it is stalled. Returns L's output.
        fn ramp_block(&mut self, n: usize, k: usize, stalled: bool) -> Vec<f32> {
            let mut l: Vec<f32> = (0..n).map(|i| (k * n + i + 1) as f32).collect();
            let mut r: Vec<f32> = l.iter().map(|v| -v).collect();
            self.df.process(&mut l, &mut r, &mut [0.0]);
            if !stalled {
                self.worker();
            }
            l
        }
        fn ramp(&mut self, n: usize, blocks: usize) -> Vec<f32> {
            (0..blocks).flat_map(|k| self.ramp_block(n, k, false)).collect()
        }
    }

    /// The ramp delayed by exactly `want_latency(n)`: silence, then 1, 2, 3, ...
    fn ideal(n: usize, len: usize) -> Vec<f32> {
        let lat = want_latency(n);
        (0..len).map(|t| if t < lat { 0.0 } else { (t - lat + 1) as f32 }).collect()
    }

    fn assert_same(got: &[f32], want: &[f32], what: &str) {
        if let Some(t) = (0..got.len()).find(|&t| got[t] != want[t]) {
            let lo = t.saturating_sub(3);
            panic!("{what}: first mismatch at {t}: got {:?}, want {:?}", &got[lo..(t + 4).min(got.len())], &want[lo..(t + 4).min(want.len())]);
        }
    }

    #[test]
    fn large_blocks_do_not_drop_hops() {
        for n in [256, 441, 2048, 4096] {
            let mut h = Harness::new();
            let out = h.ramp(n, 150_000 / n);
            let s = out.iter().position(|&v| v != 0.0).expect("no output");
            assert_eq!(out[s], 1.0, "n={n}: the first input samples were dropped");
            let want: Vec<f32> = (1..=out.len() - s).map(|v| v as f32).collect();
            assert_same(&out[s..], &want, &format!("n={n}"));
        }
    }

    #[test]
    fn startup_never_underruns() {
        for n in [128, 256, 512, 1024] {
            let mut h = Harness::new();
            let mut out = Vec::new();
            for k in 0..48_000 / n {
                out.extend(h.ramp_block(n, k, false));
                assert_eq!(h.df.dry_ahead, 0, "n={n}: underrun in block {k}");
            }
            assert_same(&out, &ideal(n, out.len()), &format!("n={n}"));
        }
    }

    #[test]
    fn underrun_resume_neither_repeats_nor_adds_latency() {
        let n = 256;
        let mut h = Harness::new();
        let mut out = Vec::new();
        for k in 0..200 {
            out.extend(h.ramp_block(n, k, (100..103).contains(&k)));
            if k == 102 {
                assert!(h.df.dry_ahead > 0, "the stall never underran");
            }
            assert_eq!(h.df.latency_samples() as usize, want_latency(n), "block {k}");
        }
        assert_same(&out, &ideal(n, out.len()), "stall 3 blocks");
    }

    #[test]
    fn latency_reports_pipeline_length() {
        for n in [128, 256, 441, 480, 512, 1024, 2048] {
            let mut h = Harness::new();
            let at = 20_000;
            let mut out = Vec::new();
            for k in 0..60_000 / n {
                let mut l: Vec<f32> = (k * n..(k + 1) * n).map(|t| if t == at { 1.0 } else { 0.0 }).collect();
                let mut r = l.clone();
                h.df.process(&mut l, &mut r, &mut [0.0]);
                out.extend(l);
                h.worker();
            }
            let peak = out.iter().position(|&v| v == 1.0).expect("impulse lost");
            assert_eq!(peak - at, want_latency(n), "n={n}: impulse delay");
            assert_eq!(h.df.latency_samples() as usize, want_latency(n), "n={n}: reported");
        }
    }

    #[test]
    fn reset_drains_pipeline() {
        let n = 256;
        let mut h = Harness::new();
        h.ramp(n, 40);
        // Stale input still queued for the worker, then the host bypasses and resets.
        let mut l = vec![5.0f32; n];
        let mut r = l.clone();
        h.df.process(&mut l, &mut r, &mut [0.0]);
        h.df.reset();
        // Audio resumes before the worker got to the flush.
        let before = h.w.input.occupied_len();
        for _ in 0..2 {
            let (mut l, mut r) = (vec![0.5f32; n], vec![0.5f32; n]);
            h.df.process(&mut l, &mut r, &mut [0.0]);
            assert!(l.iter().chain(&r).all(|&v| v == 0.0), "output while flushing: {:?}", &l[..4]);
        }
        assert_eq!(h.w.input.occupied_len(), before, "pushed to the worker while flushing");
        // A hop the worker finished just before it saw the flush.
        for _ in 0..HOP_SIZE {
            h.w.output.try_push([7.0; 4]).unwrap();
        }
        h.worker();
        let out = h.ramp(n, 48_000 / n);
        assert_same(&out, &ideal(n, out.len()), "after reset");
    }

    #[test]
    fn non_finite_input_never_reaches_worker() {
        let mut h = Harness::new();
        let mut l = vec![0.1f32; 480];
        let mut r = vec![0.1f32; 480];
        for (i, v) in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY].into_iter().enumerate() {
            l[i * 7] = v;
            r[i * 11] = v;
        }
        for _ in 0..4 {
            let (mut a, mut b) = (l.clone(), r.clone());
            h.df.process(&mut a, &mut b, &mut [0.0]);
            assert!(a.iter().chain(&b).all(|v| v.is_finite()), "non-finite output");
        }
        assert!(h.df.dry_l.iter().chain(&h.df.dry_r).all(|v| v.is_finite()), "non-finite dry");
        assert!(h.w.input.occupied_len() > 0);
        while let Some([a, b]) = h.w.input.try_pop() {
            assert!(a.is_finite() && b.is_finite(), "non-finite sample reached the worker");
        }
    }

    /// Resuming after an underrun fades from the dry continuation to the
    /// wet over 2 ms instead of switching in one sample.
    #[test]
    fn underrun_resume_crossfades_to_wet() {
        let n = 256;
        let mut h = Harness::new();
        h.model.mute = true; // wet = silence, dry = 1.0
        let block = |h: &mut Harness| {
            let (mut l, mut r) = (vec![1.0f32; n], vec![1.0f32; n]);
            h.df.process(&mut l, &mut r, &mut [0.0]);
            l
        };
        for _ in 0..20 {
            block(&mut h);
            h.worker();
        }
        for _ in 0..3 {
            block(&mut h); // worker stalled
        }
        assert!(h.df.dry_ahead > 0, "the stall never underran");
        h.worker();
        let out = block(&mut h);
        let fade = &out[..XFADE + 1];
        assert_eq!(fade[0], 1.0, "starts from the dry level");
        assert!(fade.windows(2).all(|w| w[1] < w[0]), "fades monotonically: {fade:?}");
        assert!(out[XFADE..].iter().all(|&v| v == 0.0), "fully wet after 2 ms");
    }

    #[test]
    fn inference_error_emits_previous_hop() {
        let (mut prod, mut cons) = HeapRb::<[f32; 4]>::new(2 * HOP_SIZE).split();
        let hop = |v: f32| Array2::from_shape_fn((2, HOP_SIZE), |(c, i)| v + c as f32 * 1000.0 + i as f32);
        let (noisy, mut prev) = (hop(5000.0), hop(0.0));
        emit_hop(&mut prod, None, &noisy, &mut prev);
        assert_eq!(cons.occupied_len(), HOP_SIZE);
        for i in 0..HOP_SIZE {
            let (l, r) = (i as f32, 1000.0 + i as f32);
            assert_eq!(cons.try_pop(), Some([l, r, l, r]), "sample {i}: wet and dry are the previous hop");
        }
        assert_eq!(prev, noisy, "the failed hop is the next one's dry");
    }

    #[test]
    fn worker_panic_falls_back_to_dry() {
        let mut h = Harness::new();
        let running = Arc::clone(&h.df.running);
        let _ = std::thread::spawn(move || {
            let _stop = StopOnDrop(running);
            panic!("simulated worker panic");
        })
        .join();
        let input: Vec<f32> = (0..256).map(|i| i as f32 * 0.001).collect();
        let (mut l, mut r) = (input.clone(), input.clone());
        h.df.process(&mut l, &mut r, &mut [0.0]);
        assert_eq!(l, input, "passes audio through");
        assert_eq!(h.df.latency_samples(), 0);
    }

    /// Inference at ~60% of real time: underruns must play dry (never
    /// silence at mix=1) and the pipeline must stay bounded, not ratchet.
    #[test]
    fn a_worker_slower_than_real_time_neither_gaps_nor_piles_up_latency() {
        let mut h = Harness::new();
        let block = 256;
        let cap = 2 * block + 5 * HOP_SIZE;
        for k in 0..2000 {
            let input: Vec<f32> = (0..block).map(|i| 0.5 + 0.1 * (((k * block + i) as f32) * 0.01).sin()).collect();
            let (mut l, mut r) = (input.clone(), input);
            h.df.process(&mut l, &mut r, &mut [0.0]);
            // Past the start-up silence (one flushing block plus the pipeline).
            if k * block > 2 * want_latency(block) {
                assert!(l.iter().all(|&s| s > 0.3), "block {k}: gap in the output");
            }
            assert!(h.df.dry_l.len() <= cap + 1, "block {k}: latency piled up to {}", h.df.dry_l.len());
            // One hop every third block (480 per 768 samples).
            if k % 3 == 0 {
                h.w.step(&mut h.model);
            }
        }
    }
}
