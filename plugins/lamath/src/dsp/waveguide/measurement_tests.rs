use super::*;
use crate::dsp::{
    constants::WAVEGUIDE_PICKUP_POSITION,
    render_metrics::{RenderExcitation, render_metric_profile, render_waveguide_response},
};
use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation_refined,
        first_index_above_abs, peak_abs, rms, rms_difference,
    },
    math::cents_between,
};

#[test]
fn string_dispersion_loop_stays_bounded_and_decays() {
    // With unity-gain dispersion the feedback loop is bounded by loop_gain < 1, so a
    // single impulse rings down rather than accumulating energy, even at high loop
    // gain and maximum dispersion.
    let sample_rate = 48_000.0;
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: 220.0,
        loop_gain: 0.99,
        dispersion: 1.0,
        ..WaveguideParams::default()
    };
    let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
    let output = (0..48_000)
        .map(|index| waveguide.process_sample((index == 0) as u8 as f32, params))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(peak_abs(&output) < 2.0, "peak_abs={}", peak_abs(&output));
    let early = rms(&output[..4_800]);
    let late = rms(&output[43_200..]);
    assert!(late < early * 0.7, "early={early}, late={late}");
}

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
fn steady_state_tuning_within_three_cents_across_matrix() {
    let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
    // The half-wave String holds the requested pitch across the whole range. The
    // quarter-wave Tube does so through its tunable range, where its (shorter)
    // round trip is long enough that sample quantization stays sub-cent; above
    // that the top octave tapers off (an inherent limit of the short quarter-wave
    // loop, covered by tube_1d's matrix and full-range tests), so the Tube is
    // gated over its accurate range here.
    let string_frequencies = [30.0, 110.0, 440.0, 1_500.0, 4_000.0];
    let tube_frequencies = [30.0, 55.0, 110.0, 220.0];

    for sample_rate in sample_rates {
        for (style, frequencies) in [
            (WaveguideStyle::String, string_frequencies.as_slice()),
            (WaveguideStyle::Tube, tube_frequencies.as_slice()),
        ] {
            for &target_hz in frequencies {
                let output = render_waveguide_response(
                    sample_rate,
                    WaveguideParams {
                        style,
                        frequency_hz: target_hz,
                        loop_filter_cutoff: 18_000.0,
                        loop_filter_resonance: 0.0,
                        loop_gain: 0.992,
                        loop_nonlinearity: 0.0,
                        dispersion: 0.0,
                        boundary_reflection: 0.85,
                        ..WaveguideParams::default()
                    },
                    48_000,
                    RenderExcitation::Impulse,
                );

                assert_all_finite(&output);
                // Periodicity-faithful estimator: sub-cent on tones (Phase 1
                // validated) and robust to the bore's harmonic/body colouring,
                // over a sub-octave bracket so a period multiple cannot tie.
                let estimate = estimate_f0_autocorrelation_refined(
                    &output,
                    sample_rate,
                    target_hz * 0.75,
                    target_hz * 1.5,
                )
                .unwrap_or_else(|| {
                    panic!("no estimate: {style:?} {target_hz} Hz / {sample_rate} Hz")
                });
                let cents = cents_between(target_hz, estimate);
                assert!(
                    cents < 3.0,
                    "style={style:?} sample_rate={sample_rate} target_hz={target_hz} estimate={estimate} cents={cents}"
                );
            }
        }
    }
}

#[test]
fn frequency_dependent_damping_decays_high_partials_faster_and_matches_target_t60() {
    use lindelion_dsp_utils::analysis::dft_magnitude_at;

    let sample_rate = 48_000.0;
    let f0 = 220.0;
    let loop_gain = 0.8;
    // A natural, mellow string: the loop filter rolls the upper partials off
    // while still passing the fundamental.
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: f0,
        loop_filter_cutoff: 700.0,
        loop_filter_resonance: 0.0,
        loop_gain,
        loop_nonlinearity: 0.0,
        dispersion: 0.0,
        ..WaveguideParams::default()
    };
    let target_t60 = super::core::decay_seconds_from_loop_gain(loop_gain);
    let output = render_waveguide_response(sample_rate, params, 28_800, RenderExcitation::Impulse);
    assert_all_finite(&output);

    let magnitude = |start: usize, width: usize, freq: f32| {
        dft_magnitude_at(&output[start..start + width], sample_rate, freq)
    };

    // Per-partial decay slope: a high partial above the loop-filter cutoff decays
    // measurably faster than the fundamental over the same early span.
    let partial_width = 2_048;
    let (partial_early, partial_late) = (480, 2_880);
    let high_partial = 5.0 * f0;
    let fundamental_ratio = magnitude(partial_late, partial_width, f0)
        / magnitude(partial_early, partial_width, f0).max(1.0e-12);
    let high_ratio = magnitude(partial_late, partial_width, high_partial)
        / magnitude(partial_early, partial_width, high_partial).max(1.0e-12);
    assert!(
        high_ratio < fundamental_ratio * 0.6,
        "high partial should decay faster: high_ratio={high_ratio}, fundamental_ratio={fundamental_ratio}"
    );

    // Overall T60: the fundamental decays in the requested time, within tolerance,
    // measured over a clean early span where it stays well above the noise floor.
    let t60_width = 4_096;
    let (t60_early, t60_late) = (2_400, 7_200);
    let elapsed = (t60_late - t60_early) as f32 / sample_rate;
    let drop_db = 20.0
        * (magnitude(t60_early, t60_width, f0) / magnitude(t60_late, t60_width, f0).max(1.0e-12))
            .log10();
    let measured_t60 = elapsed * 60.0 / drop_db.max(1.0e-6);
    assert!(
        (measured_t60 - target_t60).abs() < target_t60 * 0.3,
        "fundamental T60 should match target within 30%: measured={measured_t60}, target={target_t60}"
    );
}

