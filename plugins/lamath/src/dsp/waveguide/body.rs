use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math,
};

use super::{WaveguideParams, WaveguideStyle, core};
use crate::dsp::constants::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WaveguideBody {
    sample_rate: f32,
    highpass: Biquad,
    lowpass: Biquad,
    low_resonance: Biquad,
    high_resonance: Biquad,
}

impl WaveguideBody {
    pub(super) fn new(sample_rate: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            highpass: Biquad::new(BiquadCoefficients::identity()),
            lowpass: Biquad::new(BiquadCoefficients::identity()),
            low_resonance: Biquad::new(BiquadCoefficients::identity()),
            high_resonance: Biquad::new(BiquadCoefficients::identity()),
        }
    }

    pub(super) fn reset(&mut self) {
        self.highpass.reset();
        self.lowpass.reset();
        self.low_resonance.reset();
        self.high_resonance.reset();
    }

    pub(super) fn process_sample(&mut self, input: f32, params: WaveguideParams) -> f32 {
        let profile = BodyProfile::from_params(self.sample_rate, params);
        self.highpass.set_coefficients(profile.highpass);
        self.lowpass.set_coefficients(profile.lowpass);
        self.low_resonance.set_coefficients(profile.low_resonance);
        self.high_resonance.set_coefficients(profile.high_resonance);

        let input = math::snap_to_zero(input);
        let radiating_input = self.highpass.process(input);
        let direct = self.lowpass.process(radiating_input) * profile.direct_gain;
        let low_body = self.low_resonance.process(radiating_input) * profile.low_resonance_gain;
        let high_body = self.high_resonance.process(radiating_input) * profile.high_resonance_gain;

        math::snap_to_zero((direct + low_body + high_body) * profile.output_gain)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyProfile {
    highpass: BiquadCoefficients,
    lowpass: BiquadCoefficients,
    low_resonance: BiquadCoefficients,
    high_resonance: BiquadCoefficients,
    direct_gain: f32,
    low_resonance_gain: f32,
    high_resonance_gain: f32,
    output_gain: f32,
}

impl BodyProfile {
    fn from_params(sample_rate: f32, params: WaveguideParams) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let frequency_hz = math::finite_clamp(params.frequency_hz, 20.0, sample_rate * 0.45, 220.0);
        let loop_cutoff =
            math::finite_clamp(params.loop_filter_cutoff, 20.0, sample_rate * 0.45, 8_000.0);

        match params.style {
            WaveguideStyle::String => {
                let low_body_hz =
                    math::finite_clamp(150.0 + frequency_hz * 0.22, 110.0, 460.0, 180.0);
                let high_body_hz =
                    math::finite_clamp(760.0 + frequency_hz * 0.5, 480.0, 2_600.0, 900.0);
                let radiation_cutoff =
                    math::finite_clamp(loop_cutoff * 1.2, 2_800.0, sample_rate * 0.45, 9_000.0);

                Self {
                    highpass: BiquadCoefficients::highpass(sample_rate, 28.0, DEFAULT_BIQUAD_Q),
                    lowpass: BiquadCoefficients::lowpass(
                        sample_rate,
                        radiation_cutoff,
                        DEFAULT_BIQUAD_Q,
                    ),
                    low_resonance: BiquadCoefficients::bandpass(sample_rate, low_body_hz, 1.1),
                    high_resonance: BiquadCoefficients::bandpass(sample_rate, high_body_hz, 1.4),
                    direct_gain: 0.82,
                    low_resonance_gain: 0.18,
                    high_resonance_gain: 0.08,
                    output_gain: 0.92,
                }
            }
            WaveguideStyle::Tube => {
                let low_body_hz = math::finite_clamp(frequency_hz * 2.0, 120.0, 1_400.0, 440.0);
                let high_body_hz = math::finite_clamp(frequency_hz * 5.0, 500.0, 3_800.0, 1_400.0);
                let radiation_cutoff =
                    math::finite_clamp(loop_cutoff * 1.4, 2_200.0, sample_rate * 0.45, 10_000.0);

                Self {
                    highpass: BiquadCoefficients::highpass(sample_rate, 45.0, DEFAULT_BIQUAD_Q),
                    lowpass: BiquadCoefficients::lowpass(
                        sample_rate,
                        radiation_cutoff,
                        DEFAULT_BIQUAD_Q,
                    ),
                    low_resonance: BiquadCoefficients::bandpass(sample_rate, low_body_hz, 1.25),
                    high_resonance: BiquadCoefficients::bandpass(sample_rate, high_body_hz, 1.7),
                    direct_gain: 0.72,
                    low_resonance_gain: 0.14,
                    high_resonance_gain: 0.12,
                    output_gain: TUBE_BOUNDARY.output_gain(params.boundary_reflection),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{assert_all_finite, audio_window_metrics, rms_difference};

    #[test]
    fn body_renders_finite_decaying_impulse() {
        let output = render_body(WaveguideParams::default(), 2_048);

        assert_all_finite(&output);
        assert!(audio_window_metrics(&output[0..512], 48_000.0).rms > 0.000_001);
        assert!(audio_window_metrics(&output[1_536..], 48_000.0).peak_abs < 0.01);
    }

    #[test]
    fn body_profiles_make_string_and_tube_spectrally_distinct() {
        let string = render_body(
            WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: 220.0,
                loop_filter_cutoff: 12_000.0,
                ..WaveguideParams::default()
            },
            4_096,
        );
        let tube = render_body(
            WaveguideParams {
                style: WaveguideStyle::Tube,
                frequency_hz: 220.0,
                loop_filter_cutoff: 12_000.0,
                boundary_reflection: 0.8,
                ..WaveguideParams::default()
            },
            4_096,
        );
        let string_centroid = audio_window_metrics(&string[0..2_048], 48_000.0)
            .spectral_centroid_hz
            .unwrap();
        let tube_centroid = audio_window_metrics(&tube[0..2_048], 48_000.0)
            .spectral_centroid_hz
            .unwrap();

        assert_all_finite(&string);
        assert_all_finite(&tube);
        assert!(rms_difference(&string, &tube) > 0.000_001);
        assert!((string_centroid - tube_centroid).abs() > 20.0);
    }

    #[test]
    fn body_reset_clears_filter_state() {
        let mut body = WaveguideBody::new(48_000.0);
        for index in 0..512 {
            body.process_sample((index == 0) as u8 as f32, WaveguideParams::default());
        }
        body.reset();

        let output = (0..256)
            .map(|_| body.process_sample(0.0, WaveguideParams::default()))
            .collect::<Vec<_>>();

        assert_all_finite(&output);
        assert!(audio_window_metrics(&output, 48_000.0).peak_abs < 0.000_001);
    }

    fn render_body(params: WaveguideParams, sample_count: usize) -> Vec<f32> {
        let mut body = WaveguideBody::new(48_000.0);
        let mut output = Vec::with_capacity(sample_count);
        for index in 0..sample_count {
            output.push(body.process_sample((index == 0) as u8 as f32, params));
        }
        output
    }
}
