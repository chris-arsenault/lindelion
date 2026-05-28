use lindelion_dsp_utils::{delay::FirstOrderAllpass, math};

use super::{WaveguideParams, WaveguideStyle, core};
use crate::dsp::constants::WAVEGUIDE_DISPERSION;

const PRIMARY_COEFFICIENT_AT_MAX: f32 = -0.46;
const SECONDARY_COEFFICIENT_AT_MAX: f32 = -0.24;
const MAX_DELAY_COMPENSATION_SAMPLES: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DispersionProfile {
    primary_coefficient: f32,
    secondary_coefficient: f32,
    pub delay_compensation_samples: f32,
}

impl DispersionProfile {
    fn bypass() -> Self {
        Self {
            primary_coefficient: 0.0,
            secondary_coefficient: 0.0,
            delay_compensation_samples: 0.0,
        }
    }

    fn is_enabled(self) -> bool {
        self.primary_coefficient != 0.0 || self.secondary_coefficient != 0.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WaveguideDispersion {
    primary: FirstOrderAllpass,
    secondary: FirstOrderAllpass,
}

impl WaveguideDispersion {
    pub(super) fn new() -> Self {
        Self {
            primary: FirstOrderAllpass::default(),
            secondary: FirstOrderAllpass::default(),
        }
    }

    pub(super) fn reset(&mut self) {
        self.primary.reset();
        self.secondary.reset();
    }

    pub(super) fn process_sample(&mut self, input: f32, profile: DispersionProfile) -> f32 {
        let input = math::snap_to_zero(input);
        if !profile.is_enabled() {
            return input;
        }

        self.primary.set_coefficient(profile.primary_coefficient);
        self.secondary
            .set_coefficient(profile.secondary_coefficient);
        math::snap_to_zero(self.secondary.process(self.primary.process(input)))
    }
}

pub(super) fn dispersion_profile(sample_rate: f32, params: WaveguideParams) -> DispersionProfile {
    if params.style != WaveguideStyle::String {
        return DispersionProfile::bypass();
    }

    let amount = WAVEGUIDE_DISPERSION.clamp(params.dispersion);
    if amount <= f32::EPSILON {
        return DispersionProfile::bypass();
    }

    let sample_rate = core::sanitize_sample_rate(sample_rate);
    let primary_coefficient = PRIMARY_COEFFICIENT_AT_MAX * amount.powf(1.35);
    let secondary_coefficient = SECONDARY_COEFFICIENT_AT_MAX * amount.powf(1.8);
    let omega =
        std::f32::consts::TAU * tuned_frequency(sample_rate, params.frequency_hz) / sample_rate;
    let delay_compensation_samples = (first_order_group_delay_samples(primary_coefficient, omega)
        + first_order_group_delay_samples(secondary_coefficient, omega))
    .clamp(0.0, MAX_DELAY_COMPENSATION_SAMPLES);

    DispersionProfile {
        primary_coefficient,
        secondary_coefficient,
        delay_compensation_samples,
    }
}

fn tuned_frequency(sample_rate: f32, frequency_hz: f32) -> f32 {
    math::finite_clamp(frequency_hz, 1.0, sample_rate * 0.45, 220.0)
}

fn first_order_group_delay_samples(coefficient: f32, omega: f32) -> f32 {
    let coefficient = math::finite_clamp(coefficient, -0.5, 1.0, 0.0);
    let omega = math::finite_clamp(omega, 0.0, std::f32::consts::PI, 0.0);
    let denominator = 1.0 + coefficient * coefficient + 2.0 * coefficient * omega.cos();
    if denominator <= f32::EPSILON {
        return MAX_DELAY_COMPENSATION_SAMPLES;
    }

    math::finite_clamp(
        (1.0 - coefficient * coefficient) / denominator,
        0.0,
        MAX_DELAY_COMPENSATION_SAMPLES,
        0.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{assert_all_finite, rms_difference};

    #[test]
    fn default_and_tube_profiles_bypass_dispersion() {
        let default_profile = dispersion_profile(48_000.0, WaveguideParams::default());
        let tube_profile = dispersion_profile(
            48_000.0,
            WaveguideParams {
                style: WaveguideStyle::Tube,
                dispersion: 1.0,
                ..WaveguideParams::default()
            },
        );

        assert!(!default_profile.is_enabled());
        assert_eq!(default_profile.delay_compensation_samples, 0.0);
        assert!(!tube_profile.is_enabled());
        assert_eq!(tube_profile.delay_compensation_samples, 0.0);
    }

    #[test]
    fn string_profile_maps_stiffness_to_negative_allpass_sections() {
        let soft = dispersion_profile(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                dispersion: 0.25,
                ..WaveguideParams::default()
            },
        );
        let stiff = dispersion_profile(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                dispersion: 1.0,
                ..WaveguideParams::default()
            },
        );

        assert!(soft.is_enabled());
        assert!(stiff.primary_coefficient < soft.primary_coefficient);
        assert!(stiff.secondary_coefficient < soft.secondary_coefficient);
        assert!(stiff.delay_compensation_samples > soft.delay_compensation_samples);
    }

    #[test]
    fn dispersion_stage_is_finite_and_changes_impulse_shape() {
        let profile = dispersion_profile(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                dispersion: 0.8,
                ..WaveguideParams::default()
            },
        );
        let mut stage = WaveguideDispersion::new();
        let wet = render_stage(&mut stage, profile);
        let mut bypass = WaveguideDispersion::new();
        let dry = render_stage(&mut bypass, DispersionProfile::bypass());

        assert_all_finite(&wet);
        assert!(rms_difference(&wet, &dry) > 0.000_1);
    }

    #[test]
    fn dispersion_reset_clears_filter_state() {
        let profile = dispersion_profile(
            48_000.0,
            WaveguideParams {
                frequency_hz: 220.0,
                dispersion: 0.8,
                ..WaveguideParams::default()
            },
        );
        let mut stage = WaveguideDispersion::new();
        let _ = render_stage(&mut stage, profile);
        stage.reset();

        let output = (0..128)
            .map(|_| stage.process_sample(0.0, profile))
            .collect::<Vec<_>>();

        assert_all_finite(&output);
        assert!(output.iter().all(|sample| sample.abs() < 0.000_001));
    }

    fn render_stage(stage: &mut WaveguideDispersion, profile: DispersionProfile) -> Vec<f32> {
        (0..256)
            .map(|index| stage.process_sample((index == 0) as u8 as f32, profile))
            .collect()
    }
}
