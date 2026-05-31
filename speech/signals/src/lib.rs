//! Speech analysis-signal derivation library.
//!
//! Cheap signals (envelope/filter) are computed inline on the audio thread, allocation-free; the
//! heavy signals (SwiftF0 voicing and FFT-based flux/HNR) are computed off-thread in the analysis
//! worker and read back through a lock-free handoff. See `HOTMIC-PORT-PLAN.md` (M2) for the
//! realtime strategy and the benchmark gate.

#![forbid(unsafe_code)]

pub mod analyzer;
pub mod inline;
pub mod worker;

pub use analyzer::{SignalAnalyzer, SignalSnapshot};
pub use inline::{FricativeActivity, SibilanceEnergy, SpeechPresence};
pub use worker::AnalysisWorker;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
