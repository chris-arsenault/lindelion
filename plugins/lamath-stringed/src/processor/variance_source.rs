//! Humanize steady-state variance for the bowed string: random walks for intonation, bow
//! position, speed, and pressure.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) const HUMANIZE_NOMINAL_INTONATION_CENTS: f32 = 4.0;
pub(super) const HUMANIZE_NOMINAL_POSITION_DEPTH: f32 = 0.015;
pub(super) const HUMANIZE_NOMINAL_SPEED_DEPTH: f32 = 0.05;
pub(super) const HUMANIZE_NOMINAL_PRESSURE_DEPTH: f32 = 0.06;
/// Schelleng-axis multiplier at knob 1.0 (worst-case corner at the boundary).
pub(super) const HUMANIZE_CONE_MAX_MULTIPLIER: f32 = 1.5;
// Walk periods: incommensurate so the combined drift never cycles audibly;
// the left hand moves slower than the bow arm.
pub(super) const INTONATION_WALK_TARGET_SECONDS: f32 = 0.83;
pub(super) const INTONATION_WALK_SMOOTH_SECONDS: f32 = 0.61;
pub(super) const POSITION_WALK_TARGET_SECONDS: f32 = 0.59;
pub(super) const POSITION_WALK_SMOOTH_SECONDS: f32 = 0.43;
pub(super) const SPEED_WALK_TARGET_SECONDS: f32 = 0.41;
pub(super) const SPEED_WALK_SMOOTH_SECONDS: f32 = 0.29;
pub(super) const PRESSURE_WALK_TARGET_SECONDS: f32 = 0.31;
pub(super) const PRESSURE_WALK_SMOOTH_SECONDS: f32 = 0.23;
pub(super) const INTONATION_WALK_TAG: u32 = 0x5A17_C3E9;
pub(super) const POSITION_WALK_TAG: u32 = 0xB42D_71F5;
pub(super) const SPEED_WALK_TAG: u32 = 0x39E8_D5A1;
pub(super) const PRESSURE_WALK_TAG: u32 = 0xC7F1_2B63;
/// Cents-to-frequency-ratio small-angle factor (`ln 2 / 1200`); exact within
/// a part in 10^5 over the wander range.
pub(super) const CENTS_TO_RATIO: f32 = 0.000_577_623;

/// Per-axis humanize depths after the single knob's linked scaling.

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct StringSteadyVariance {
    pub(super) intonation_cents: f32,
    pub(super) position_depth: f32,
    pub(super) speed_depth: f32,
    pub(super) pressure_depth: f32,
}

impl StringSteadyVariance {
    pub(super) fn from_humanize(humanize: f32) -> Self {
        let humanize = if humanize.is_finite() {
            humanize.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if humanize <= f32::EPSILON {
            return Self::default();
        }
        // 0..0.5 ramps to nominal; 0.5..1.0 continues toward the unmusical
        // ceiling (Schelleng axes capped at the crush boundary).
        let nominal_scale = (2.0 * humanize).min(1.0);
        let cone_scale = if humanize <= 0.5 {
            2.0 * humanize
        } else {
            1.0 + (2.0 * humanize - 1.0) * (HUMANIZE_CONE_MAX_MULTIPLIER - 1.0)
        };
        let intonation_scale = if humanize <= 0.5 {
            nominal_scale
        } else {
            2.0 * humanize
        };
        Self {
            intonation_cents: HUMANIZE_NOMINAL_INTONATION_CENTS * intonation_scale,
            position_depth: HUMANIZE_NOMINAL_POSITION_DEPTH * cone_scale,
            speed_depth: HUMANIZE_NOMINAL_SPEED_DEPTH * cone_scale,
            pressure_depth: HUMANIZE_NOMINAL_PRESSURE_DEPTH * cone_scale,
        }
    }

    fn is_active(self) -> bool {
        self.intonation_cents > 0.0
            || self.position_depth > 0.0
            || self.speed_depth > 0.0
            || self.pressure_depth > 0.0
    }
}

/// Current per-sample humanize offsets in each axis's own units.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct StringVarianceOffsets {
    pub(super) intonation_cents: f32,
    pub(super) position: f32,
    pub(super) speed: f32,
    pub(super) pressure: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StringVarianceSource {
    pub(super) intonation: SmoothNoise,
    pub(super) position: SmoothNoise,
    pub(super) speed: SmoothNoise,
    pub(super) pressure: SmoothNoise,
}

impl StringVarianceSource {
    pub(super) fn new(sample_rate: f32) -> Self {
        let instance = variance::variance_instance_seed();
        Self {
            intonation: SmoothNoise::new(
                sample_rate,
                INTONATION_WALK_TARGET_SECONDS,
                INTONATION_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, INTONATION_WALK_TAG),
            ),
            position: SmoothNoise::new(
                sample_rate,
                POSITION_WALK_TARGET_SECONDS,
                POSITION_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, POSITION_WALK_TAG),
            ),
            speed: SmoothNoise::new(
                sample_rate,
                SPEED_WALK_TARGET_SECONDS,
                SPEED_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, SPEED_WALK_TAG),
            ),
            pressure: SmoothNoise::new(
                sample_rate,
                PRESSURE_WALK_TARGET_SECONDS,
                PRESSURE_WALK_SMOOTH_SECONDS,
                variance::walk_seed(instance, PRESSURE_WALK_TAG),
            ),
        }
    }

    pub(super) fn process(&mut self, variance: StringSteadyVariance) -> StringVarianceOffsets {
        if !variance.is_active() {
            return StringVarianceOffsets::default();
        }
        StringVarianceOffsets {
            intonation_cents: self.intonation.process() * variance.intonation_cents,
            position: self.position.process() * variance.position_depth,
            speed: self.speed.process() * variance.speed_depth,
            pressure: self.pressure.process() * variance.pressure_depth,
        }
    }
}
