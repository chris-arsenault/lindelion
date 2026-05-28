use super::*;
use crate::dsp::{
    constants::WAVEGUIDE_PICKUP_POSITION,
    render_metrics::{RenderExcitation, render_metric_profile, render_waveguide_response},
};
use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation,
        first_index_above_abs, rms_difference,
    },
    math::cents_between,
};

#[test]
fn measurement_harness_covers_excitation_styles() {
    let sample_rate = 48_000.0;
    let mut case_count = 0;

    for style in [WaveguideStyle::String, WaveguideStyle::Tube] {
        for excitation in RenderExcitation::ALL {
            let output = render_waveguide_response(
                sample_rate,
                WaveguideParams {
                    style,
                    frequency_hz: 220.0,
                    loop_filter_cutoff: 8_000.0,
                    loop_filter_resonance: 0.2,
                    loop_gain: 0.965,
                    loop_nonlinearity: 0.15,
                    dispersion: 0.35,
                    position_of_strike: 0.38,
                    pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
                    boundary_reflection: 0.65,
                },
                12_000,
                excitation,
            );
            let metrics = audio_window_metrics(&output[512..2_560], sample_rate);

            assert_all_finite(&output);
            assert!(metrics.rms > 0.000_000_1, "{style:?} {excitation:?}");
            assert!(metrics.peak_abs < 4.0, "{style:?} {excitation:?}");
            assert!(metrics.dc_offset_abs() < metrics.peak_abs.max(0.000_001));
            assert!(metrics.spectral_centroid_hz.unwrap_or_default().is_finite());
            case_count += 1;
        }
    }

    assert_eq!(
        case_count,
        WaveguideStyle::ALL.len() * RenderExcitation::ALL.len()
    );
}

#[test]
fn measurement_harness_reports_pitch_error_matrix() {
    for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
        for target_hz in [110.0, 220.0, 440.0, 880.0] {
            let output = render_waveguide_response(
                sample_rate,
                WaveguideParams {
                    frequency_hz: target_hz,
                    loop_filter_cutoff: 18_000.0,
                    loop_filter_resonance: 0.0,
                    loop_gain: 0.99,
                    loop_nonlinearity: 0.0,
                    position_of_strike: 0.5,
                    ..WaveguideParams::default()
                },
                (sample_rate * 0.32) as usize,
                RenderExcitation::Impulse,
            );

            assert_all_finite(&output);
            let estimate = estimate_f0_autocorrelation(
                &output[1_024..],
                sample_rate,
                target_hz * 0.8,
                target_hz * 1.25,
            )
            .unwrap();
            let cents = cents_between(target_hz, estimate);
            assert!(
                cents < 150.0,
                "sample_rate={sample_rate}, target_hz={target_hz}, estimate={estimate}, cents={cents}"
            );
        }
    }
}

#[test]
fn measurement_harness_reports_decay_centroid_and_partials() {
    let sample_rate = 48_000.0;
    let params = WaveguideParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 2_400.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.975,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.42,
        ..WaveguideParams::default()
    };
    let output =
        render_waveguide_response(sample_rate, params, 24_000, RenderExcitation::ShapedPluck);
    let profile = render_metric_profile(&output, sample_rate, params.frequency_hz);

    assert_all_finite(&output);
    assert!(profile.early.rms > profile.late.rms, "profile={profile:?}");
    assert!(profile.early.spectral_centroid_hz.is_some());
    assert!(profile.late.spectral_centroid_hz.is_some());
    assert!(profile.harmonic_decay.len() >= 4);
    assert!(profile.harmonic_decay.iter().all(|partial| {
        partial.early_magnitude.is_finite()
            && partial.late_magnitude.is_finite()
            && partial.late_to_early_ratio.is_finite()
    }));
    assert!(
        profile
            .harmonic_decay
            .iter()
            .any(|partial| partial.early_magnitude > 0.000_000_1)
    );
}

#[test]
fn measurement_harness_reports_position_timing_difference() {
    let sample_rate = 48_000.0;
    let high_position = render_waveguide_response(
        sample_rate,
        WaveguideParams {
            frequency_hz: 240.0,
            loop_filter_cutoff: 12_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.94,
            position_of_strike: 0.9,
            ..WaveguideParams::default()
        },
        4_096,
        RenderExcitation::ShapedPluck,
    );
    let low_position = render_waveguide_response(
        sample_rate,
        WaveguideParams {
            position_of_strike: 0.1,
            ..WaveguideParams {
                frequency_hz: 240.0,
                loop_filter_cutoff: 12_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.94,
                position_of_strike: 0.9,
                ..WaveguideParams::default()
            }
        },
        4_096,
        RenderExcitation::ShapedPluck,
    );

    let high_position_onset = first_index_above_abs(&high_position, 0.000_1).unwrap();
    let low_position_onset = first_index_above_abs(&low_position, 0.000_1).unwrap();
    let difference = rms_difference(&high_position[256..], &low_position[256..]);

    assert!(
        low_position_onset + 20 < high_position_onset,
        "low_position_onset={low_position_onset}, high_position_onset={high_position_onset}"
    );
    assert!(difference > 0.000_01, "difference={difference}");
}
