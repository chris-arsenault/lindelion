//! Analysis worker: runs [`SignalAnalyzer`](crate::analyzer::SignalAnalyzer) off-thread and hands
//! the latest snapshot back to the audio thread through a lock-free handoff.
//!
//! Audio thread → worker: a single-producer/single-consumer ring of `AtomicU32` sample bits
//! (`push`). Worker → audio thread: per-signal `AtomicU32`s holding f32 bits (`latest`). Both
//! audio-thread operations are allocation-free and non-blocking (ADR-0001); all heavy work and
//! allocation happen on the worker thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::analyzer::{SignalAnalyzer, SignalSnapshot};

const RING_CAPACITY: usize = 1 << 16; // power of two; ~1.3 s at 48 kHz
const DRAIN_MAX: usize = 4_096;
const IDLE_SLEEP: Duration = Duration::from_millis(2);

// Single-producer/single-consumer ring of f32 bits. Safe via atomics (no `unsafe`). Lossy under
// overflow, which is acceptable for control-rate analysis.
struct SampleRing {
    buffer: Box<[AtomicU32]>,
    mask: usize,
    write: AtomicUsize,
    read: AtomicUsize,
}

impl SampleRing {
    fn new(capacity_pow2: usize) -> Self {
        let buffer = (0..capacity_pow2)
            .map(|_| AtomicU32::new(0))
            .collect::<Vec<_>>();
        Self {
            mask: capacity_pow2 - 1,
            buffer: buffer.into_boxed_slice(),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    fn push(&self, sample: f32) {
        let w = self.write.load(Ordering::Relaxed);
        self.buffer[w & self.mask].store(sample.to_bits(), Ordering::Relaxed);
        self.write.store(w.wrapping_add(1), Ordering::Release);
    }

    fn drain_into(&self, out: &mut Vec<f32>, max: usize) -> usize {
        let r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        let available = w.wrapping_sub(r).min(max);
        for i in 0..available {
            let bits = self.buffer[r.wrapping_add(i) & self.mask].load(Ordering::Relaxed);
            out.push(f32::from_bits(bits));
        }
        self.read
            .store(r.wrapping_add(available), Ordering::Release);
        available
    }
}

struct Shared {
    ring: SampleRing,
    pitch_hz: AtomicU32,
    pitch_confidence: AtomicU32,
    voicing_score: AtomicU32,
    voicing_state: AtomicU32,
    onset_flux_high: AtomicU32,
    spectral_flux: AtomicU32,
    hnr_db: AtomicU32,
    stop: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: SampleRing::new(RING_CAPACITY),
            pitch_hz: AtomicU32::new(0),
            pitch_confidence: AtomicU32::new(0),
            voicing_score: AtomicU32::new(0),
            voicing_state: AtomicU32::new(0),
            onset_flux_high: AtomicU32::new(0),
            spectral_flux: AtomicU32::new(0),
            hnr_db: AtomicU32::new(0),
            stop: AtomicBool::new(false),
        }
    }

    fn publish(&self, snapshot: &SignalSnapshot) {
        self.pitch_hz
            .store(snapshot.pitch_hz.to_bits(), Ordering::Relaxed);
        self.pitch_confidence
            .store(snapshot.pitch_confidence.to_bits(), Ordering::Relaxed);
        self.voicing_score
            .store(snapshot.voicing_score.to_bits(), Ordering::Relaxed);
        self.voicing_state
            .store(snapshot.voicing_state.to_bits(), Ordering::Relaxed);
        self.onset_flux_high
            .store(snapshot.onset_flux_high.to_bits(), Ordering::Relaxed);
        self.spectral_flux
            .store(snapshot.spectral_flux.to_bits(), Ordering::Relaxed);
        self.hnr_db
            .store(snapshot.hnr_db.to_bits(), Ordering::Relaxed);
    }

    fn snapshot(&self) -> SignalSnapshot {
        SignalSnapshot {
            pitch_hz: f32::from_bits(self.pitch_hz.load(Ordering::Relaxed)),
            pitch_confidence: f32::from_bits(self.pitch_confidence.load(Ordering::Relaxed)),
            voicing_score: f32::from_bits(self.voicing_score.load(Ordering::Relaxed)),
            voicing_state: f32::from_bits(self.voicing_state.load(Ordering::Relaxed)),
            onset_flux_high: f32::from_bits(self.onset_flux_high.load(Ordering::Relaxed)),
            spectral_flux: f32::from_bits(self.spectral_flux.load(Ordering::Relaxed)),
            hnr_db: f32::from_bits(self.hnr_db.load(Ordering::Relaxed)),
        }
    }
}

fn worker_loop(shared: Arc<Shared>, source_sample_rate: u32) {
    let mut analyzer = SignalAnalyzer::new(source_sample_rate);
    let mut local = Vec::with_capacity(DRAIN_MAX);
    while !shared.stop.load(Ordering::Acquire) {
        local.clear();
        if shared.ring.drain_into(&mut local, DRAIN_MAX) > 0 {
            let snapshot = analyzer.process(&local);
            shared.publish(&snapshot);
        } else {
            thread::sleep(IDLE_SLEEP);
        }
    }
}

/// Runs the heavy signal analysis on a background thread; the audio thread pushes input and reads
/// the latest snapshot, both allocation-free.
pub struct AnalysisWorker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl AnalysisWorker {
    /// Spawn the worker for audio at `source_sample_rate`.
    pub fn new(source_sample_rate: u32) -> Self {
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        let handle = thread::spawn(move || worker_loop(worker_shared, source_sample_rate));
        Self {
            shared,
            handle: Some(handle),
        }
    }

    /// Hand a block of input audio to the worker. Allocation-free; non-blocking.
    pub fn push(&self, block: &[f32]) {
        for &sample in block {
            self.shared.ring.push(sample);
        }
    }

    /// Read the latest analysis snapshot. Allocation-free.
    pub fn latest(&self) -> SignalSnapshot {
        self.shared.snapshot()
    }
}

impl Drop for AnalysisWorker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voiced_tone(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / 48_000.0;
                0.5 * (150.0 * t).sin() + 0.25 * (300.0 * t).sin() + 0.12 * (450.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn push_and_latest_are_allocation_free() {
        let worker = AnalysisWorker::new(48_000);
        let block = [0.1_f32; 512];
        lindelion_test_allocator::assert_no_allocations("worker push + latest", || {
            worker.push(&block);
            let _ = worker.latest();
        });
    }

    #[test]
    fn voiced_input_eventually_reads_voiced() {
        let worker = AnalysisWorker::new(48_000);
        worker.push(&voiced_tone(16_384));
        // Generous, scheduling-tolerant deadline (the worker runs SwiftF0 off-thread).
        let mut voiced = false;
        for _ in 0..200 {
            if worker.latest().voicing_state == 2.0 {
                voiced = true;
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(voiced, "worker never reported voiced within deadline");
    }
}
