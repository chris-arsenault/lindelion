//! Compute-once shared analysis for the chain. One off-thread [`AnalysisWorker`] produces the
//! [`SignalSnapshot`] the chain injects into the SwiftF0-consuming slots, rather than each effect
//! owning its own worker. Analysis stays off the audio thread (ADR-0001); the snapshot is
//! control-rate and lag-tolerant. M2 sequences the slots and injects this snapshot before each
//! slot's `process`.

use lindelion_speech_signals::{AnalysisWorker, SignalSnapshot};

/// Owns the chain's single analysis worker.
pub struct SharedAnalysis {
    worker: AnalysisWorker,
}

impl SharedAnalysis {
    /// Create the shared analysis for audio at `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            worker: AnalysisWorker::new(sample_rate),
        }
    }

    /// Feed one block of input and return the latest analysis snapshot. Allocation-free; the heavy
    /// analysis runs off-thread, so the returned snapshot reflects recent (not necessarily this
    /// block's) audio.
    pub fn update(&self, block: &[f32]) -> SignalSnapshot {
        self.worker.push(block);
        self.worker.latest()
    }
}
