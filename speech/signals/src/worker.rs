//! Analysis worker: runs [`SignalAnalyzer`](crate::analyzer::SignalAnalyzer) off-thread and hands
//! the latest snapshot back to the audio thread through a lock-free handoff.
//!
//! Audio thread → worker: a shared [`SampleRing`] (`push`). Worker → audio thread: per-signal
//! [`AtomicF32`] cells (`latest`). Both audio-thread operations are allocation-free and non-blocking
//! (ADR-0001); all heavy work and allocation happen on the worker thread. The lock-free primitives
//! live in `lindelion-dsp-utils::handoff`, shared with the other analysis workers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lindelion_dsp_utils::handoff::{AtomicF32, SampleRing};

use crate::analyzer::{SignalAnalyzer, SignalSnapshot};

const RING_CAPACITY: usize = 1 << 16; // power of two; ~1.3 s at 48 kHz
const DRAIN_MAX: usize = 4_096;
const IDLE_SLEEP: Duration = Duration::from_millis(2);

/// Count of [`AnalysisWorker`]s constructed in this process. Always-on instrumentation (one relaxed
/// increment per construction) so tests can assert the chain builds exactly one analyzer.
static ANALYSIS_WORKER_CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);

/// Number of [`AnalysisWorker`]s constructed so far in this process.
pub fn analysis_worker_constructions() -> usize {
    ANALYSIS_WORKER_CONSTRUCTIONS.load(Ordering::Relaxed)
}

struct Shared {
    ring: SampleRing,
    pitch_hz: AtomicF32,
    pitch_confidence: AtomicF32,
    voicing_score: AtomicF32,
    voicing_state: AtomicF32,
    onset_flux_high: AtomicF32,
    spectral_flux: AtomicF32,
    hnr_db: AtomicF32,
    stop: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: SampleRing::new(RING_CAPACITY),
            pitch_hz: AtomicF32::default(),
            pitch_confidence: AtomicF32::default(),
            voicing_score: AtomicF32::default(),
            voicing_state: AtomicF32::default(),
            onset_flux_high: AtomicF32::default(),
            spectral_flux: AtomicF32::default(),
            hnr_db: AtomicF32::default(),
            stop: AtomicBool::new(false),
        }
    }

    fn publish(&self, snapshot: &SignalSnapshot) {
        self.pitch_hz.store(snapshot.pitch_hz);
        self.pitch_confidence.store(snapshot.pitch_confidence);
        self.voicing_score.store(snapshot.voicing_score);
        self.voicing_state.store(snapshot.voicing_state);
        self.onset_flux_high.store(snapshot.onset_flux_high);
        self.spectral_flux.store(snapshot.spectral_flux);
        self.hnr_db.store(snapshot.hnr_db);
    }

    fn snapshot(&self) -> SignalSnapshot {
        SignalSnapshot {
            pitch_hz: self.pitch_hz.load(),
            pitch_confidence: self.pitch_confidence.load(),
            voicing_score: self.voicing_score.load(),
            voicing_state: self.voicing_state.load(),
            onset_flux_high: self.onset_flux_high.load(),
            spectral_flux: self.spectral_flux.load(),
            hnr_db: self.hnr_db.load(),
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
        ANALYSIS_WORKER_CONSTRUCTIONS.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        let handle = thread::spawn(move || worker_loop(worker_shared, source_sample_rate));
        Self {
            shared,
            handle: Some(handle),
        }
    }

    /// Hand a block of input audio to the worker. Allocation-free and non-blocking.
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
    fn constructing_a_worker_increments_the_counter() {
        // The global counter only ever increments, so a single construction strictly raises it
        // even when other tests build workers concurrently. (The exact "one analyzer per chain"
        // assertion lives in caloma's chain test, which is the sole worker-builder in its binary.)
        let before = analysis_worker_constructions();
        let _worker = AnalysisWorker::new(48_000);
        assert!(
            analysis_worker_constructions() > before,
            "AnalysisWorker::new must increment the construction counter"
        );
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
