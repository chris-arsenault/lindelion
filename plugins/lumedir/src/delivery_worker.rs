//! Off-thread delivery worker: runs [`SignalAnalyzer`] + [`DeliveryAggregator`] on a background
//! thread and hands the latest [`DeliverySnapshot`] back through a lock-free handoff.
//!
//! Modeled on `lindelion_speech_signals::AnalysisWorker`: the audio thread pushes samples into a
//! single-producer/single-consumer ring of `AtomicU32` bits (`push`, allocation-free & non-blocking,
//! ADR-0001); the worker thread drains the ring in small chunks (≈ one analysis frame each), runs
//! the analyzer and the aggregator, and publishes both the underlying `SignalSnapshot` and the
//! assembled `DeliverySnapshot` through per-field atomics (`latest_snapshot` / `latest_delivery`).
//! The transport is reimplemented (not reused) because the published payload is richer than
//! `AnalysisWorker`'s.

// In the `test-sync-analysis` build the off-thread machinery (worker loop, ring, idle sleep) is
// unused; suppress the resulting dead-code/unused-import warnings in that build only.
#![cfg_attr(feature = "test-sync-analysis", allow(dead_code, unused_imports))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lindelion_speech_signals::{SignalAnalyzer, SignalSnapshot};

use crate::delivery::{DeliveryAggregator, DeliveryConfig, DeliverySnapshot};

const RING_CAPACITY: usize = 1 << 16; // power of two; ~1.3 s at 48 kHz
const DRAIN_MAX: usize = 256; // ≈ one analysis frame per drain → near-frame-rate snapshot sampling
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
    // Delivery snapshot.
    d_syllables_per_second: AtomicU32,
    d_words_per_minute: AtomicU32,
    d_pitch_dynamism: AtomicU32,
    d_pause_fraction: AtomicU32,
    d_pause_count: AtomicU32,
    d_clarity: AtomicU32,
    // Underlying signal snapshot (continuity with M0).
    s_pitch_hz: AtomicU32,
    s_pitch_confidence: AtomicU32,
    s_voicing_score: AtomicU32,
    s_voicing_state: AtomicU32,
    s_onset_flux_high: AtomicU32,
    s_spectral_flux: AtomicU32,
    s_hnr_db: AtomicU32,
    stop: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: SampleRing::new(RING_CAPACITY),
            d_syllables_per_second: AtomicU32::new(0),
            d_words_per_minute: AtomicU32::new(0),
            d_pitch_dynamism: AtomicU32::new(0),
            d_pause_fraction: AtomicU32::new(0),
            d_pause_count: AtomicU32::new(0),
            d_clarity: AtomicU32::new(0),
            s_pitch_hz: AtomicU32::new(0),
            s_pitch_confidence: AtomicU32::new(0),
            s_voicing_score: AtomicU32::new(0),
            s_voicing_state: AtomicU32::new(0),
            s_onset_flux_high: AtomicU32::new(0),
            s_spectral_flux: AtomicU32::new(0),
            s_hnr_db: AtomicU32::new(0),
            stop: AtomicBool::new(false),
        }
    }

    fn publish_delivery(&self, d: &DeliverySnapshot) {
        self.d_syllables_per_second
            .store(d.syllables_per_second.to_bits(), Ordering::Relaxed);
        self.d_words_per_minute
            .store(d.words_per_minute.to_bits(), Ordering::Relaxed);
        self.d_pitch_dynamism
            .store(d.pitch_dynamism_semitones.to_bits(), Ordering::Relaxed);
        self.d_pause_fraction
            .store(d.pause_fraction.to_bits(), Ordering::Relaxed);
        self.d_pause_count.store(d.pause_count, Ordering::Relaxed);
        self.d_clarity.store(d.clarity.to_bits(), Ordering::Relaxed);
    }

    fn publish_signal(&self, s: &SignalSnapshot) {
        self.s_pitch_hz
            .store(s.pitch_hz.to_bits(), Ordering::Relaxed);
        self.s_pitch_confidence
            .store(s.pitch_confidence.to_bits(), Ordering::Relaxed);
        self.s_voicing_score
            .store(s.voicing_score.to_bits(), Ordering::Relaxed);
        self.s_voicing_state
            .store(s.voicing_state.to_bits(), Ordering::Relaxed);
        self.s_onset_flux_high
            .store(s.onset_flux_high.to_bits(), Ordering::Relaxed);
        self.s_spectral_flux
            .store(s.spectral_flux.to_bits(), Ordering::Relaxed);
        self.s_hnr_db.store(s.hnr_db.to_bits(), Ordering::Relaxed);
    }

    fn delivery_snapshot(&self) -> DeliverySnapshot {
        DeliverySnapshot {
            syllables_per_second: f32::from_bits(
                self.d_syllables_per_second.load(Ordering::Relaxed),
            ),
            words_per_minute: f32::from_bits(self.d_words_per_minute.load(Ordering::Relaxed)),
            pitch_dynamism_semitones: f32::from_bits(self.d_pitch_dynamism.load(Ordering::Relaxed)),
            pause_fraction: f32::from_bits(self.d_pause_fraction.load(Ordering::Relaxed)),
            pause_count: self.d_pause_count.load(Ordering::Relaxed),
            clarity: f32::from_bits(self.d_clarity.load(Ordering::Relaxed)),
        }
    }

    fn signal_snapshot(&self) -> SignalSnapshot {
        SignalSnapshot {
            pitch_hz: f32::from_bits(self.s_pitch_hz.load(Ordering::Relaxed)),
            pitch_confidence: f32::from_bits(self.s_pitch_confidence.load(Ordering::Relaxed)),
            voicing_score: f32::from_bits(self.s_voicing_score.load(Ordering::Relaxed)),
            voicing_state: f32::from_bits(self.s_voicing_state.load(Ordering::Relaxed)),
            onset_flux_high: f32::from_bits(self.s_onset_flux_high.load(Ordering::Relaxed)),
            spectral_flux: f32::from_bits(self.s_spectral_flux.load(Ordering::Relaxed)),
            hnr_db: f32::from_bits(self.s_hnr_db.load(Ordering::Relaxed)),
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
