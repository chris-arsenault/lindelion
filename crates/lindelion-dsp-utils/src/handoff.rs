//! Lock-free realtime hand-off primitives shared by the off-thread analysis workers
//! (`lindelion-speech-signals`'s `AnalysisWorker`, Lúmedir's `DeliveryWorker`) and by atomic meter
//! snapshots (Cenedril). Before this module each of those rolled its own byte-identical copy.
//!
//! The discipline throughout: an `f32` is published through an `AtomicU32` holding its bit pattern,
//! stored/loaded at `Relaxed` ordering; index handoff uses `Release`/`Acquire`. Audio-thread
//! operations (`AtomicF32::store`, `SampleRing::push`) are allocation-free and non-blocking
//! (ADR-0001); only construction allocates, and must happen off the audio thread. Multi-field
//! snapshots may read cosmetically torn across fields, which is acceptable for control-rate signals
//! and meters.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// An `f32` stored in an `AtomicU32` via its bit pattern, with lock-free `Relaxed` `store`/`load`.
/// This is the per-field cell every snapshot hand-off in the workspace is built from. Default is
/// `0.0`.
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    /// Create a cell holding `value`.
    pub fn new(value: f32) -> Self {
        Self(AtomicU32::new(value.to_bits()))
    }

    /// Publish `value`. Allocation-free and non-blocking.
    pub fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Read the latest published value. Allocation-free.
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Single-producer/single-consumer ring of `f32` samples (each stored as bits in an [`AtomicF32`]).
/// The audio thread `push`es one sample at a time; an off-thread worker `drain_into`s in chunks.
/// Lossy under overflow — if the producer laps a stalled consumer the oldest unread samples are
/// overwritten — which is acceptable for control-rate analysis. Contains no `unsafe`.
pub struct SampleRing {
    buffer: Box<[AtomicF32]>,
    mask: usize,
    write: AtomicUsize,
    read: AtomicUsize,
}

impl SampleRing {
    /// Allocate a ring with `capacity_pow2` slots (must be a power of two). Allocates; construct
    /// off the audio thread.
    pub fn new(capacity_pow2: usize) -> Self {
        assert!(
            capacity_pow2.is_power_of_two(),
            "ring capacity must be a power of two"
        );
        let buffer = (0..capacity_pow2)
            .map(|_| AtomicF32::default())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            mask: capacity_pow2 - 1,
            buffer,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    /// Producer (audio thread): push one sample. Allocation-free and non-blocking.
    pub fn push(&self, sample: f32) {
        let w = self.write.load(Ordering::Relaxed);
        self.buffer[w & self.mask].store(sample);
        self.write.store(w.wrapping_add(1), Ordering::Release);
    }

    /// Consumer (worker): append up to `max` samples written since the last drain into `out`,
    /// oldest first. Returns the number drained. Allocation-free aside from `out`'s own growth.
    pub fn drain_into(&self, out: &mut Vec<f32>, max: usize) -> usize {
        let r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        let available = w.wrapping_sub(r).min(max);
        for i in 0..available {
            out.push(self.buffer[r.wrapping_add(i) & self.mask].load());
        }
        self.read
            .store(r.wrapping_add(available), Ordering::Release);
        available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_f32_round_trips() {
        let cell = AtomicF32::default();
        assert_eq!(cell.load(), 0.0);
        cell.store(-14.25);
        assert_eq!(cell.load(), -14.25);
        assert_eq!(AtomicF32::new(3.5).load(), 3.5);
    }

    #[test]
    fn atomic_f32_store_is_allocation_free() {
        let cell = AtomicF32::new(0.0);
        lindelion_test_allocator::assert_no_allocations("atomic f32 store/load", || {
            cell.store(1.0);
            let _ = cell.load();
        });
    }

    #[test]
    fn ring_drains_in_order() {
        let ring = SampleRing::new(8);
        for s in [1.0, 2.0, 3.0] {
            ring.push(s);
        }
        let mut out = Vec::new();
        assert_eq!(ring.drain_into(&mut out, 64), 3);
        assert_eq!(out, vec![1.0, 2.0, 3.0]);
        // A second drain with nothing new returns zero.
        out.clear();
        assert_eq!(ring.drain_into(&mut out, 64), 0);
    }

    #[test]
    fn ring_respects_drain_max() {
        let ring = SampleRing::new(8);
        for s in [1.0, 2.0, 3.0, 4.0] {
            ring.push(s);
        }
        let mut out = Vec::new();
        assert_eq!(ring.drain_into(&mut out, 2), 2);
        assert_eq!(out, vec![1.0, 2.0]);
        assert_eq!(ring.drain_into(&mut out, 64), 2);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn ring_push_is_allocation_free() {
        let ring = SampleRing::new(1 << 12);
        let block = [0.1_f32; 512];
        lindelion_test_allocator::assert_no_allocations("ring push", || {
            for &s in &block {
                ring.push(s);
            }
        });
    }
}
