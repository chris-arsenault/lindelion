//! Slow random-walk variance sources for humanization.
//!
//! A humanized physical control is the player's input drifting, not the sound
//! being post-processed: each axis runs a [`SmoothNoise`] walk — a new uniform
//! target every `target_seconds`, approached through a one-pole over
//! `smooth_seconds` — whose output offsets that axis's physical target.
//! Axes use deliberately incommensurate periods so the combined motion never
//! cycles audibly. Walks are seeded per instance ([`variance_instance_seed`])
//! and per axis ([`walk_seed`]), so concurrent voices decorrelate while a
//! fixed construction order (offline renders, tests) stays reproducible.

use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_VARIANCE_INSTANCE: AtomicU32 = AtomicU32::new(0x6C8E_9CF5);

/// Draw a fresh per-instance seed for a variance source.
pub fn variance_instance_seed() -> u32 {
    NEXT_VARIANCE_INSTANCE.fetch_add(0x9E37_79B9, Ordering::Relaxed)
}

/// Mix an instance seed with a per-axis tag into a walk seed (SplitMix-style
/// finalizer, so adjacent instances/tags land far apart in state space).
pub fn walk_seed(instance: u32, tag: u32) -> u32 {
    let mut x = instance ^ tag;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

/// A smoothed uniform random walk in `[-1, 1]`: a new target every
/// `target_seconds`, approached through a one-pole with time constant
/// `smooth_seconds`. Allocation-free and `Copy`; suitable for per-sample use
/// on the audio thread.
#[derive(Debug, Clone, Copy)]
pub struct SmoothNoise {
    state: u32,
    value: f32,
    target: f32,
    coeff: f32,
    target_samples: usize,
    samples_until_target: usize,
}

impl SmoothNoise {
    pub fn new(sample_rate: f32, target_seconds: f32, smooth_seconds: f32, seed: u32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let target_samples = ((target_seconds.max(0.001) * sample_rate).round() as usize).max(1);
        let coeff = 1.0 - (-1.0 / (smooth_seconds.max(0.001) * sample_rate)).exp();
        let mut noise = Self {
            state: seed,
            value: 0.0,
            target: 0.0,
            coeff,
            target_samples,
            samples_until_target: target_samples,
        };
        let initial = noise.next_noise();
        noise.value = initial;
        noise.target = initial;
        noise
    }

    pub fn process(&mut self) -> f32 {
        if self.samples_until_target == 0 {
            self.target = self.next_noise();
            self.samples_until_target = self.target_samples;
        }
        self.samples_until_target = self.samples_until_target.saturating_sub(1);
        self.value += (self.target - self.value) * self.coeff;
        self.value
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_stays_bounded_and_moves() {
        let mut noise = SmoothNoise::new(48_000.0, 0.05, 0.02, walk_seed(1, 0xA511_E9B3));
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for _ in 0..48_000 {
            let value = noise.process();
            assert!(value.is_finite());
            min = min.min(value);
            max = max.max(value);
        }
        assert!((-1.0..=1.0).contains(&min) && (-1.0..=1.0).contains(&max));
        assert!(max - min > 0.2, "walk barely moved: {min}..{max}");
    }

    #[test]
    fn seeds_decorrelate_axes() {
        let instance = variance_instance_seed();
        let mut a = SmoothNoise::new(48_000.0, 0.05, 0.02, walk_seed(instance, 0x1111_1111));
        let mut b = SmoothNoise::new(48_000.0, 0.05, 0.02, walk_seed(instance, 0x2222_2222));
        let mut difference = 0.0_f32;
        for _ in 0..24_000 {
            difference = difference.max((a.process() - b.process()).abs());
        }
        assert!(difference > 0.1, "axes should decorrelate: {difference}");
    }
}
