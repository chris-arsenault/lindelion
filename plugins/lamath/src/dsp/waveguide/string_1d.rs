use lindelion_dsp_utils::{filters::BiquadCoefficients, math, soft_saturate};

use super::{
    WaveguideParams,
    body::WaveguideBody,
    core, dispersion,
    traveling::{BoundaryFilters, BoundarySide, TravelingWavePair},
};
use crate::dsp::constants::LOWEST_RESONATOR_FREQUENCY_HZ;

#[derive(Debug, Clone, Copy, PartialEq)]
struct String1dParams {
    frequency_hz: f32,
    loop_filter_cutoff: f32,
    loop_filter_resonance: f32,
    loop_gain: f32,
    loop_nonlinearity: f32,
    dispersion: f32,
    strike_position: f32,
    pickup_position: f32,
}

impl String1dParams {
    fn from_waveguide(params: WaveguideParams) -> Self {
        Self {
            frequency_hz: params.frequency_hz,
            loop_filter_cutoff: params.loop_filter_cutoff,
            loop_filter_resonance: params.loop_filter_resonance,
            loop_gain: params.loop_gain,
            loop_nonlinearity: params.loop_nonlinearity,
            dispersion: params.dispersion,
            strike_position: params.position_of_strike,
            pickup_position: params.pickup_position,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct String1d {
    sample_rate: f32,
    waves: TravelingWavePair,
    terminations: BoundaryFilters,
    left_dispersion: dispersion::WaveguideDispersion,
    right_dispersion: dispersion::WaveguideDispersion,
    body: WaveguideBody,
}

impl String1d {
    pub(super) fn new(sample_rate: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            waves: TravelingWavePair::new(sample_rate, LOWEST_RESONATOR_FREQUENCY_HZ, 2.0),
            terminations: BoundaryFilters::new(),
            left_dispersion: dispersion::WaveguideDispersion::new(),
            right_dispersion: dispersion::WaveguideDispersion::new(),
            body: WaveguideBody::new(sample_rate),
        }
    }

    pub(super) fn reset(&mut self) {
        self.waves.clear();
        self.terminations.reset();
        self.left_dispersion.reset();
        self.right_dispersion.reset();
        self.body.reset();
    }

    /// Production entry point: drive the two-rail string from `WaveguideParams`,
    /// mirroring `Tube1d::process_sample`.
    pub(super) fn process(&mut self, excitation: f32, params: WaveguideParams) -> f32 {
        self.process_sample(excitation, String1dParams::from_waveguide(params))
    }

    fn process_sample(&mut self, excitation: f32, params: String1dParams) -> f32 {
        let waveguide_params = waveguide_params_from_string(params);
        let damping = core::loop_damping(self.sample_rate, waveguide_params);
        let dispersion_profile = dispersion::dispersion_profile(self.sample_rate, waveguide_params);
        let geometry = core::waveguide_geometry(params.strike_position, params.pickup_position);
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
        let one_way_delay = tuning.integer_delay + tuning.fractional_delay;

        // Apply the loop filter at a single termination (once per round trip): the
        // gain compensation in `loop_damping` divides out one filter peak, so a
        // second pass would make the resonant round-trip gain exceed unity.
        self.terminations
            .set_coefficients(damping.coefficients, BiquadCoefficients::identity());

        let boundary = self.waves.boundary_samples(one_way_delay);
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, geometry.pickup_position);

        let reflection_gain = core::endpoint_reflection_gain(damping.loop_gain);
        let left_reflection = self.reflected_sample(
            boundary.left,
            reflection_gain,
            BoundarySide::Left,
            params,
            dispersion_profile,
        );
        let right_reflection = self.reflected_sample(
            boundary.right,
            reflection_gain,
            BoundarySide::Right,
            params,
            dispersion_profile,
        );

        self.waves.push(right_reflection, left_reflection);
        self.waves.add_symmetric_excitation(
            one_way_delay,
            geometry.excitation_taps,
            math::snap_to_zero(excitation),
        );

        self.body.process_sample(pickup.average(), waveguide_params)
    }

    fn reflected_sample(
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

fn waveguide_params_from_string(params: String1dParams) -> WaveguideParams {
    WaveguideParams {
        frequency_hz: params.frequency_hz,
        loop_filter_cutoff: params.loop_filter_cutoff,
        loop_filter_resonance: params.loop_filter_resonance,
        loop_gain: params.loop_gain,
        loop_nonlinearity: params.loop_nonlinearity,
        dispersion: params.dispersion,
        position_of_strike: params.strike_position,
        pickup_position: params.pickup_position,
        ..WaveguideParams::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{
        constants::WAVEGUIDE_PICKUP_POSITION,
        render_metrics::{RenderExcitation, render_response},
    };
    use lindelion_dsp_utils::analysis::{
        assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation,
        first_index_above_abs, rms_difference,
    };
    use lindelion_dsp_utils::math::cents_between;

    #[test]
    fn string_1d_renders_finite_decaying_audio() {
        let sample_rate = 48_000.0;
        let output = render_string_1d(
            sample_rate,
            String1dParams {
                frequency_hz: 220.0,
                loop_filter_cutoff: 6_000.0,
                loop_filter_resonance: 0.1,
                loop_gain: 0.82,
                loop_nonlinearity: 0.0,
                dispersion: 0.0,
                strike_position: 0.34,
                pickup_position: 0.78,
            },
            24_000,
            RenderExcitation::ShapedPluck,
        );
        let early = audio_window_metrics(&output[512..2_560], sample_rate);
        let late = audio_window_metrics(&output[12_000..14_048], sample_rate);

        assert_all_finite(&output);
        assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
        assert!(early.peak_abs < 4.0);
    }

    #[test]
    fn string_1d_pitch_tracks_target_matrix() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for target_hz in [110.0, 220.0, 440.0, 880.0] {
                let output = render_string_1d(
                    sample_rate,
                    String1dParams {
                        frequency_hz: target_hz,
                        loop_filter_cutoff: 18_000.0,
                        loop_filter_resonance: 0.0,
                        loop_gain: 0.99,
                        loop_nonlinearity: 0.0,
                        dispersion: 0.0,
                        strike_position: 0.37,
                        pickup_position: 0.73,
                    },
                    (sample_rate * 0.32) as usize,
                    RenderExcitation::Impulse,
                );
                let estimate = estimate_f0_autocorrelation(
                    &output[1_024..],
                    sample_rate,
                    target_hz * 0.8,
                    target_hz * 1.25,
                )
                .unwrap();
                let cents = cents_between(target_hz, estimate);

                assert!(
                    cents < 80.0,
                    "sample_rate={sample_rate}, target_hz={target_hz}, estimate={estimate}, cents={cents}"
                );
            }
        }
    }

    #[test]
    fn string_1d_separates_strike_and_pickup_positions() {
        let sample_rate = 48_000.0;
        let base = String1dParams::from_waveguide(WaveguideParams {
            frequency_hz: 240.0,
            loop_filter_cutoff: 12_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.96,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.2,
            ..WaveguideParams::default()
        });
        let near_pickup = render_string_1d(
            sample_rate,
            String1dParams {
                pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
                ..base
            },
            4_096,
            RenderExcitation::ShapedPluck,
        );
        let far_pickup = render_string_1d(
            sample_rate,
            String1dParams {
                pickup_position: 0.35,
                ..base
            },
            4_096,
            RenderExcitation::ShapedPluck,
        );

        let near_onset = first_index_above_abs(&near_pickup, 0.000_1).unwrap();
        let far_onset = first_index_above_abs(&far_pickup, 0.000_1).unwrap();
        let difference = rms_difference(&near_pickup[256..], &far_pickup[256..]);

        assert_ne!(near_onset, far_onset);
        assert!(difference > 0.000_01, "difference={difference}");
    }

    #[test]
    fn string_1d_dispersion_materially_changes_render() {
        let sample_rate = 48_000.0;
        let base = String1dParams::from_waveguide(WaveguideParams {
            frequency_hz: 220.0,
            loop_filter_cutoff: 18_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.985,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.37,
            ..WaveguideParams::default()
        });
        let natural = render_string_1d(sample_rate, base, 16_000, RenderExcitation::ShapedPluck);
        let dispersed = render_string_1d(
            sample_rate,
            String1dParams {
                dispersion: 0.85,
                ..base
            },
            16_000,
            RenderExcitation::ShapedPluck,
        );

        assert_all_finite(&natural);
        assert_all_finite(&dispersed);
        assert!(rms_difference(&natural[512..], &dispersed[512..]) > 0.000_001);
    }

    #[test]
    fn string_1d_reset_clears_state() {
        let sample_rate = 48_000.0;
        let params = String1dParams::from_waveguide(WaveguideParams {
            frequency_hz: 220.0,
            loop_gain: 0.98,
            ..WaveguideParams::default()
        });
        let mut string = String1d::new(sample_rate);
        let _ = render_response(
            sample_rate,
            params.frequency_hz,
            4_096,
            RenderExcitation::Impulse,
            |sample| string.process_sample(sample, params),
        );
        string.reset();

        let output = (0..512)
            .map(|_| string.process_sample(0.0, params))
            .collect::<Vec<_>>();

        assert_all_finite(&output);
        assert!(audio_window_metrics(&output, sample_rate).peak_abs < 0.000_001);
    }

    #[test]
    fn string_1d_low_note_near_capacity_tracks_target() {
        // A low note near the buffer-capacity limit must still track its target;
        // this guards the shared frequency sanitizer used by both the delay length
        // and the filter-delay compensation.
        let sample_rate = 48_000.0;
        let target_hz = 35.0;
        let output = render_string_1d(
            sample_rate,
            String1dParams {
                frequency_hz: target_hz,
                loop_filter_cutoff: 16_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.99,
                loop_nonlinearity: 0.0,
                dispersion: 0.0,
                strike_position: 0.4,
                pickup_position: 0.7,
            },
            32_000,
            RenderExcitation::Impulse,
        );
        let estimate = estimate_f0_autocorrelation(
            &output[2_048..],
            sample_rate,
            target_hz * 0.8,
            target_hz * 1.25,
        )
        .unwrap();
        let cents = cents_between(target_hz, estimate);
        assert!(cents < 80.0, "estimate={estimate}, cents={cents}");
    }

    #[test]
    fn string_1d_resonant_loop_decays_without_high_frequency_growth() {
        // Regression for the loop filter being applied at both terminations: with a
        // low cutoff and a resonant loop, the round-trip peak gain exceeded 1 and
        // the partials grew over time. Applying the filter once per round trip keeps
        // the decay monotonic.
        let sample_rate = 48_000.0;
        let output = render_string_1d(
            sample_rate,
            String1dParams {
                frequency_hz: 220.0,
                loop_filter_cutoff: 2_400.0,
                loop_filter_resonance: 0.1,
                loop_gain: 0.975,
                loop_nonlinearity: 0.0,
                dispersion: 0.0,
                strike_position: 0.42,
                pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
            },
            24_000,
            RenderExcitation::ShapedPluck,
        );
        let early = audio_window_metrics(&output[512..2_560], sample_rate);
        let late = audio_window_metrics(&output[20_000..22_048], sample_rate);

        assert_all_finite(&output);
        assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
    }

    fn render_string_1d(
        sample_rate: f32,
        params: String1dParams,
        sample_count: usize,
        excitation: RenderExcitation,
    ) -> Vec<f32> {
        let mut string = String1d::new(sample_rate);
        render_response(
            sample_rate,
            params.frequency_hz,
            sample_count,
            excitation,
            |sample| string.process_sample(sample, params),
        )
    }
}
