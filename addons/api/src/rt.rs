/// Bounds the latency a ring buffer between two legs can accumulate.
///
/// The backlog captured before the output leg starts, plus clock drift
/// between two devices, otherwise stays as permanent latency (up to the
/// ring's full capacity). Tracks the *minimum* occupancy seen over a window
/// — the latency that is truly excess, regardless of how bursty the
/// producer is — and trims the oldest frames so that minimum becomes one
/// consumer block plus `margin` (headroom for producer jitter). Excess
/// within `margin` is left alone, so slow drift trims rarely, in one step,
/// instead of shaving a few frames (a click) every window.
// ponytail: drops frames (one small click per trim) instead of resampling;
// add an adaptive resampler if drift trims turn out to be audible.
pub struct BacklogTrimmer {
    window: usize,
    margin: usize,
    elapsed: usize,
    min_seen: usize,
}

impl BacklogTrimmer {
    /// `window` and `margin` in frames — e.g. one second and a few ms.
    pub fn new(window: usize, margin: usize) -> Self {
        Self { window: window.max(1), margin, elapsed: 0, min_seen: usize::MAX }
    }

    /// Call right before popping `block` frames. Returns frames dropped.
    pub fn before_pop<T>(&mut self, cons: &mut ringbuf::HeapCons<T>, block: usize) -> usize {
        use ringbuf::traits::{Consumer, Observer};
        self.min_seen = self.min_seen.min(cons.occupied_len());
        self.elapsed += block;
        if self.elapsed < self.window {
            return 0;
        }
        let excess = self.min_seen.saturating_sub(block + self.margin);
        self.elapsed = 0;
        self.min_seen = usize::MAX;
        if excess > self.margin { cons.skip(excess) } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::{traits::{Consumer, Observer, Producer, Split}, HeapProd, HeapRb};

    type Frame = [f32; 2];

    fn fill(prod: &mut HeapProd<Frame>, n: usize, start: usize) {
        for i in start..start + n {
            prod.try_push([i as f32, -(i as f32)]).unwrap();
        }
    }

    #[test]
    fn trimmer_drops_a_steady_backlog_down_to_one_block() {
        let (mut prod, mut cons) = HeapRb::<Frame>::new(4096).split();
        let mut trimmer = BacklogTrimmer::new(1000, 0);
        fill(&mut prod, 1000, 0); // 1000-frame backlog, then producer keeps pace
        let mut next = 1000;
        for _ in 0..10 {
            trimmer.before_pop(&mut cons, 100);
            for _ in 0..100 { cons.try_pop(); }
            fill(&mut prod, 100, next);
            next += 100;
        }
        trimmer.before_pop(&mut cons, 100);
        assert_eq!(cons.occupied_len(), 100, "excess backlog should be trimmed to one block");
    }

    #[test]
    fn trimmer_works_on_any_frame_type() {
        let (mut prod, mut cons) = HeapRb::<[f32; 4]>::new(64).split();
        for i in 0..40 {
            prod.try_push([i as f32; 4]).unwrap();
        }
        let mut trimmer = BacklogTrimmer::new(8, 0);
        assert_eq!(trimmer.before_pop(&mut cons, 8), 32);
        assert_eq!(cons.occupied_len(), 8);
    }

    #[test]
    fn trimmer_keeps_a_jitter_margin_above_one_block() {
        let (mut prod, mut cons) = HeapRb::<Frame>::new(4096).split();
        let mut trimmer = BacklogTrimmer::new(1000, 50);
        fill(&mut prod, 1000, 0);
        let mut next = 1000;
        for _ in 0..10 {
            trimmer.before_pop(&mut cons, 100);
            for _ in 0..100 { cons.try_pop(); }
            fill(&mut prod, 100, next);
            next += 100;
        }
        trimmer.before_pop(&mut cons, 100);
        assert_eq!(cons.occupied_len(), 150, "trimmed to one block + margin");
    }

    #[test]
    fn trimmer_ignores_an_excess_within_the_margin() {
        let (mut prod, mut cons) = HeapRb::<Frame>::new(4096).split();
        let mut trimmer = BacklogTrimmer::new(100, 50);
        fill(&mut prod, 190, 0); // cushion 150 + 40 excess (< margin)
        assert_eq!(trimmer.before_pop(&mut cons, 100), 0);
        assert_eq!(cons.occupied_len(), 190);
    }

    #[test]
    fn trimmer_leaves_a_bursty_producer_alone() {
        // Producer delivers 480-frame bursts, consumer pulls 64: occupancy
        // swings high but regularly drops near zero — no excess latency.
        let (mut prod, mut cons) = HeapRb::<Frame>::new(4096).split();
        let mut trimmer = BacklogTrimmer::new(1000, 0);
        let (mut produced, mut consumed) = (0usize, 0usize);
        for _ in 0..200 {
            if produced < consumed + 64 {
                fill(&mut prod, 480, produced);
                produced += 480;
            }
            trimmer.before_pop(&mut cons, 64);
            for _ in 0..64 {
                assert!(cons.try_pop().is_some(), "trimmer caused an underrun");
            }
            consumed += 64;
        }
    }
}
