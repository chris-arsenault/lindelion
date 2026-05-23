use ahara_dsp_utils::{
    delay::{DelayLine, FirstOrderAllpass},
    filters::{Biquad, BiquadCoefficients},
    math, soft_saturate,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveguideStyle {
    #[default]
    String,
    Tube,
}

impl WaveguideStyle {
    pub const ALL: [Self; 2] = [Self::String, Self::Tube];

    pub fn from_plain(value: f32) -> Self {
        let index = if value.is_finite() {
            value.round().clamp(0.0, (Self::ALL.len() - 1) as f32) as usize
        } else {
            0
        };
        Self::ALL[index]
    }

    pub const fn plain(self) -> f32 {
        match self {
            Self::String => 0.0,
            Self::Tube => 1.0,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Tube => "Tube",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveguideParams {
    pub style: WaveguideStyle,
    pub frequency_hz: f32,
    pub loop_filter_cutoff: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub position_of_strike: f32,
    pub boundary_reflection: f32,
}

impl Default for WaveguideParams {
    fn default() -> Self {
        Self {
            style: WaveguideStyle::String,
            frequency_hz: 220.0,
            loop_filter_cutoff: 8_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.92,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
            boundary_reflection: 0.75,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveguideResonator {
    sample_rate: f32,
    delay: DelayLine,
    fractional_delay: FirstOrderAllpass,
    loop_filter: Biquad,
}

impl WaveguideResonator {
    pub fn new(sample_rate: f32, lowest_frequency_hz: f32) -> Self {
        let max_delay = (sample_rate / lowest_frequency_hz.max(1.0)).ceil() as usize + 8;
        Self {
            sample_rate,
            delay: DelayLine::new(max_delay),
            fractional_delay: FirstOrderAllpass::default(),
            loop_filter: Biquad::new(BiquadCoefficients::lowpass(sample_rate, 8_000.0, 0.707)),
        }
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.fractional_delay.reset();
        self.loop_filter.reset();
    }

    pub fn process_sample(&mut self, excitation: f32, params: WaveguideParams) -> f32 {
        let frequency_hz = params.frequency_hz.clamp(
            self.sample_rate / self.delay.capacity() as f32,
            self.sample_rate * 0.45,
        );
        let delay_samples =
            (self.sample_rate / frequency_hz - 1.0).clamp(1.0, self.delay.capacity() as f32 - 3.0);
        let integer_delay = delay_samples.floor();
        let fractional_delay = delay_samples - integer_delay;
        self.fractional_delay.set_fractional_delay(fractional_delay);
        self.loop_filter.set_coefficients(loop_filter_coefficients(
            self.sample_rate,
            params.loop_filter_cutoff,
            params.loop_filter_resonance,
        ));

        let delay_tap = self.delay.read(integer_delay);
        let delayed = self.fractional_delay.process(delay_tap);
        let damped = self.loop_filter.process(delayed);
        let nonlinear = if params.loop_nonlinearity > 0.0 {
            soft_saturate(damped, params.loop_nonlinearity)
        } else {
            damped
        };

        let resonance_gain_compensation =
            1.0 / (1.0 + params.loop_filter_resonance.clamp(0.0, 0.999) * 0.45);
        let feedback = feedback_sample(nonlinear, params, resonance_gain_compensation);
        self.delay.push(math::snap_to_zero(feedback));
        self.delay.add_at(
            injection_delay_samples(integer_delay, params.position_of_strike),
            math::snap_to_zero(excitation_sample(excitation, params)),
        );

        math::snap_to_zero(output_sample(delayed, params))
    }
}

fn feedback_sample(
    loop_sample: f32,
    params: WaveguideParams,
    resonance_gain_compensation: f32,
) -> f32 {
    let loop_gain = params.loop_gain.clamp(0.0, 0.999) * resonance_gain_compensation;
    match params.style {
        WaveguideStyle::String => loop_sample * loop_gain,
        WaveguideStyle::Tube => {
            let reflection = math::finite_clamp(params.boundary_reflection, -1.0, 1.0, 0.75);
            loop_sample * loop_gain * reflection
        }
    }
}

fn excitation_sample(excitation: f32, params: WaveguideParams) -> f32 {
    match params.style {
        WaveguideStyle::String => excitation,
        WaveguideStyle::Tube => {
            let reflection = math::finite_clamp(params.boundary_reflection, -1.0, 1.0, 0.75);
            let boundary_loss = 1.0 - reflection.abs() * 0.25;
            excitation * boundary_loss.clamp(0.5, 1.0)
        }
    }
}

fn output_sample(delayed: f32, params: WaveguideParams) -> f32 {
    match params.style {
        WaveguideStyle::String => delayed,
        WaveguideStyle::Tube => {
            let reflection = math::finite_clamp(params.boundary_reflection, -1.0, 1.0, 0.75);
            delayed * (0.8 + reflection.abs() * 0.2)
        }
    }
}

fn loop_filter_coefficients(
    sample_rate: f32,
    loop_filter_cutoff: f32,
    loop_filter_resonance: f32,
) -> BiquadCoefficients {
    let resonance = math::finite_clamp(loop_filter_resonance, 0.0, 0.999, 0.0);
    let q = 0.55 + resonance * 4.0;
    BiquadCoefficients::lowpass(sample_rate, loop_filter_cutoff, q)
}

fn injection_delay_samples(loop_delay_samples: f32, position_of_strike: f32) -> f32 {
    let position = math::finite_clamp(position_of_strike, 0.001, 0.999, 0.5);
    (loop_delay_samples * position).clamp(0.0, loop_delay_samples.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahara_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};

    #[test]
    fn impulse_produces_decaying_output() {
        let mut waveguide = WaveguideResonator::new(48_000.0, 20.0);
        let params = WaveguideParams {
            frequency_hz: 240.0,
            loop_filter_cutoff: 12_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.95,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
            ..WaveguideParams::default()
        };
        let mut output = Vec::new();

        for index in 0..8_000 {
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        assert_all_finite(&output);
        assert!(rms(&output[500..2_000]) > rms(&output[6_000..]));
    }

    #[test]
    fn impulse_frequency_tracks_delay_length() {
        let sample_rate = 48_000.0;
        let target = 440.0;
        let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
        let params = WaveguideParams {
            frequency_hz: target,
            loop_filter_cutoff: 20_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.99,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
            ..WaveguideParams::default()
        };
        let mut output = Vec::new();

        for index in 0..10_000 {
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        let estimate =
            estimate_frequency_autocorrelation(&output[500..], sample_rate, 200.0, 700.0).unwrap();
        assert!((estimate - target).abs() < 12.0, "estimate={estimate}");
    }

    #[test]
    fn non_integer_delay_tracks_fractional_frequency() {
        let sample_rate = 48_000.0;
        let target = 277.18;
        let output = render_impulse(
            sample_rate,
            WaveguideParams {
                frequency_hz: target,
                loop_filter_cutoff: 18_000.0,
                loop_filter_resonance: 0.1,
                loop_gain: 0.985,
                loop_nonlinearity: 0.0,
                position_of_strike: 0.5,
                ..WaveguideParams::default()
            },
            18_000,
        );

        let estimate =
            estimate_frequency_autocorrelation(&output[1_000..], sample_rate, 180.0, 420.0)
                .unwrap();
        assert!((estimate - target).abs() < 6.0, "estimate={estimate}");
    }

    #[test]
    fn loop_filter_resonance_materially_changes_render() {
        let sample_rate = 48_000.0;
        let base = waveguide_material_change_params();
        let dry = render_impulse(sample_rate, base, 12_000);
        let resonant = render_impulse(
            sample_rate,
            WaveguideParams {
                loop_filter_resonance: 0.9,
                ..base
            },
            12_000,
        );

        assert_all_finite(&dry);
        assert_all_finite(&resonant);
        assert!(rms_difference(&dry[512..], &resonant[512..]) > 0.000_01);
    }

    #[test]
    fn loop_filter_cutoff_materially_changes_render() {
        let sample_rate = 48_000.0;
        let base = waveguide_material_change_params();
        let damped = render_impulse(
            sample_rate,
            WaveguideParams {
                loop_filter_cutoff: 650.0,
                ..base
            },
            12_000,
        );
        let open = render_impulse(
            sample_rate,
            WaveguideParams {
                loop_filter_cutoff: 14_000.0,
                ..base
            },
            12_000,
        );

        assert_all_finite(&damped);
        assert_all_finite(&open);
        assert!(rms_difference(&damped[512..], &open[512..]) > 0.000_01);
    }

    #[test]
    fn loop_gain_materially_changes_decay() {
        let sample_rate = 48_000.0;
        let base = waveguide_material_change_params();
        let short = render_impulse(
            sample_rate,
            WaveguideParams {
                loop_gain: 0.55,
                ..base
            },
            12_000,
        );
        let long = render_impulse(
            sample_rate,
            WaveguideParams {
                loop_gain: 0.985,
                ..base
            },
            12_000,
        );
        let short_tail = rms(&short[6_000..]);
        let long_tail = rms(&long[6_000..]);

        assert_all_finite(&short);
        assert_all_finite(&long);
        assert!(
            long_tail > short_tail * 3.0,
            "short_tail={short_tail}, long_tail={long_tail}"
        );
    }

    #[test]
    fn loop_nonlinearity_materially_changes_render() {
        let sample_rate = 48_000.0;
        let base = WaveguideParams {
            loop_gain: 0.99,
            loop_filter_cutoff: 18_000.0,
            loop_filter_resonance: 0.25,
            ..waveguide_material_change_params()
        };
        let linear = render_impulse(sample_rate, base, 12_000);
        let driven = render_impulse(
            sample_rate,
            WaveguideParams {
                loop_nonlinearity: 1.0,
                ..base
            },
            12_000,
        );

        assert_all_finite(&linear);
        assert_all_finite(&driven);
        assert!(rms_difference(&linear[512..], &driven[512..]) > 0.000_001);
    }

    #[test]
    fn tube_style_materially_changes_render() {
        let sample_rate = 48_000.0;
        let base = WaveguideParams {
            loop_filter_cutoff: 3_200.0,
            loop_filter_resonance: 0.35,
            loop_gain: 0.985,
            boundary_reflection: 0.85,
            ..waveguide_material_change_params()
        };
        let string = render_impulse(sample_rate, base, 12_000);
        let tube = render_impulse(
            sample_rate,
            WaveguideParams {
                style: WaveguideStyle::Tube,
                ..base
            },
            12_000,
        );

        assert_all_finite(&string);
        assert_all_finite(&tube);
        assert!(rms_difference(&string[512..], &tube[512..]) > 0.000_01);
    }

    #[test]
    fn tube_boundary_reflection_materially_changes_render() {
        let sample_rate = 48_000.0;
        let base = WaveguideParams {
            style: WaveguideStyle::Tube,
            loop_filter_cutoff: 4_000.0,
            loop_filter_resonance: 0.2,
            loop_gain: 0.98,
            ..waveguide_material_change_params()
        };
        let closed = render_impulse(
            sample_rate,
            WaveguideParams {
                boundary_reflection: -0.85,
                ..base
            },
            12_000,
        );
        let open = render_impulse(
            sample_rate,
            WaveguideParams {
                boundary_reflection: 0.85,
                ..base
            },
            12_000,
        );

        assert_all_finite(&closed);
        assert_all_finite(&open);
        assert!(rms_difference(&closed[512..], &open[512..]) > 0.000_01);
    }

    #[test]
    fn strike_position_moves_excitation_injection_point() {
        let sample_rate = 48_000.0;
        let near_output = render_impulse(
            sample_rate,
            WaveguideParams {
                frequency_hz: 240.0,
                loop_filter_cutoff: 12_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.94,
                loop_nonlinearity: 0.0,
                position_of_strike: 0.9,
                ..WaveguideParams::default()
            },
            2_000,
        );
        let near_input = render_impulse(
            sample_rate,
            WaveguideParams {
                position_of_strike: 0.1,
                ..WaveguideParams {
                    frequency_hz: 240.0,
                    loop_filter_cutoff: 12_000.0,
                    loop_filter_resonance: 0.0,
                    loop_gain: 0.94,
                    loop_nonlinearity: 0.0,
                    position_of_strike: 0.9,
                    ..WaveguideParams::default()
                }
            },
            2_000,
        );

        let near_output_onset = first_above(&near_output, 0.000_1).unwrap();
        let near_input_onset = first_above(&near_input, 0.000_1).unwrap();

        assert!(
            near_output_onset + 80 < near_input_onset,
            "near_output_onset={near_output_onset}, near_input_onset={near_input_onset}"
        );
    }

    #[test]
    fn stable_across_parameter_sweep() {
        let sample_rate = 48_000.0;
        let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
        let mut output = Vec::new();

        for index in 0..20_000 {
            let t = index as f32 / 19_999.0;
            let params = WaveguideParams {
                style: if index % 2 == 0 {
                    WaveguideStyle::String
                } else {
                    WaveguideStyle::Tube
                },
                frequency_hz: 40.0 + t * 4_000.0,
                loop_filter_cutoff: 200.0 + t * 18_000.0,
                loop_filter_resonance: t * 0.95,
                loop_gain: 0.2 + t * 0.799,
                loop_nonlinearity: t,
                position_of_strike: 0.1 + 0.8 * t,
                boundary_reflection: -1.0 + t * 2.0,
            };
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        assert_all_finite(&output);
        assert!(peak_abs(&output) < 10.0);
    }

    fn render_impulse(sample_rate: f32, params: WaveguideParams, sample_count: usize) -> Vec<f32> {
        let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
        let mut output = Vec::with_capacity(sample_count);

        for index in 0..sample_count {
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        output
    }

    fn waveguide_material_change_params() -> WaveguideParams {
        WaveguideParams {
            frequency_hz: 180.0,
            loop_filter_cutoff: 1_700.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.965,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.45,
            ..WaveguideParams::default()
        }
    }

    fn rms_difference(left: &[f32], right: &[f32]) -> f32 {
        let len = left.len().min(right.len());
        let mut sum = 0.0;
        for (left, right) in left.iter().zip(right).take(len) {
            sum += (left - right).powi(2);
        }
        (sum / len as f32).sqrt()
    }

    fn first_above(samples: &[f32], threshold: f32) -> Option<usize> {
        samples.iter().position(|sample| sample.abs() > threshold)
    }

    fn estimate_frequency_autocorrelation(
        samples: &[f32],
        sample_rate: f32,
        min_hz: f32,
        max_hz: f32,
    ) -> Option<f32> {
        let min_lag = (sample_rate / max_hz).floor().max(1.0) as usize;
        let max_lag = (sample_rate / min_hz).ceil() as usize;
        let max_lag = max_lag.min(samples.len().saturating_sub(1));
        let mut best_lag = 0;
        let mut best_score = f32::NEG_INFINITY;

        for lag in min_lag..=max_lag {
            let mut score = 0.0;
            for index in 0..samples.len() - lag {
                score += samples[index] * samples[index + lag];
            }
            if score > best_score {
                best_score = score;
                best_lag = lag;
            }
        }

        (best_lag > 0).then(|| sample_rate / best_lag as f32)
    }
}
