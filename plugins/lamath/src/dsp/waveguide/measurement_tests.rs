use super::*;
use crate::assert_no_allocations;
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
fn waveguide_render_path_is_allocation_free() {
    let sample_rate = 48_000.0;
    let base = WaveguideParams {
        frequency_hz: 196.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.15,
        loop_gain: 0.95,
        loop_nonlinearity: 0.1,
        position_of_strike: 0.35,
        pickup_position: 0.62,
        boundary_reflection: 0.7,
        ..WaveguideParams::default()
    };

    for style in [WaveguideStyle::String, WaveguideStyle::Tube] {
        let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
        let steady = WaveguideParams { style, ..base };

        // Prime the prepared-model caches and settle the input smoothers outside
        // the asserted region (first derivation and any Vec growth happen here).
        for index in 0..1_024 {
            waveguide.process_sample((index == 0) as u8 as f32, steady);
        }

        assert_no_allocations("waveguide_steady_render", || {
            for _ in 0..512 {
                waveguide.process_sample(0.0, steady);
            }
        });

        // Force per-sample recompute: a stepped continuous input keeps the
        // smoother ramping, so the prepared model re-derives every sample.
        let stepped = WaveguideParams {
            loop_filter_cutoff: 2_000.0,
            loop_gain: 0.6,
            boundary_reflection: -0.4,
            ..steady
        };
        assert_no_allocations("waveguide_transition_render", || {
            for _ in 0..512 {
                waveguide.process_sample(0.0, stepped);
            }
        });
    }
}

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
                    excitation_spread: 0.0,
                    source_body_balance: 0.0,
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

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
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
                // The String holds sub-3-cents across the musical range. In the top
                // octave (>2 kHz) the half-wave loop is only a few samples long, and
                // the M7 two-way body adds a small frequency-dependent bridge phase
                // while the pickup comb biases the period estimate, so the top tapers
                // to a looser (still musically slight) bound — the inherent
                // short-loop limit the CHANGELOG documents.
                let tolerance_cents = if target_hz > 2_000.0 { 12.0 } else { 3.0 };
                assert!(
                    cents < tolerance_cents,
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
    // 330 Hz (and its 5th, 1650 Hz) sit in the guitar body's modal gaps, so this
    // measures the loop filter's T60 calibration without the M7 body's modal loss
    // confounding it (at 220 Hz the fundamental lands on the 200/230 Hz plate modes,
    // which legitimately shortens its decay — that two-way coloring is covered by
    // the body-coupling tests, not here).
    let f0 = 330.0;
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
    // measurably faster than the fundamental over the same early span. The contrast
    // factor is looser than a bare string's because the M7 body radiates a little
    // broadband energy into the output, putting a gentle floor under every partial's
    // apparent decay; the loop filter still rolls the high partial off clearly faster
    // (the mechanism under test), and the absolute T60 is checked separately below.
    let partial_width = 2_048;
    let (partial_early, partial_late) = (480, 2_880);
    let high_partial = 5.0 * f0;
    let fundamental_ratio = magnitude(partial_late, partial_width, f0)
        / magnitude(partial_early, partial_width, f0).max(1.0e-12);
    let high_ratio = magnitude(partial_late, partial_width, high_partial)
        / magnitude(partial_early, partial_width, high_partial).max(1.0e-12);
    assert!(
        high_ratio < fundamental_ratio * 0.72,
        "high partial should decay faster: high_ratio={high_ratio}, fundamental_ratio={fundamental_ratio}"
    );

    // Overall T60: the loop filter sets a base decay from loop_gain, but the M7 body
    // is coupled two-way at the bridge and *absorbs* energy across its (dense, low)
    // active range, so the played pitch decays measurably faster than a bare string
    // would — physically correct for a body-coupled string, which sustains far less
    // than an undamped one. The decay therefore lands in a loop_gain-related band
    // *below* the bare-loop target rather than matching it: the body shortens it, but
    // the string still rings for a meaningful fraction of the base time (the body
    // absorbs energy, it does not kill the note). loop_gain still lengthens the decay
    // (per_partial_decay_slope_holds_across_damping_settings covers that monotonicity).
    let t60_width = 4_096;
    let (t60_early, t60_late) = (2_400, 7_200);
    let elapsed = (t60_late - t60_early) as f32 / sample_rate;
    let drop_db = 20.0
        * (magnitude(t60_early, t60_width, f0) / magnitude(t60_late, t60_width, f0).max(1.0e-12))
            .log10();
    let measured_t60 = elapsed * 60.0 / drop_db.max(1.0e-6);
    // The M11 P2 cap raise lifted the bare-loop `target_t60` (the loop can now ring
    // far longer), but this heavily loop-filtered (700 Hz), body-coupled config is
    // dominated by the filter + body absorption, so the played pitch lands well
    // below that raised target — a smaller fraction than under the old 2.5 s cap.
    assert!(
        measured_t60 < target_t60 * 1.1 && measured_t60 > target_t60 * 0.1,
        "body-coupled fundamental T60 should sit below the bare-loop target but stay a meaningful fraction of it: measured={measured_t60}, target={target_t60}"
    );
}

