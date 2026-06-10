//! Prepared/current string operators: smoothed parameters, material caching, tuning, and the
//! dispersive endpoint reflection.

#![allow(clippy::wildcard_imports)]

use super::*;

impl StringModel {
    /// Smooth continuous material inputs toward their targets. Played pitch is
    /// handled separately as physical speaking-length state.
    pub(super) fn smoothed_params(&mut self, params: String1dParams) -> String1dParams {
        String1dParams {
            loop_gain: self.loop_gain.next(params.loop_gain),
            loop_filter_cutoff: self.loop_filter_cutoff.next(params.loop_filter_cutoff),
            loop_filter_resonance: self
                .loop_filter_resonance
                .next(params.loop_filter_resonance),
            dispersion: self.dispersion.next(params.dispersion),
            ..params
        }
    }

    pub(super) fn current_tuning_frequency(&mut self, target_frequency_hz: f32) -> f32 {
        let target_frequency_hz = core::sanitize_frequency(self.sample_rate, target_frequency_hz);
        let target_delay = self.sample_rate / (2.0 * target_frequency_hz);
        let current_delay = self.tuning_delay.next(target_delay).max(1.0);
        core::sanitize_frequency(self.sample_rate, self.sample_rate / (2.0 * current_delay))
    }

    pub(super) fn current_operators(
        &self,
        params: String1dParams,
        prepared: PreparedStringModel,
    ) -> CurrentStringOperators {
        let damping =
            core::loop_damping_from_material(self.sample_rate, params, prepared.loop_material);
        let dispersion_profile = dispersion::dispersion_profile_for_frequency(
            self.sample_rate,
            params.frequency_hz,
            params.dispersion,
        );
        let tuning = core::delay_tuning(
            self.sample_rate,
            self.waves.capacity(),
            params.frequency_hz,
            2.0,
            // The loop filter is applied at one termination only (once per round
            // trip), so it contributes half its group delay per one-way pass.
            1.0 + 0.5 * damping.filter_delay_samples
                + dispersion_profile.delay_compensation_samples,
        );

        CurrentStringOperators {
            dispersion_profile,
            one_way_delay: tuning.integer_delay + tuning.fractional_delay,
            reflection_gain: core::endpoint_reflection_gain(damping.loop_gain),
        }
    }

    /// Return cached material operators, re-deriving them only when instrument
    /// identity/material inputs move. Played pitch is intentionally excluded.
    pub(super) fn prepared_model(&mut self, params: StringMaterialParams) -> PreparedStringModel {
        if let Some((cached_params, prepared)) = self.prepared
            && cached_params == params
        {
            return prepared;
        }

        let material_probe = String1dParams {
            frequency_hz: 220.0,
            loop_filter_cutoff: params.loop_filter_cutoff,
            loop_filter_resonance: params.loop_filter_resonance,
            loop_gain: params.loop_gain,
            loop_nonlinearity: params.loop_nonlinearity,
            dispersion: params.dispersion,
            strike_position: params.strike_position,
            pickup_position: params.pickup_position,
        };
        let loop_material = core::loop_material(self.sample_rate, material_probe);
        let geometry = core::waveguide_geometry(params.strike_position, params.pickup_position);

        // Apply the loop filter at a single termination (once per round trip): the
        // gain compensation in `loop_damping` divides out one filter peak, so a
        // second pass would make the resonant round-trip gain exceed unity.
        self.terminations
            .set_coefficients(loop_material.coefficients, BiquadCoefficients::identity());

        let prepared = PreparedStringModel {
            loop_material,
            geometry,
        };
        self.prepared = Some((params, prepared));
        #[cfg(test)]
        {
            self.recompute_count += 1;
        }
        prepared
    }

    pub(super) fn reflected_sample(
        &mut self,
        input: f32,
        reflection_gain: f32,
        side: BoundarySide,
        params: String1dParams,
        dispersion_profile: dispersion::DispersionProfile,
    ) -> f32 {
        let filtered = self.terminations.process(side, input);
        let loop_nonlinearity = math::finite_clamp(params.loop_nonlinearity, 0.0, 1.0, 0.0);
        let nonlinear = if loop_nonlinearity > 0.0 {
            soft_saturate(filtered, loop_nonlinearity)
        } else {
            filtered
        };
        let dispersed = match side {
            BoundarySide::Left => self
                .left_dispersion
                .process_sample(nonlinear, dispersion_profile),
            BoundarySide::Right => self
                .right_dispersion
                .process_sample(nonlinear, dispersion_profile),
        };
        math::snap_to_zero(-dispersed * reflection_gain)
    }
}

/// Shorten the effective one-way delay as a function of the string's own stored
/// energy (tension modulation; Bank/Sujbert, Tolonen/Välimäki). The delay only
/// ever shortens (`drive >= 0`) and never below `one_way_delay / (1 + DEPTH*MAX)`,
/// so it stays bounded and within the fixed traveling-wave capacity; at
/// `drive == 0` it returns the nominal delay (tuning unaffected).
pub(super) fn tension_modulated_delay(one_way_delay: f32, drive: f32) -> f32 {
    let drive = math::finite_clamp(drive, 0.0, STRING_TENSION_MAX_DRIVE, 0.0);
    one_way_delay / (1.0 + STRING_TENSION_DEPTH * drive)
}
