//! Shared struck-body physical models.
//!
//! These are host-neutral DSP kernels for persistent idiophone-style instruments.
//! Product crates own MIDI, patching, sample loading, and UI policy.

pub mod plate;
mod runtime;

pub use lindelion_dsp_utils::energy::EnergyFollower;
pub use runtime::{MeshResonator, MeshVoiceParams};

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

pub(crate) fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    }
}
