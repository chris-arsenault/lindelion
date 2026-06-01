//! Lock-free single-producer/single-consumer ring of `f32` samples — the capture→render hand-off.
//!
//! Capacity is rounded up to a power of two and the backing is allocated once at construction.
//! `push` (producer/capture thread) and `pop` (consumer/render thread) are allocation-free and never
//! block (ADR-0001). Monotonic `read`/`write` counters are masked to index the backing; their
//! wrapping difference is the number of buffered samples.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A lock-free SPSC ring of `f32` samples.
pub struct AudioRing {
    buffer: Box<[UnsafeCell<f32>]>,
    mask: usize,
    write: AtomicUsize,
    read: AtomicUsize,
}

// Safe: a single producer touches cells in `[read, write)` and a single consumer touches cells in
// `[write, read)`; the regions are disjoint and the atomics publish the boundaries.
unsafe impl Sync for AudioRing {}
unsafe impl Send for AudioRing {}

impl AudioRing {
    /// A ring holding at least `min_capacity` samples (rounded up to a power of two, minimum 2).
    pub fn with_capacity(min_capacity: usize) -> Self {
        let capacity = min_capacity.next_power_of_two().max(2);
        let buffer = (0..capacity)
            .map(|_| UnsafeCell::new(0.0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            buffer,
            mask: capacity - 1,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    /// Number of samples the ring can hold.
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Number of samples currently buffered.
    pub fn available(&self) -> usize {
        let write = self.write.load(Ordering::Acquire);
        let read = self.read.load(Ordering::Acquire);
        write.wrapping_sub(read)
    }

    /// Number of free sample slots.
    pub fn free(&self) -> usize {
        self.capacity() - self.available()
    }

    /// Push as many of `src`'s samples as fit; returns the count written. Producer side only.
    pub fn push(&self, src: &[f32]) -> usize {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let free = self.capacity() - write.wrapping_sub(read);
        let n = src.len().min(free);
        for (i, &sample) in src.iter().take(n).enumerate() {
            let index = write.wrapping_add(i) & self.mask;
            unsafe {
                *self.buffer[index].get() = sample;
            }
        }
        self.write.store(write.wrapping_add(n), Ordering::Release);
        n
    }

    /// Pop up to `dst.len()` samples; returns the count read. Consumer side only.
    pub fn pop(&self, dst: &mut [f32]) -> usize {
        let read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        let available = write.wrapping_sub(read);
        let n = dst.len().min(available);
        for (i, slot) in dst.iter_mut().take(n).enumerate() {
            let index = read.wrapping_add(i) & self.mask;
            *slot = unsafe { *self.buffer[index].get() };
        }
        self.read.store(read.wrapping_add(n), Ordering::Release);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_roundtrips_with_wraparound() {
        let ring = AudioRing::with_capacity(8);
        assert_eq!(ring.capacity(), 8);

        let first = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(ring.push(&first), 5);
        let mut out = [0.0; 5];
        assert_eq!(ring.pop(&mut out), 5);
        assert_eq!(out, first);

        // Counters now at 5; this push spans the mask boundary (indices 5,6,7,0,1,2).
        let second = [6.0, 7.0, 8.0, 9.0, 10.0, 11.0];
        assert_eq!(ring.push(&second), 6);
        let mut out2 = [0.0; 6];
        assert_eq!(ring.pop(&mut out2), 6);
        assert_eq!(out2, second);
    }

    #[test]
    fn push_saturates_at_capacity_and_pop_at_available() {
        let ring = AudioRing::with_capacity(4);
        let src = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(ring.push(&src), 4); // only capacity fits
        let mut out = [0.0; 8];
        assert_eq!(ring.pop(&mut out), 4); // only available comes out
        assert_eq!(&out[..4], &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn push_pop_is_allocation_free() {
        let ring = AudioRing::with_capacity(64);
        let src = [0.5f32; 32];
        let mut dst = [0.0f32; 32];
        lindelion_test_allocator::assert_no_allocations("ring push/pop", || {
            ring.push(&src);
            ring.pop(&mut dst);
        });
    }
}
