//! Shared struck-body physical models.
//!
//! These are host-neutral DSP kernels for persistent idiophone-style instruments.
//! Product crates own MIDI, patching, sample loading, and UI policy.

mod energy;
pub mod mesh;

pub use energy::EnergyFollower;
pub use mesh::{MeshResonator, MeshVoiceParams};

pub(crate) fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    }
}
