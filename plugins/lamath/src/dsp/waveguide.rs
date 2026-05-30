use serde::{Deserialize, Serialize};

use super::constants::{
    FILTER_RESONANCE, STRIKE_POSITION, WAVEGUIDE_DISPERSION, WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ,
    WAVEGUIDE_LOOP_GAIN, WAVEGUIDE_PICKUP_POSITION,
};

mod body;
mod core;
mod dispersion;
mod mesh_2d;
mod string_1d;
mod traveling;
mod tube_1d;

pub use mesh_2d::{MeshResonator, MeshVoiceParams};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveguideStyle {
    #[default]
    String,
    Tube,
}

impl WaveguideStyle {
    pub const ALL: [Self; 2] = [Self::String, Self::Tube];
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveguideParams {
    pub style: WaveguideStyle,
    pub frequency_hz: f32,
    pub loop_filter_cutoff: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub dispersion: f32,
    pub position_of_strike: f32,
    pub pickup_position: f32,
    pub boundary_reflection: f32,
}

fn default_pickup_position() -> f32 {
    WAVEGUIDE_PICKUP_POSITION.default
}

impl Default for WaveguideParams {
    fn default() -> Self {
        Self {
            style: WaveguideStyle::String,
            frequency_hz: 220.0,
            loop_filter_cutoff: WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ.default,
            loop_filter_resonance: FILTER_RESONANCE.default,
            loop_gain: WAVEGUIDE_LOOP_GAIN.default,
            loop_nonlinearity: 0.0,
            dispersion: WAVEGUIDE_DISPERSION.default,
            position_of_strike: STRIKE_POSITION.default,
            pickup_position: default_pickup_position(),
            boundary_reflection: crate::dsp::constants::TUBE_BOUNDARY.reflection.default,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveguideResonator {
    string: string_1d::String1d,
    tube: tube_1d::Tube1d,
}

impl WaveguideResonator {
    pub fn new(sample_rate: f32, lowest_frequency_hz: f32) -> Self {
        Self {
            string: string_1d::String1d::new(sample_rate),
            tube: tube_1d::Tube1d::new(sample_rate, lowest_frequency_hz),
        }
    }

    pub fn reset(&mut self) {
        self.string.reset();
        self.tube.reset();
    }

    pub fn process_sample(&mut self, excitation: f32, params: WaveguideParams) -> f32 {
        match params.style {
            WaveguideStyle::Tube => self.tube.process_sample(excitation, params),
            WaveguideStyle::String => self.string.process(excitation, params),
        }
    }
}

#[cfg(test)]
mod measurement_tests;
#[cfg(test)]
mod position_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{
        assert_all_finite, estimate_f0_autocorrelation, first_index_above_abs, peak_abs, rms,
        rms_difference,
    };

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
            estimate_f0_autocorrelation(&output[500..], sample_rate, 200.0, 700.0).unwrap();
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
            estimate_f0_autocorrelation(&output[1_000..], sample_rate, 180.0, 420.0).unwrap();
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
    fn string_dispersion_changes_render_without_losing_fundamental_tuning() {
        let sample_rate = 48_000.0;
        let target = 220.0;
        let base = WaveguideParams {
            frequency_hz: target,
            loop_gain: 0.99,
            loop_filter_cutoff: 18_000.0,
            loop_filter_resonance: 0.05,
            ..waveguide_material_change_params()
        };
        let dispersed_params = WaveguideParams {
            dispersion: 0.9,
            ..base
        };
        let natural = render_impulse(sample_rate, base, 18_000);
        let dispersed = render_impulse(sample_rate, dispersed_params, 18_000);

        assert_all_finite(&natural);
        assert_all_finite(&dispersed);
        assert!(rms_difference(&natural[512..], &dispersed[512..]) > 0.000_001);
        let estimate = estimate_f0_autocorrelation(
            &dispersed[1_024..],
            sample_rate,
            target * 0.8,
            target * 1.25,
        )
        .unwrap();
        assert!((estimate - target).abs() < 12.0, "estimate={estimate}");
    }

    #[test]
    fn waveguide_recovers_from_non_finite_excitation_and_params() {
        let mut waveguide = WaveguideResonator::new(48_000.0, 20.0);
        let params = WaveguideParams {
            frequency_hz: f32::NAN,
            loop_filter_cutoff: f32::NAN,
            loop_filter_resonance: f32::NAN,
            loop_gain: f32::NAN,
            loop_nonlinearity: f32::NAN,
            dispersion: f32::NAN,
            position_of_strike: f32::NAN,
            boundary_reflection: f32::NAN,
            ..WaveguideParams::default()
        };

        for input in [f32::NAN, f32::INFINITY, 0.5, 0.0] {
            assert!(waveguide.process_sample(input, params).is_finite());
        }
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
        // The corrected quarter-wave bore circulates less energy and renders at a
        // lower absolute level than before, so this floor is set below it while
        // still asserting a material boundary-dependent difference.
        assert!(rms_difference(&closed[512..], &open[512..]) > 0.000_001);
    }

    #[test]
    fn strike_position_moves_excitation_injection_point() {
        let sample_rate = 48_000.0;
        let high_position = render_impulse(
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
        let low_position = render_impulse(
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

        let high_position_onset = first_index_above_abs(&high_position, 0.000_1).unwrap();
        let low_position_onset = first_index_above_abs(&low_position, 0.000_1).unwrap();

        assert!(
            low_position_onset + 20 < high_position_onset,
            "low_position_onset={low_position_onset}, high_position_onset={high_position_onset}"
        );
    }

    /// Emits docs/plots/data/waveguide_impulse.csv for the WaveguideResonator doc.
    #[test]
    fn export_waveguide_impulse_csv() {
        use std::fs::{File, create_dir_all};
        use std::io::Write;
        use std::path::PathBuf;

        let sample_rate = 48_000.0_f32;
        let mut waveguide = WaveguideResonator::new(sample_rate, 30.0);
        let params = WaveguideParams {
            style: WaveguideStyle::String,
            frequency_hz: 240.0,
            loop_filter_cutoff: 12_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.95,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
            ..WaveguideParams::default()
        };

        let n_samples = 8_192_usize;
        let mut output = Vec::with_capacity(n_samples);
        output.push(waveguide.process_sample(1.0, params));
        for _ in 1..n_samples {
            output.push(waveguide.process_sample(0.0, params));
        }

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("plugins dir")
            .parent()
            .expect("workspace root")
            .join("docs")
            .join("plots")
            .join("data");
        create_dir_all(&dir).expect("create data dir");

        let mut file = File::create(dir.join("waveguide_impulse.csv")).expect("create csv");
        writeln!(file, "time_s,value").unwrap();
        for (i, v) in output.iter().enumerate() {
            let t = i as f32 / sample_rate;
            writeln!(file, "{:.6},{:.6}", t, v).unwrap();
        }
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
                dispersion: t,
                position_of_strike: 0.1 + 0.8 * t,
                pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
                boundary_reflection: -1.0 + t * 2.0,
            };
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        assert_all_finite(&output);
        // Unity-gain dispersion keeps the swept loop bounded well below the old 10.0
        // ceiling (measured peak ~0.19); a dispersion energy leak would breach this.
        assert!(
            peak_abs(&output) < 1.0,
            "sweep peak_abs={}",
            peak_abs(&output)
        );
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
}
