//! Off-thread delivery worker: runs [`SignalAnalyzer`] + [`DeliveryAggregator`] on a background
//! thread and hands the latest [`DeliverySnapshot`] back through a lock-free handoff.
//!
//! The audio thread pushes samples into a shared [`SampleRing`] (`push`, allocation-free &
//! non-blocking, ADR-0001); the worker thread drains the ring in small chunks (≈ one analysis frame
//! each), runs the analyzer and the aggregator, and publishes both the underlying `SignalSnapshot`
//! and the assembled `DeliverySnapshot` through per-field [`AtomicF32`] cells (`latest_snapshot` /
//! `latest_delivery`). The lock-free ring and atomic cells are shared with
//! `lindelion_speech_signals::AnalysisWorker` via `lindelion-dsp-utils::handoff`; only the
//! richer-than-`SignalSnapshot` published payload and its dual publish are Lúmedir-specific.

// In the `test-sync-analysis` build the off-thread machinery (worker loop, ring, idle sleep) is
// unused; suppress the resulting dead-code/unused-import warnings in that build only.
#![cfg_attr(feature = "test-sync-analysis", allow(dead_code, unused_imports))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lindelion_dsp_utils::handoff::{AtomicF32, SampleRing};
use lindelion_speech_signals::{SignalAnalyzer, SignalSnapshot};

use crate::delivery::{DeliveryAggregator, DeliveryConfig, DeliverySnapshot};

const RING_CAPACITY: usize = 1 << 16; // power of two; ~1.3 s at 48 kHz
const DRAIN_MAX: usize = 256; // ≈ one analysis frame per drain → near-frame-rate snapshot sampling
const IDLE_SLEEP: Duration = Duration::from_millis(2);

struct Shared {
    ring: SampleRing,
    // Delivery snapshot.
    d_syllables_per_second: AtomicF32,
    d_words_per_minute: AtomicF32,
    d_pitch_dynamism: AtomicF32,
    d_pause_fraction: AtomicF32,
    d_pause_count: AtomicU32,
    d_clarity: AtomicF32,
    // Underlying signal snapshot (continuity with M0).
    s_pitch_hz: AtomicF32,
    s_pitch_confidence: AtomicF32,
    s_voicing_score: AtomicF32,
    s_voicing_state: AtomicF32,
    s_onset_flux_high: AtomicF32,
    s_spectral_flux: AtomicF32,
    s_hnr_db: AtomicF32,
    stop: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: SampleRing::new(RING_CAPACITY),
            d_syllables_per_second: AtomicF32::default(),
            d_words_per_minute: AtomicF32::default(),
            d_pitch_dynamism: AtomicF32::default(),
            d_pause_fraction: AtomicF32::default(),
            d_pause_count: AtomicU32::new(0),
            d_clarity: AtomicF32::default(),
            s_pitch_hz: AtomicF32::default(),
            s_pitch_confidence: AtomicF32::default(),
            s_voicing_score: AtomicF32::default(),
            s_voicing_state: AtomicF32::default(),
            s_onset_flux_high: AtomicF32::default(),
            s_spectral_flux: AtomicF32::default(),
            s_hnr_db: AtomicF32::default(),
            stop: AtomicBool::new(false),
        }
    }

    fn publish_delivery(&self, d: &DeliverySnapshot) {
        self.d_syllables_per_second.store(d.syllables_per_second);
        self.d_words_per_minute.store(d.words_per_minute);
        self.d_pitch_dynamism.store(d.pitch_dynamism_semitones);
        self.d_pause_fraction.store(d.pause_fraction);
        self.d_pause_count.store(d.pause_count, Ordering::Relaxed);
        self.d_clarity.store(d.clarity);
    }

    fn publish_signal(&self, s: &SignalSnapshot) {
        self.s_pitch_hz.store(s.pitch_hz);
        self.s_pitch_confidence.store(s.pitch_confidence);
        self.s_voicing_score.store(s.voicing_score);
        self.s_voicing_state.store(s.voicing_state);
        self.s_onset_flux_high.store(s.onset_flux_high);
        self.s_spectral_flux.store(s.spectral_flux);
        self.s_hnr_db.store(s.hnr_db);
    }

    fn delivery_snapshot(&self) -> DeliverySnapshot {
        DeliverySnapshot {
            syllables_per_second: self.d_syllables_per_second.load(),
            words_per_minute: self.d_words_per_minute.load(),
            pitch_dynamism_semitones: self.d_pitch_dynamism.load(),
            pause_fraction: self.d_pause_fraction.load(),
            pause_count: self.d_pause_count.load(Ordering::Relaxed),
            clarity: self.d_clarity.load(),
        }
    }

    fn signal_snapshot(&self) -> SignalSnapshot {
        SignalSnapshot {
            pitch_hz: self.s_pitch_hz.load(),
            pitch_confidence: self.s_pitch_confidence.load(),
            voicing_score: self.s_voicing_score.load(),
            voicing_state: self.s_voicing_state.load(),
            onset_flux_high: self.s_onset_flux_high.load(),
            spectral_flux: self.s_spectral_flux.load(),
            hnr_db: self.s_hnr_db.load(),
        }
    }
}

