//! Lock-free control→audio hand-off — publish a prepared graph from the control thread, consume it
//! on the audio thread, with **reclamation guaranteed off the audio thread** (ADR-0001).
//!
//! Two `AtomicPtr` slots: `pending` (control→audio) and `retired` (audio→control). The audio side
//! only swaps raw pointers and parks the old graph in `retired`; it never drops/frees. The control
//! side publishes (reclaiming the retired graph and any un-taken prior pending first) and reclaims.
//! Invariant: control reclaims before each publish and audio retires only after taking a pending, so
//! at most one retired graph is outstanding — the audio side's `retire` never overwrites a live slot.

use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// A single-producer (control) → single-consumer (audio) prepared-graph hand-off.
pub struct Handoff<T> {
    pending: AtomicPtr<T>,
    retired: AtomicPtr<T>,
}

impl<T> Handoff<T> {
    /// An empty hand-off.
    pub fn new() -> Self {
        Self {
            pending: AtomicPtr::new(ptr::null_mut()),
            retired: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Control thread: publish a new graph (reclaims the retired graph and any un-taken prior pending).
    pub fn publish(&self, value: Box<T>) {
        self.reclaim();
        let prev = self.pending.swap(Box::into_raw(value), Ordering::AcqRel);
        if !prev.is_null() {
            drop(unsafe { Box::from_raw(prev) });
        }
    }

    /// Control thread: drop the retired graph, if any.
    pub fn reclaim(&self) {
        let retired = self.retired.swap(ptr::null_mut(), Ordering::Acquire);
        if !retired.is_null() {
            drop(unsafe { Box::from_raw(retired) });
        }
    }

    /// Control thread: take ownership of the retired graph (without dropping it), if any. Used after
    /// the audio thread has stopped to recover the final graph (e.g. to capture its state).
    pub fn take_retired(&self) -> Option<Box<T>> {
        let retired = self.retired.swap(ptr::null_mut(), Ordering::Acquire);
        if retired.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(retired) })
        }
    }

    /// Audio thread: take a newly published graph, if any. The caller owns the raw pointer as its
    /// `current` graph. Allocation-free, lock-free.
    pub fn try_take(&self) -> Option<*mut T> {
        let pending = self.pending.swap(ptr::null_mut(), Ordering::Acquire);
        if pending.is_null() {
            None
        } else {
            Some(pending)
        }
    }

    /// Audio thread: park the old `current` graph for the control thread to reclaim. **Never frees.**
    pub fn retire(&self, old: *mut T) {
        if old.is_null() {
            return;
        }
        let prev = self.retired.swap(old, Ordering::Release);
        // Invariant (see module docs): `prev` is null. If control fell behind we leak the older graph
        // rather than free on the audio thread.
        debug_assert!(
            prev.is_null(),
            "retired slot overwritten — control thread fell behind"
        );
    }
}

impl<T> Default for Handoff<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for Handoff<T> {
    fn drop(&mut self) {
        let pending = *self.pending.get_mut();
        if !pending.is_null() {
            drop(unsafe { Box::from_raw(pending) });
        }
        let retired = *self.retired.get_mut();
        if !retired.is_null() {
            drop(unsafe { Box::from_raw(retired) });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering as O};

    use super::*;

    /// Increments a shared counter when dropped, so we can see *where* reclamation happens.
    struct Counter(Arc<AtomicUsize>);

    impl Drop for Counter {
        fn drop(&mut self) {
            self.0.fetch_add(1, O::SeqCst);
        }
    }

    #[test]
    fn publish_take_retire_reclaim_sequence() {
        let drops = Arc::new(AtomicUsize::new(0));
        let handoff: Handoff<Counter> = Handoff::new();

        handoff.publish(Box::new(Counter(drops.clone()))); // graph A
        let a = handoff.try_take().expect("A taken");

        handoff.publish(Box::new(Counter(drops.clone()))); // graph B
        let b = handoff.try_take().expect("B taken");
        handoff.retire(a); // audio parks A
        assert_eq!(
            drops.load(O::SeqCst),
            0,
            "retire must not drop on the audio thread"
        );

        handoff.reclaim(); // control drops A
        assert_eq!(
            drops.load(O::SeqCst),
            1,
            "reclaim drops the retired graph on the control thread"
        );

        handoff.retire(b);
        handoff.reclaim();
        assert_eq!(drops.load(O::SeqCst), 2);
    }

    #[test]
    fn publish_while_pending_reclaims_untaken() {
        let drops = Arc::new(AtomicUsize::new(0));
        let handoff: Handoff<Counter> = Handoff::new();

        handoff.publish(Box::new(Counter(drops.clone()))); // A, never taken
        handoff.publish(Box::new(Counter(drops.clone()))); // B; A reclaimed on the control thread
        assert_eq!(drops.load(O::SeqCst), 1);

        let b = handoff.try_take().expect("B taken");
        handoff.retire(b);
        handoff.reclaim();
        assert_eq!(drops.load(O::SeqCst), 2);
    }

    #[test]
    fn audio_ops_are_allocation_free() {
        let drops = Arc::new(AtomicUsize::new(0));
        let handoff: Handoff<Counter> = Handoff::new();
        handoff.publish(Box::new(Counter(drops.clone())));
        let mut current = handoff.try_take().expect("current");
        handoff.publish(Box::new(Counter(drops.clone())));

        lindelion_test_allocator::assert_no_allocations("handoff audio ops", || {
            if let Some(next) = handoff.try_take() {
                handoff.retire(current);
                current = next;
            }
        });
        assert_eq!(drops.load(O::SeqCst), 0, "audio ops free nothing");

        handoff.reclaim(); // control drops the first graph
        assert_eq!(drops.load(O::SeqCst), 1);
        handoff.retire(current);
        handoff.reclaim();
        assert_eq!(drops.load(O::SeqCst), 2);
    }
}