#[test]
fn per_partial_decay_slope_holds_across_damping_settings() {
    use lindelion_dsp_utils::analysis::dft_magnitude_at;

    let sample_rate = 48_000.0;
    let f0 = 220.0;
    let high_partial = 5.0 * f0;

    // Short / medium / long damping (loop gain sets the decay time).
    let mut fundamental_retention = Vec::new();
    for loop_gain in [0.5, 0.8, 0.95] {
        let params = WaveguideParams {
            style: WaveguideStyle::String,
            frequency_hz: f0,
            loop_filter_cutoff: 800.0,
            loop_filter_resonance: 0.0,
            loop_gain,
            loop_nonlinearity: 0.0,
            dispersion: 0.0,
            ..WaveguideParams::default()
        };
        let output =
            render_waveguide_response(sample_rate, params, 48_000, RenderExcitation::Impulse);
        assert_all_finite(&output);
        let magnitude = |start: usize, freq: f32| {
            dft_magnitude_at(&output[start..start + 2_048], sample_rate, freq)
        };
        // Early span (10 ms -> 30 ms) where every setting still rings.
        let fundamental_ratio = magnitude(1_440, f0) / magnitude(480, f0).max(1.0e-12);
        let high_ratio = magnitude(1_440, high_partial) / magnitude(480, high_partial).max(1.0e-12);
        assert!(
            high_ratio < fundamental_ratio * 0.6,
            "high partial should decay faster at loop_gain={loop_gain}: high={high_ratio}, fundamental={fundamental_ratio}"
        );
        fundamental_retention.push(fundamental_ratio);
    }

    // The settings really are short < medium < long: the fundamental retains more
    // energy over the same span as the damping lengthens.
    assert!(
        fundamental_retention[0] < fundamental_retention[1]
            && fundamental_retention[1] < fundamental_retention[2],
        "decay should lengthen with loop gain: {fundamental_retention:?}"
    );
}

#[test]
fn nonlinearity_aliasing_stays_bounded_and_linear_path_is_clean() {
    use lindelion_dsp_utils::analysis::{inter_peak_floor_ratio, peak_abs};

    let sample_rate = 48_000.0;
    // A non-SR-dividing fundamental with harmonics reaching toward Nyquist, so
    // nonlinear-drive aliasing folds to inharmonic frequencies between the
    // harmonic peaks where it is measurable (an alias-sensitive render).
    let f0 = 470.0;
    let peaks: Vec<f32> = (1..=12).map(|n| f0 * n as f32).collect();
    let render = |drive: f32| {
        render_waveguide_response(
            sample_rate,
            WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: f0,
                loop_filter_cutoff: 20_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.97,
                loop_nonlinearity: drive,
                dispersion: 0.0,
                position_of_strike: 0.5,
                ..WaveguideParams::default()
            },
            48_000,
            RenderExcitation::Impulse,
        )
    };

    let linear = render(0.0);
    let driven = render(0.95);
    assert_all_finite(&linear);
    assert_all_finite(&driven);

    let linear_floor = inter_peak_floor_ratio(&linear[4_096..], sample_rate, &peaks);
    let driven_floor = inter_peak_floor_ratio(&driven[4_096..], sample_rate, &peaks);

    // The default linear path stays clean: almost no inter-harmonic floor.
    assert!(
        linear_floor < 0.05,
        "linear path should be clean: floor={linear_floor}"
    );
    // High drive does add aliasing (so the clean-linear check is meaningful)...
    assert!(
        driven_floor > linear_floor,
        "drive should introduce aliasing: driven={driven_floor}, linear={linear_floor}"
    );
    // ...but it stays bounded: the output level and the alias floor remain finite
    // and below a ceiling rather than running away.
    assert!(
        peak_abs(&driven) < 2.0,
        "driven output must stay bounded: peak={}",
        peak_abs(&driven)
    );
    assert!(
        driven_floor < 3.0,
        "aliasing must stay bounded: floor={driven_floor}"
    );
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