fn worker_loop(shared: Arc<Shared>, sample_rate: f32, config: DeliveryConfig) {
    let mut analyzer = SignalAnalyzer::new(sample_rate as u32);
    let mut aggregator = DeliveryAggregator::new(sample_rate, config);
    let mut local = Vec::with_capacity(DRAIN_MAX);
    while !shared.stop.load(Ordering::Acquire) {
        local.clear();
        if shared.ring.drain_into(&mut local, DRAIN_MAX) > 0 {
            let snapshot = analyzer.process(&local);
            aggregator.update(&local, &snapshot);
            shared.publish_signal(&snapshot);
            shared.publish_delivery(&aggregator.snapshot());
        } else {
            thread::sleep(IDLE_SLEEP);
        }
    }
}

/// Runs the delivery analysis on a background thread; the audio thread pushes input and reads the
/// latest delivery snapshot, both allocation-free.
///
/// With the `test-sync-analysis` feature, the analysis runs synchronously on `push` instead of on a
/// background thread, so `latest_delivery()` immediately reflects the pushed audio (deterministic
/// offline tests).
pub struct DeliveryWorker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
    #[cfg(feature = "test-sync-analysis")]
    inline: std::sync::Mutex<(SignalAnalyzer, DeliveryAggregator)>,
}

impl DeliveryWorker {
    /// Spawn the worker for audio at `sample_rate`.
    pub fn new(sample_rate: f32, config: DeliveryConfig) -> Self {
        let shared = Arc::new(Shared::new());
        #[cfg(not(feature = "test-sync-analysis"))]
        {
            let worker_shared = Arc::clone(&shared);
            let handle = thread::spawn(move || worker_loop(worker_shared, sample_rate, config));
            Self {
                shared,
                handle: Some(handle),
            }
        }
        #[cfg(feature = "test-sync-analysis")]
        {
            Self {
                shared,
                handle: None,
                inline: std::sync::Mutex::new((
                    SignalAnalyzer::new(sample_rate as u32),
                    DeliveryAggregator::new(sample_rate, config),
                )),
            }
        }
    }

    /// Hand a block of input audio to the worker. Allocation-free and non-blocking in the default
    /// (off-thread) build. With `test-sync-analysis`, runs the analysis inline and publishes before
    /// returning.
    pub fn push(&self, block: &[f32]) {
        #[cfg(not(feature = "test-sync-analysis"))]
        for &sample in block {
            self.shared.ring.push(sample);
        }
        #[cfg(feature = "test-sync-analysis")]
        {
            let mut guard = self.inline.lock().expect("delivery inline mutex");
            let (analyzer, aggregator) = &mut *guard;
            let snapshot = analyzer.process(block);
            aggregator.update(block, &snapshot);
            self.shared.publish_signal(&snapshot);
            self.shared.publish_delivery(&aggregator.snapshot());
        }
    }

    /// Read the latest delivery snapshot. Allocation-free.
    pub fn latest_delivery(&self) -> DeliverySnapshot {
        self.shared.delivery_snapshot()
    }

    /// Read the latest underlying signal snapshot. Allocation-free.
    pub fn latest_snapshot(&self) -> SignalSnapshot {
        self.shared.signal_snapshot()
    }

    /// A cloneable, `Send + Sync` read handle onto the worker's published snapshots, for the editor
    /// (UI thread) to poll. It shares the same lock-free atomic cells the worker stores into, so it
    /// never touches the audio thread or the `RefCell<Lumedir>` — mirroring Cenedril's `frame_ring`
    /// handle to its audio→editor ring.
    pub fn reader(&self) -> DeliveryReader {
        DeliveryReader {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// A lock-free read handle onto a [`DeliveryWorker`]'s latest published snapshots. Holds a clone of
/// the worker's `Arc<Shared>`, so reads are atomic loads — safe from the editor thread, allocation-
/// free, and unaffected by the worker's lifetime.
#[derive(Clone)]
pub struct DeliveryReader {
    shared: Arc<Shared>,
}

impl DeliveryReader {
    /// The latest delivery snapshot (default before the worker has published anything).
    pub fn latest_delivery(&self) -> DeliverySnapshot {
        self.shared.delivery_snapshot()
    }

    /// The latest underlying signal snapshot.
    pub fn latest_snapshot(&self) -> SignalSnapshot {
        self.shared.signal_snapshot()
    }
}

impl Drop for DeliveryWorker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(all(test, not(feature = "test-sync-analysis")))]
mod tests {
    use super::*;

    #[test]
    fn reader_starts_at_the_default_delivery_snapshot() {
        let worker = DeliveryWorker::new(48_000.0, DeliveryConfig::default());
        let reader = worker.reader();
        assert_eq!(reader.latest_delivery(), DeliverySnapshot::default());
        assert_eq!(reader.latest_snapshot(), SignalSnapshot::default());
    }

    #[test]
    fn push_and_latest_are_allocation_free() {
        let worker = DeliveryWorker::new(48_000.0, DeliveryConfig::default());
        let block = [0.1_f32; 512];
        crate::assert_no_allocations("delivery worker push + latest", || {
            worker.push(&block);
            let _ = worker.latest_delivery();
            let _ = worker.latest_snapshot();
        });
    }
}
