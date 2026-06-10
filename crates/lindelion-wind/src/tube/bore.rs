//! Bore endpoint profile: mouth-loss coefficients, end reflections, and pickup mix.

use lindelion_dsp_utils::{filters::BiquadCoefficients, math};

use super::super::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY, core, traveling::PickupSamples};
use super::{
    MIN_END_REFLECTION_MAGNITUDE, MOUTH_REFLECTION, REGISTER_MODE_RATIO_MAX,
    REGISTER_MODE_RATIO_MIN, ReedTubeParams, STEEPEN_ENERGY_REF, STEEPEN_MAX_ENERGY,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TubeBoreProfile {
    pub(super) mouth_loss: BiquadCoefficients,
    pub(super) mouth_reflection: f32,
    pub(super) end_reflection: f32,
    pub(super) pressure_mix: f32,
}

impl TubeBoreProfile {
    pub(super) fn from_params(
        sample_rate: f32,
        params: ReedTubeParams,
        loop_gain: f32,
        bore_cutoff_hz: f32,
    ) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let endpoint_loss = core::endpoint_reflection_gain(loop_gain);
        let end_reflection = bore_end_reflection(params.boundary_reflection) * endpoint_loss;
        let openness = (1.0 - TUBE_BOUNDARY.reflection(params.boundary_reflection)) * 0.5;
        let mouth_cutoff = math::finite_clamp(
            bore_cutoff_hz * (0.90 + 0.15 * openness),
            160.0,
            sample_rate * 0.45,
            1_900.0,
        );

        Self {
            mouth_loss: BiquadCoefficients::lowpass(sample_rate, mouth_cutoff, DEFAULT_BIQUAD_Q),
            mouth_reflection: MOUTH_REFLECTION * endpoint_loss,
            end_reflection,
            pressure_mix: math::finite_clamp(0.30 + 0.60 * (1.0 - openness), 0.2, 0.95, 0.65),
        }
    }

    pub(super) fn pickup_sample(self, pickup: PickupSamples) -> f32 {
        let pressure = pickup.average();
        let flow = (pickup.right - pickup.left) * 0.5;
        math::snap_to_zero(pressure * self.pressure_mix + flow * (1.0 - self.pressure_mix))
    }
}

pub(super) fn bore_end_reflection(boundary_reflection: f32) -> f32 {
    let reflection = TUBE_BOUNDARY.reflection(boundary_reflection);
    if reflection.abs() < MIN_END_REFLECTION_MAGNITUDE {
        MIN_END_REFLECTION_MAGNITUDE.copysign(reflection)
    } else {
        reflection
    }
}

pub(super) fn bore_frequency_hz(params: ReedTubeParams) -> f32 {
    let ratio = math::finite_clamp(
        params.register_mode_ratio,
        REGISTER_MODE_RATIO_MIN,
        REGISTER_MODE_RATIO_MAX,
        1.0,
    );
    math::finite_or(params.frequency_hz / ratio, params.frequency_hz).max(1.0)
}

pub(super) fn steepening_energy(energy: f32) -> f32 {
    let normalized = math::finite_or(energy, 0.0).max(0.0) / STEEPEN_ENERGY_REF;
    math::finite_clamp(normalized * normalized, 0.0, STEEPEN_MAX_ENERGY, 0.0)
}
