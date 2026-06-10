//! Humanize steady-state variance: per-note random walks for pressure, embouchure, and voicing.

use lindelion_dsp_utils::variance::{self, SmoothNoise};

use lindelion_dsp_utils::math;

use super::sanitize_sample_rate;

const PRESSURE_VARIANCE_TARGET_SECONDS: f32 = 0.37;
const PRESSURE_VARIANCE_SMOOTH_SECONDS: f32 = 0.26;
const EMBOUCHURE_VARIANCE_TARGET_SECONDS: f32 = 0.53;
const EMBOUCHURE_VARIANCE_SMOOTH_SECONDS: f32 = 0.42;
const VOICING_VARIANCE_TARGET_SECONDS: f32 = 0.71;
const VOICING_VARIANCE_SMOOTH_SECONDS: f32 = 0.55;
const HUMANIZE_PRESSURE_DEPTH: f32 = 0.37;
const HUMANIZE_EMBOUCHURE_DEPTH: f32 = 0.35;
const HUMANIZE_VOICING_DEPTH: f32 = 2.0;

const PRESSURE_WALK_TAG: u32 = 0xA511_E9B3;
const EMBOUCHURE_WALK_TAG: u32 = 0x63D8_35AF;
const VOICING_WALK_TAG: u32 = 0xD1B5_4A32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TubeSteadyVariance {
    pressure_depth: f32,
    embouchure_depth: f32,
    voicing_depth: f32,
}

impl TubeSteadyVariance {
    const OFF: Self = Self {
        pressure_depth: 0.0,
        embouchure_depth: 0.0,
        voicing_depth: 0.0,
    };

    const fn with_voicing(pressure_depth: f32, embouchure_depth: f32, voicing_depth: f32) -> Self {
        Self {
            pressure_depth,
            embouchure_depth,
            voicing_depth,
        }
    }

    pub(super) fn from_humanize(humanize: f32) -> Self {
        let humanize = math::finite_clamp(humanize, 0.0, 1.0, 0.0);
        if humanize <= f32::EPSILON {
            return Self::OFF;
        }
        Self::with_voicing(
            HUMANIZE_PRESSURE_DEPTH * humanize,
            HUMANIZE_EMBOUCHURE_DEPTH * humanize,
            HUMANIZE_VOICING_DEPTH * humanize,
        )
    }

    pub(super) fn sanitized(self) -> Self {
        Self {
            pressure_depth: self.pressure_depth.clamp(0.0, 0.95),
            embouchure_depth: self.embouchure_depth.clamp(0.0, 0.45),
            voicing_depth: self.voicing_depth.clamp(0.0, 2.0),
        }
    }

    fn is_active(self) -> bool {
        self.pressure_depth > 0.0 || self.embouchure_depth > 0.0 || self.voicing_depth > 0.0
    }
}

impl Default for TubeSteadyVariance {
    fn default() -> Self {
        Self::OFF
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SteadyVarianceSource {
    pressure: SmoothNoise,
    embouchure: SmoothNoise,
    voicing: SmoothNoise,
}

impl SteadyVarianceSource {
    pub(super) fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let instance = variance::variance_instance_seed();
        Self {
            pressure: SmoothNoise::new(
                sample_rate,
                PRESSURE_VARIANCE_TARGET_SECONDS,
                PRESSURE_VARIANCE_SMOOTH_SECONDS,
                variance::walk_seed(instance, PRESSURE_WALK_TAG),
            ),
            embouchure: SmoothNoise::new(
                sample_rate,
                EMBOUCHURE_VARIANCE_TARGET_SECONDS,
                EMBOUCHURE_VARIANCE_SMOOTH_SECONDS,
                variance::walk_seed(instance, EMBOUCHURE_WALK_TAG),
            ),
            voicing: SmoothNoise::new(
                sample_rate,
                VOICING_VARIANCE_TARGET_SECONDS,
                VOICING_VARIANCE_SMOOTH_SECONDS,
                variance::walk_seed(instance, VOICING_WALK_TAG),
            ),
        }
    }

    pub(super) fn reset(&mut self, sample_rate: f32) {
        *self = Self::new(sample_rate);
    }

    pub(super) fn process(&mut self, variance: TubeSteadyVariance) -> (f32, f32, f32) {
        if variance.is_active() {
            (
                self.pressure.process() * variance.pressure_depth,
                self.embouchure.process() * variance.embouchure_depth,
                self.voicing.process() * variance.voicing_depth,
            )
        } else {
            (0.0, 0.0, 0.0)
        }
    }
}
