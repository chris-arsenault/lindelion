use lindelion_dsp_utils::math::{finite_clamp, finite_or};

use crate::dsp::constants::DSP_FALLBACK_SAMPLE_RATE;

/// Time constant of the per-sample energy follower. Short enough to track playing
/// dynamics, long enough to read the vibrating state rather than the waveform.
const ENERGY_FOLLOWER_SECONDS: f32 = 0.010;

/// Allocation-free per-sample energy/RMS follower: a one-pole smoother on the
/// instantaneous power (`sample^2`), reported as RMS (`sqrt(mean_square)`).
///
/// `lindelion-dsp-utils` only has a static `rms` over a slice (ADR-0014); this is the
/// streaming form the effort/energy bus needs. Shared across the dynamic-response
/// stages: the per-voice measured-energy bus (`modulation_state`) and the global
/// sympathetic chamber's energy-scaled send (ADR-0028).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EnergyFollower {
    mean_square: f32,
    coefficient: f32,
}

impl EnergyFollower {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let sample_rate = finite_or(sample_rate, DSP_FALLBACK_SAMPLE_RATE).max(1.0);
        let coefficient =
            finite_clamp(1.0 / (ENERGY_FOLLOWER_SECONDS * sample_rate), 0.0, 1.0, 1.0);
        Self {
            mean_square: 0.0,
            coefficient,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.mean_square = 0.0;
    }

    /// Fold one sample into the follower and return the current RMS estimate.
    pub(crate) fn observe(&mut self, sample: f32) -> f32 {
        let sample = finite_or(sample, 0.0);
        let power = sample * sample;
        let mean_square = finite_or(self.mean_square, 0.0)
            + self.coefficient * (power - finite_or(self.mean_square, 0.0));
        self.mean_square = finite_clamp(mean_square, 0.0, f32::MAX, 0.0);
        self.mean_square.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_no_allocations;

    #[test]
    fn follower_converges_to_known_rms() {
        let mut follower = EnergyFollower::new(48_000.0);
        let amplitude = 0.5_f32;
        // A full-scale square wave at +/-amplitude has RMS == amplitude.
        let mut rms = 0.0;
        for index in 0..8_000 {
            let sample = if index % 2 == 0 {
                amplitude
            } else {
                -amplitude
            };
            rms = follower.observe(sample);
        }

        assert!(
            (rms - amplitude).abs() < 0.01,
            "rms={rms} should approach {amplitude}"
        );
    }

    #[test]
    fn reset_returns_to_zero() {
        let mut follower = EnergyFollower::new(48_000.0);
        for _ in 0..1_000 {
            follower.observe(0.8);
        }
        follower.reset();

        assert_eq!(follower.observe(0.0), 0.0);
    }

    #[test]
    fn non_finite_input_stays_finite() {
        let mut follower = EnergyFollower::new(48_000.0);
        assert!(follower.observe(f32::NAN).is_finite());
        assert!(follower.observe(f32::INFINITY).is_finite());
        assert!(follower.observe(0.25).is_finite());
    }

    #[test]
    fn observe_does_not_allocate() {
        let mut follower = EnergyFollower::new(48_000.0);
        assert_no_allocations("energy_follower_observe", || {
            for index in 0..512 {
                follower.observe((index % 2) as f32 * 0.5 - 0.25);
            }
        });
    }
}
