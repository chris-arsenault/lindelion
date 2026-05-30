//! Peak / RMS envelope follower with independent attack and release smoothing.
//!
//! A one-pole detector: the envelope chases the rectified (peak) or squared (RMS) input with an
//! attack coefficient while rising and a release coefficient while falling. Coefficients are
//! `exp(-1 / (time_s * sample_rate))`, so the envelope reaches ~63% of a step in one time
//! constant. Distinct from the musical ADSR in [`crate::envelope`]: this tracks signal level for
//! dynamics processing (gates, compressors, limiters, de-essers).

use crate::math::snap_to_zero;

/// Level-detection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectorMode {
    /// Track the rectified peak `|x|`.
    Peak,
    /// Track RMS (root of a smoothed mean-square).
    Rms,
}

/// One-pole attack/release envelope follower.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeFollower {
    mode: DetectorMode,
    attack_coeff: f32,
    release_coeff: f32,
    state: f32,
}

impl EnvelopeFollower {
    /// Create a follower in `mode`; instantaneous until [`Self::set_times`] is called.
    pub fn new(mode: DetectorMode) -> Self {
        Self {
            mode,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            state: 0.0,
        }
    }

    /// Set attack/release time constants (seconds) for `sample_rate`.
    pub fn set_times(&mut self, attack_s: f32, release_s: f32, sample_rate: f32) {
        self.attack_coeff = time_coeff(attack_s, sample_rate);
        self.release_coeff = time_coeff(release_s, sample_rate);
    }

    /// Clear the envelope state.
    pub fn reset(&mut self) {
        self.state = 0.0;
    }

    /// Advance one sample, returning the current envelope (linear amplitude).
    pub fn process(&mut self, input: f32) -> f32 {
        let target = match self.mode {
            DetectorMode::Peak => input.abs(),
            DetectorMode::Rms => input * input,
        };
        let coeff = if target > self.state {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.state = snap_to_zero(target + coeff * (self.state - target));
        self.envelope()
    }

    /// Current envelope (linear amplitude) without advancing.
    pub fn envelope(&self) -> f32 {
        match self.mode {
            DetectorMode::Peak => self.state,
            DetectorMode::Rms => self.state.max(0.0).sqrt(),
        }
    }
}

fn time_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s <= 0.0 || sample_rate <= 0.0 {
        0.0
    } else {
        (-1.0 / (time_s * sample_rate)).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_reaches_63_percent_in_one_attack_time_constant() {
        let mut follower = EnvelopeFollower::new(DetectorMode::Peak);
        let sr = 48_000.0;
        follower.set_times(0.01, 0.05, sr);
        for _ in 0..(0.01 * sr) as usize {
            follower.process(1.0);
        }
        let env = follower.envelope();
        assert!((env - 0.632).abs() < 0.02, "attack envelope {env}");
    }

    #[test]
    fn process_is_allocation_free() {
        let mut follower = EnvelopeFollower::new(DetectorMode::Peak);
        follower.set_times(0.01, 0.05, 48_000.0);
        lindelion_test_allocator::assert_no_allocations("envelope follower process", || {
            for n in 0..256 {
                follower.process((n as f32 * 0.01).sin());
            }
        });
    }

    #[test]
    fn release_decays_after_input_stops() {
        let mut follower = EnvelopeFollower::new(DetectorMode::Peak);
        let sr = 48_000.0;
        follower.set_times(0.001, 0.05, sr);
        for _ in 0..2_000 {
            follower.process(1.0);
        }
        let peak = follower.envelope();
        for _ in 0..(0.05 * sr) as usize {
            follower.process(0.0);
        }
        let after = follower.envelope();
        assert!(after < peak * 0.45, "release {after} vs peak {peak}");
    }
}