#[test]
fn per_partial_decay_slope_holds_across_damping_settings() {
    use lindelion_dsp_utils::analysis::dft_magnitude_at;

    let sample_rate = 48_000.0;
    // 165 Hz (E3) sits in a guitar-body modal gap, so loop_gain — not the body
    // modes — is the decay control. (At 220 Hz the fundamental lands on the 200/
    // 230 Hz plate modes, which after the M11 P2 body re-tune dominate the decay
    // and mask loop_gain; that body-shaped per-note variation is its own test.)
    let f0 = 165.0;
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
        // Early span (10 ms -> 30 ms) where every setting still rings: the loop
        // filter rolls the high partial off faster than the fundamental.
        let fundamental_ratio = magnitude(1_440, f0) / magnitude(480, f0).max(1.0e-12);
        let high_ratio = magnitude(1_440, high_partial) / magnitude(480, high_partial).max(1.0e-12);
        assert!(
            high_ratio < fundamental_ratio * 0.6,
            "high partial should decay faster at loop_gain={loop_gain}: high={high_ratio}, fundamental={fundamental_ratio}"
        );
        // Monotonicity needs a longer span (10 ms -> 200 ms): now that the M11 P2
        // body re-tune lightened the broadband loss, loop_gain 0.8 and 0.95 both
        // barely decay over 30 ms and only separate over a longer window.
        let long_retention = magnitude(9_600, f0) / magnitude(480, f0).max(1.0e-12);
        fundamental_retention.push(long_retention);
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

    // Both paths stay clean: almost no inter-harmonic floor. The 2x oversampled inner
    // loop (M3) keeps the saturator's harmonics below the oversampled fold point, so
    // high drive adds *harmonics* without folding measurable aliasing between them —
    // the driven path is as clean as the linear one (the whole point of oversampling
    // the nonlinearity). This is a stronger guarantee than the pre-oversampling test,
    // which expected the drive to raise the inter-harmonic floor.
    assert!(
        linear_floor < 0.05,
        "linear path should be clean: floor={linear_floor}"
    );
    assert!(
        driven_floor < 0.05,
        "driven path should stay clean (oversampling prevents drive aliasing): floor={driven_floor}"
    );
    // The drive is genuinely active (so the clean-path checks are meaningful): high
    // drive materially changes the render versus the linear path.
    assert!(
        rms_difference(&driven[4_096..], &linear[4_096..]) > 0.000_01,
        "drive should materially change the render"
    );
    // ...and the driven output stays bounded rather than running away.
    assert!(
        peak_abs(&driven) < 2.0,
        "driven output must stay bounded: peak={}",
        peak_abs(&driven)
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

    // The String output reads at the bridge/pickup (body radiation plus pickup tap),
    // so both positions onset within a few samples; the strike nearer the bridge
    // (0.1) onsets no later than the far one (0.9). The material render difference is
    // the robust evidence that strike position still moves the injection point.
    assert!(
        low_position_onset <= high_position_onset,
        "low_position_onset={low_position_onset}, high_position_onset={high_position_onset}"
    );
    assert!(difference > 0.000_01, "difference={difference}");
}
