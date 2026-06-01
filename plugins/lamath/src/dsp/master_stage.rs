//! M11 P9: master output safety stage.
//!
//! A per-sample soft clipper on the final stereo mix (after the per-voice output
//! staging and the sympathetic chamber). Below a −6 dBFS knee it is the identity, so
//! single notes and quiet passages pass through bit-exact; above the knee it follows a
//! soft-knee curve that asymptotes to a −1 dBFS ceiling, so dense polyphony is caught
//! transparently instead of hard-clipping.
//!
//! Deliberately **not** a loudness normalizer — the per-family makeup already sets the
//! level, and auto-normalizing would squash the M11 P8 dynamic range. No lookahead and
//! no allocation (ADR-0001): a stateless waveshaper, so it adds no latency.

use lindelion_dsp_utils::math::{finite_or, snap_to_zero};

/// Knee magnitude: at or below this the clipper is the identity (`db_to_gain(−6 dBFS)`).
const MASTER_CLIP_KNEE: f32 = 0.501_187_2;
/// Ceiling the soft knee asymptotes to (`db_to_gain(−1 dBFS)`); output magnitude never
/// exceeds this, so the master output stays at least 1 dB below full scale.
const MASTER_CLIP_CEILING: f32 = 0.891_250_9;

/// Stateless master soft clipper applied to the final stereo mix.
#[derive(Debug, Default)]
pub(crate) struct MasterStage;

impl MasterStage {
    pub(crate) fn new() -> Self {
        Self
    }

    /// Soft-clip each sample of the stereo block in place.
    pub(crate) fn process_block(&self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        for index in 0..len {
            left[index] = soft_clip(left[index]);
            right[index] = soft_clip(right[index]);
        }
    }
}

/// Identity below the knee; above it, a soft knee that asymptotes to the ceiling.
/// `C1`-continuous at the knee (unit slope on both sides), so there is no audible
/// corner when a peak crosses into the limiting region.
fn soft_clip(sample: f32) -> f32 {
    let sample = finite_or(snap_to_zero(sample), 0.0);
    let magnitude = sample.abs();
    if magnitude <= MASTER_CLIP_KNEE {
        return sample;
    }
    let range = MASTER_CLIP_CEILING - MASTER_CLIP_KNEE;
    let shaped = MASTER_CLIP_KNEE + range * ((magnitude - MASTER_CLIP_KNEE) / range).tanh();
    snap_to_zero(sample.signum() * shaped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_no_allocations;

    #[test]
    fn identity_below_the_knee() {
        // A sine that never exceeds the knee passes through bit-exact — quiet passages
        // and single notes are untouched.
        let stage = MasterStage::new();
        let mut left: Vec<f32> = (0..512)
            .map(|index| 0.4 * (std::f32::consts::TAU * index as f32 / 64.0).sin())
            .collect();
        let mut right = left.clone();
        let original = left.clone();
        stage.process_block(&mut left, &mut right);
        assert_eq!(left, original, "sub-knee input must be identity");
        assert_eq!(right, original, "sub-knee input must be identity");
    }

    #[test]
    fn soft_clips_hot_input_below_the_ceiling() {
        // Hot input is shaped down toward — but never reaching — the ceiling, so the
        // master output never clips full scale.
        for hot in [0.6_f32, 1.0, 5.0, 50.0, -3.0] {
            let clipped = soft_clip(hot);
            assert!(clipped.is_finite(), "clip({hot}) not finite: {clipped}");
            assert!(
                clipped.abs() <= MASTER_CLIP_CEILING,
                "clip({hot})={clipped} should never exceed the ceiling {MASTER_CLIP_CEILING}"
            );
            assert_eq!(clipped.signum(), hot.signum(), "clip must preserve sign");
        }
    }

    #[test]
    fn process_block_does_not_allocate() {
        let stage = MasterStage::new();
        let mut left = vec![0.7_f32; 512];
        let mut right = vec![-0.7_f32; 512];
        assert_no_allocations("master_stage_process_block", || {
            stage.process_block(&mut left, &mut right);
        });
    }
}
