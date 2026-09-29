//! Per-plugin input/output level history for the plugin GUIs' before/after
//! waveform: one (pre, post) peak pair per ~10 ms window, written lock-free
//! by the audio thread and read by the UI.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Windows kept: 2 s of history at 10 ms per window.
pub const HISTORY: usize = 200;

pub struct Scope {
    /// Frames per window (10 ms at the plugin's sample rate).
    window: usize,
    // Audio-thread accumulators for the window in progress.
    acc_frames: AtomicUsize,
    acc_pre: AtomicU32,
    acc_post: AtomicU32,
    // Ring of finished windows (f32 bits).
    pre: [AtomicU32; HISTORY],
    post: [AtomicU32; HISTORY],
    /// Total windows ever written; the ring index is `written % HISTORY`.
    written: AtomicUsize,
    /// Pre-processing copy of the block (audio thread only; pre-allocated).
    scratch: parking_lot::Mutex<(Vec<f32>, Vec<f32>)>,
}

impl Scope {
    pub fn new(window: usize) -> Self {
        Self {
            window: window.max(1),
            acc_frames: AtomicUsize::new(0),
            acc_pre: AtomicU32::new(0),
            acc_post: AtomicU32::new(0),
            pre: std::array::from_fn(|_| AtomicU32::new(0)),
            post: std::array::from_fn(|_| AtomicU32::new(0)),
            written: AtomicUsize::new(0),
            scratch: parking_lot::Mutex::new((Vec::with_capacity(8192), Vec::with_capacity(8192))),
        }
    }

    /// Runs `process` on the block and records its before/after levels.
    pub fn around(&self, left: &mut [f32], right: &mut [f32], process: impl FnOnce(&mut [f32], &mut [f32])) {
        let Some(mut scratch) = self.scratch.try_lock() else {
            process(left, right);
            return;
        };
        let (pre_l, pre_r) = &mut *scratch;
        pre_l.clear();
        pre_l.extend_from_slice(left);
        pre_r.clear();
        pre_r.extend_from_slice(right);
        process(left, right);
        self.record(pre_l, pre_r, left, right);
    }

    /// Audio thread only.
    pub fn record(&self, pre_l: &[f32], pre_r: &[f32], post_l: &[f32], post_r: &[f32]) {
        let n = pre_l.len().min(pre_r.len()).min(post_l.len()).min(post_r.len());
        let mut frames = self.acc_frames.load(Ordering::Relaxed);
        let mut pre = f32::from_bits(self.acc_pre.load(Ordering::Relaxed));
        let mut post = f32::from_bits(self.acc_post.load(Ordering::Relaxed));
        for i in 0..n {
            pre = pre.max(pre_l[i].abs()).max(pre_r[i].abs());
            post = post.max(post_l[i].abs()).max(post_r[i].abs());
            frames += 1;
            if frames == self.window {
                let w = self.written.load(Ordering::Relaxed);
                self.pre[w % HISTORY].store(pre.to_bits(), Ordering::Relaxed);
                self.post[w % HISTORY].store(post.to_bits(), Ordering::Relaxed);
                self.written.store(w + 1, Ordering::Release);
                frames = 0;
                pre = 0.0;
                post = 0.0;
            }
        }
        self.acc_frames.store(frames, Ordering::Relaxed);
        self.acc_pre.store(pre.to_bits(), Ordering::Relaxed);
        self.acc_post.store(post.to_bits(), Ordering::Relaxed);
    }

    /// (pre, post) peaks, oldest first. A window being overwritten while
    /// this reads may show up torn — harmless for a display.
    pub fn snapshot(&self) -> Vec<(f32, f32)> {
        let written = self.written.load(Ordering::Acquire);
        let n = written.min(HISTORY);
        (written - n..written)
            .map(|w| {
                let i = w % HISTORY;
                (
                    f32::from_bits(self.pre[i].load(Ordering::Relaxed)),
                    f32::from_bits(self.post[i].load(Ordering::Relaxed)),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_one_peak_pair_per_window_oldest_first() {
        let scope = Scope::new(480);
        let pre = vec![0.1f32; 480];
        let post = vec![0.4f32; 480];
        for i in 0..3 {
            let scale = (i + 1) as f32;
            let pre: Vec<f32> = pre.iter().map(|s| s * scale).collect();
            let post: Vec<f32> = post.iter().map(|s| -s * scale).collect();
            scope.record(&pre, &pre, &post, &post);
        }
        let snap = scope.snapshot();
        assert_eq!(snap.len(), 3);
        assert!((snap[0].0 - 0.1).abs() < 1e-6 && (snap[0].1 - 0.4).abs() < 1e-6);
        assert!((snap[2].0 - 0.3).abs() < 1e-6 && (snap[2].1 - 1.2).abs() < 1e-6);
    }

    #[test]
    fn partial_windows_accumulate_across_blocks() {
        let scope = Scope::new(480);
        let quiet = vec![0.1f32; 240];
        let loud = vec![0.5f32; 240];
        scope.record(&quiet, &quiet, &quiet, &quiet);
        assert!(scope.snapshot().is_empty());
        scope.record(&loud, &loud, &loud, &loud);
        let snap = scope.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0], (0.5, 0.5), "window peak spans both blocks");
    }

    #[test]
    fn keeps_only_the_most_recent_history() {
        let scope = Scope::new(10);
        let block = vec![0.2f32; 10];
        for _ in 0..(HISTORY + 5) {
            scope.record(&block, &block, &block, &block);
        }
        assert_eq!(scope.snapshot().len(), HISTORY);
    }
}
