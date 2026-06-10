//! Frequency-shaped mesh boundary loss (M11 P2 step 3). A waveguide mesh reflects
//! ~2000×/s, so a full lowpass on the reflection annihilates the high modes in
//! milliseconds; instead each reflection applies a gentle high-shelf loss so the
//! high plate modes die first (metallic shimmer) while the low modes are reflected
//! essentially losslessly and the base `damping` stays the decay control.

use lindelion_dsp_utils::math;

use super::super::sanitize_sample_rate;

/// Crossover of the per-edge boundary loss: the one-pole lowpass that separates
/// the long-ringing low modes from the high modes that should die first. Content
/// above this is treated as "high" and bled off by `BOUNDARY_HF_LOSS` per
/// reflection.
const BOUNDARY_LOSS_CUTOFF_HZ: f32 = 2_500.0;

/// Fraction of the *high-frequency* content removed at each boundary reflection.
/// Tiny because the mesh reflects ~2000×/s: at ~1e-3 the high modes ring a
/// metallic shimmer that fades a few seconds before the low modes (highs die
/// first), while the low modes are reflected essentially losslessly.
pub(super) const DEFAULT_BOUNDARY_HF_LOSS: f32 = 0.001_2;

/// One-pole lowpass state per boundary cell, used to compute a gentle high-shelf
/// loss so each reflection is frequency shaped (highs die first) without dumping
/// the high-mode energy. Sized once at construction from the grid edges; the audio
/// thread only reads/writes existing slots, never allocates.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BoundaryLowpass {
    pub(super) coeff: f32,
    pub(super) left: Vec<f32>,
    pub(super) right: Vec<f32>,
    pub(super) top: Vec<f32>,
    pub(super) bottom: Vec<f32>,
}

impl BoundaryLowpass {
    pub(super) fn new(width: usize, height: usize, sample_rate: f32) -> Self {
        Self {
            coeff: Self::coeff_for(sample_rate),
            left: vec![0.0; height],
            right: vec![0.0; height],
            top: vec![0.0; width],
            bottom: vec![0.0; width],
        }
    }

    /// One-pole coefficient for `BOUNDARY_LOSS_CUTOFF_HZ` at the mesh rate. Clamped
    /// to `(0, 1]`.
    fn coeff_for(sample_rate: f32) -> f32 {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let omega = std::f32::consts::TAU * BOUNDARY_LOSS_CUTOFF_HZ / sample_rate;
        math::finite_clamp(1.0 - (-omega).exp(), 0.05, 1.0, 1.0)
    }

    pub(super) fn set_sample_rate(&mut self, sample_rate: f32) {
        self.coeff = Self::coeff_for(sample_rate);
    }

    pub(super) fn clear(&mut self) {
        self.left.iter_mut().for_each(|s| *s = 0.0);
        self.right.iter_mut().for_each(|s| *s = 0.0);
        self.top.iter_mut().for_each(|s| *s = 0.0);
        self.bottom.iter_mut().for_each(|s| *s = 0.0);
    }
}

/// Advance the per-cell lowpass and return a gently high-shelved reflection: the
/// low band passes losslessly, the high band (`sample − lowpass`) is attenuated by
/// `BOUNDARY_HF_LOSS`. `reflected = sample − ε·(sample − lowpass)`.
pub(super) fn boundary_lowpass_step(state: &mut f32, coeff: f32, hf_loss: f32, sample: f32) -> f32 {
    *state += coeff * (sample - *state);
    let high_band = sample - *state;
    let hf_loss = math::finite_clamp(hf_loss, 0.0, 0.02, DEFAULT_BOUNDARY_HF_LOSS);
    sample - hf_loss * high_band
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MeshBoundaryKind {
    Fixed,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MeshBoundaryEdge {
    pub(super) kind: MeshBoundaryKind,
    pub(super) damping: f32,
}

impl MeshBoundaryEdge {
    pub(super) fn fixed(damping: f32) -> Self {
        Self {
            kind: MeshBoundaryKind::Fixed,
            damping,
        }
    }

    pub(super) fn free(damping: f32) -> Self {
        Self {
            kind: MeshBoundaryKind::Free,
            damping,
        }
    }

    pub(super) fn reflection(self) -> f32 {
        let sign = match self.kind {
            MeshBoundaryKind::Fixed => -1.0,
            MeshBoundaryKind::Free => 1.0,
        };
        sign * (1.0 - math::finite_clamp(self.damping, 0.0, 1.0, 0.0))
    }
}
