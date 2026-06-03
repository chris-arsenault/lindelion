use crate::math::{finite_clamp, finite_or};

/// Time constant of the per-sample energy follower.
const ENERGY_FOLLOWER_SECONDS: f32 = 0.010;

/// Allocation-free per-sample energy/RMS follower.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyFollower {
    mean_square: f32,
    coefficient: f32,
}

impl EnergyFollower {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = finite_or(sample_rate, 48_000.0).max(1.0);
        let coefficient =
            finite_clamp(1.0 / (ENERGY_FOLLOWER_SECONDS * sample_rate), 0.0, 1.0, 1.0);
        Self {
            mean_square: 0.0,
            coefficient,
        }
    }

    pub fn reset(&mut self) {
        self.mean_square = 0.0;
    }

    pub fn observe(&mut self, sample: f32) -> f32 {
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

    #[test]
    fn follower_converges_to_known_rms() {
        let mut follower = EnergyFollower::new(48_000.0);
        let amplitude = 0.5_f32;
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
    fn non_finite_input_stays_finite() {
        let mut follower = EnergyFollower::new(48_000.0);
        assert!(follower.observe(f32::NAN).is_finite());
        assert!(follower.observe(f32::INFINITY).is_finite());
        assert!(follower.observe(0.25).is_finite());
    }
}
