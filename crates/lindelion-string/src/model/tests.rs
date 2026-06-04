use super::*;
use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, audio_window_metrics, dft_magnitude_at, estimate_f0_autocorrelation,
        peak_abs, rms_difference,
    },
    math::cents_between,
};

mod tension;

#[derive(Debug, Clone, Copy)]
pub(super) enum TestExcitation {
    Impulse,
    ShapedPluck,
}

#[test]
fn string_model_renders_finite_audible_impulse() {
    let output = render_string_model(
        48_000.0,
        StringModelParams::default(),
        4_800,
        TestExcitation::Impulse,
    );

    assert_all_finite(&output);
    assert!(peak_abs(&output) > 0.001, "peak={}", peak_abs(&output));
}

#[test]
fn disabled_body_remains_finite() {
    let output = render_string_model(
        48_000.0,
        StringModelParams {
            body_mode: StringBodyMode::Disabled,
            ..StringModelParams::default()
        },
        2_400,
        TestExcitation::Impulse,
    );

    assert_all_finite(&output);
}

#[test]
fn string_model_renders_finite_decaying_audio() {
    let sample_rate = 48_000.0;
    let output = render_string_model(
        sample_rate,
        StringModelParams {
            frequency_hz: 220.0,
            loop_filter_cutoff_hz: 6_000.0,
            loop_filter_resonance: 0.1,
            loop_gain: 0.82,
            dispersion: 0.0,
            strike_position: 0.34,
            pickup_position: 0.78,
            ..StringModelParams::default()
        },
        24_000,
        TestExcitation::ShapedPluck,
    );
    let early = audio_window_metrics(&output[512..2_560], sample_rate);
    let late = audio_window_metrics(&output[12_000..14_048], sample_rate);

    assert_all_finite(&output);
    assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
    assert!(early.peak_abs < 4.0);
}

#[test]
fn string_model_pitch_tracks_target_matrix() {
    for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
        for target_hz in [110.0, 220.0, 440.0, 880.0] {
            let output = render_string_model(
                sample_rate,
                StringModelParams {
                    frequency_hz: target_hz,
                    loop_filter_cutoff_hz: 18_000.0,
                    loop_filter_resonance: 0.0,
                    loop_gain: 0.99,
                    dispersion: 0.0,
                    strike_position: 0.37,
                    pickup_position: 0.73,
                    body_mode: StringBodyMode::Disabled,
                    ..StringModelParams::default()
                },
                (sample_rate * 0.32) as usize,
                TestExcitation::Impulse,
            );
            let estimate = estimate_f0_autocorrelation(
                &output[1_024..],
                sample_rate,
                target_hz * 0.8,
                target_hz * 1.25,
            )
            .unwrap_or_else(|| panic!("no pitch estimate for {target_hz} Hz at {sample_rate} Hz"));
            let cents = cents_between(target_hz, estimate);

            assert!(
                cents < 80.0,
                "sample_rate={sample_rate}, target_hz={target_hz}, estimate={estimate}, cents={cents}"
            );
        }
    }
}

#[test]
fn strike_position_changes_response() {
    let sample_rate = 48_000.0;
    let base = StringModelParams {
        frequency_hz: 240.0,
        loop_filter_cutoff_hz: 12_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.96,
        dispersion: 0.0,
        ..StringModelParams::default()
    };
    let center = render_string_model(
        sample_rate,
        StringModelParams {
            strike_position: 0.5,
            ..base
        },
        4_096,
        TestExcitation::ShapedPluck,
    );
    let edge = render_string_model(
        sample_rate,
        StringModelParams {
            strike_position: 0.12,
            ..base
        },
        4_096,
        TestExcitation::ShapedPluck,
    );

    assert_all_finite(&center);
    assert_all_finite(&edge);
    let difference = rms_difference(&center[256..], &edge[256..]);
    assert!(difference > 0.000_01, "difference={difference}");
}

#[test]
fn dispersion_materially_changes_render() {
    let sample_rate = 48_000.0;
    let base = StringModelParams {
        frequency_hz: 220.0,
        loop_filter_cutoff_hz: 18_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.985,
        dispersion: 0.0,
        strike_position: 0.37,
        ..StringModelParams::default()
    };
    let natural = render_string_model(sample_rate, base, 16_000, TestExcitation::ShapedPluck);
    let dispersed = render_string_model(
        sample_rate,
        StringModelParams {
            dispersion: 0.85,
            ..base
        },
        16_000,
        TestExcitation::ShapedPluck,
    );

    assert_all_finite(&natural);
    assert_all_finite(&dispersed);
    assert!(rms_difference(&natural[512..], &dispersed[512..]) > 0.000_001);
}

#[test]
fn reset_clears_state() {
    let sample_rate = 48_000.0;
    let params = StringModelParams {
        frequency_hz: 220.0,
        loop_gain: 0.98,
        ..StringModelParams::default()
    };
    let mut string = StringModel::new(sample_rate);
    for index in 0..4_096 {
        let excitation = excitation_sample(
            TestExcitation::Impulse,
            index,
            sample_rate,
            params.frequency_hz,
        );
        string.process(excitation, params);
    }
    string.reset();

    let output = (0..512)
        .map(|_| string.process(0.0, params))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(audio_window_metrics(&output, sample_rate).peak_abs < 0.000_001);
}

#[test]
fn low_note_near_capacity_tracks_target() {
    let sample_rate = 48_000.0;
    let target_hz = 35.0;
    let output = render_string_model(
        sample_rate,
        StringModelParams {
            frequency_hz: target_hz,
            loop_filter_cutoff_hz: 16_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.99,
            dispersion: 0.0,
            strike_position: 0.4,
            pickup_position: 0.7,
            body_mode: StringBodyMode::Disabled,
            ..StringModelParams::default()
        },
        32_000,
        TestExcitation::Impulse,
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
fn resonant_loop_decays_without_high_frequency_growth() {
    let sample_rate = 48_000.0;
    let output = render_string_model(
        sample_rate,
        StringModelParams {
            frequency_hz: 220.0,
            loop_filter_cutoff_hz: 2_400.0,
            loop_filter_resonance: 0.1,
            loop_gain: 0.975,
            dispersion: 0.0,
            strike_position: 0.42,
            pickup_position: 0.82,
            ..StringModelParams::default()
        },
        24_000,
        TestExcitation::ShapedPluck,
    );
    let early = audio_window_metrics(&output[512..2_560], sample_rate);
    let late = audio_window_metrics(&output[20_000..22_048], sample_rate);

    assert_all_finite(&output);
    assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
}

#[test]
fn model_derived_once_for_constant_params() {
    let mut string = StringModel::new(48_000.0);
    let params = string_params(StringModelParams {
        frequency_hz: 220.0,
        loop_filter_cutoff_hz: 9_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.9,
        dispersion: 0.2,
        strike_position: 0.35,
        pickup_position: 0.62,
        ..StringModelParams::default()
    });
    let model_params = StringModelParams::default();

    for index in 0..2_048 {
        string.process_sample((index == 0) as u8 as f32, params, model_params);
    }

    assert_eq!(
        string.recompute_count, 1,
        "string operators recomputed per sample"
    );
}

#[test]
fn model_recomputes_when_params_move() {
    let mut string = StringModel::new(48_000.0);
    let base = string_params(StringModelParams {
        frequency_hz: 220.0,
        loop_filter_cutoff_hz: 9_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.9,
        dispersion: 0.2,
        strike_position: 0.35,
        pickup_position: 0.62,
        ..StringModelParams::default()
    });
    let moved = String1dParams {
        loop_gain: 0.7,
        ..base
    };
    let model_params = StringModelParams::default();

    for _ in 0..16 {
        string.process_sample(0.0, base, model_params);
    }
    let settled = string.recompute_count;
    for _ in 0..8_000 {
        string.process_sample(0.0, moved, model_params);
    }

    assert_eq!(settled, 1, "constant base params should derive once");
    assert!(
        string.recompute_count > settled,
        "moving params should invalidate the cache"
    );
}

#[test]
fn continuous_input_step_ramps_over_multiple_samples() {
    let mut smoother = core::ScalarSmoother::new(48_000.0);

    assert_eq!(smoother.next(0.9), 0.9);
    let after_one = smoother.next(0.5);

    assert!(
        after_one > 0.6 && after_one < 0.9,
        "single-sample step should ramp, not jump: {after_one}"
    );
    for _ in 0..8_000 {
        smoother.next(0.5);
    }
    assert_eq!(smoother.next(0.5), 0.5);
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn body_coupling_two_way_changes_partial_decay() {
    let sample_rate = 48_000.0;
    let params = StringModelParams {
        frequency_hz: 100.0,
        loop_filter_cutoff_hz: 9_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.99,
        dispersion: 0.0,
        strike_position: 0.3,
        pickup_position: 0.6,
        body_mode: StringBodyMode::Guitar,
        ..StringModelParams::default()
    };
    let render = |scale: f32| {
        let mut string = StringModel::new(sample_rate);
        string.set_body_coupling_scale(scale);
        render_string_with_model(
            &mut string,
            sample_rate,
            params,
            16_000,
            TestExcitation::ShapedPluck,
        )
    };
    let unloaded = render(0.0);
    let loaded = render(1.0);

    let decay_ratio = |output: &[f32]| {
        let early = audio_window_metrics(&output[1_000..4_000], sample_rate).rms;
        let late = audio_window_metrics(&output[10_000..14_000], sample_rate).rms;
        late / early.max(1.0e-9)
    };
    let loaded_ratio = decay_ratio(&loaded);
    let unloaded_ratio = decay_ratio(&unloaded);

    assert_all_finite(&loaded);
    assert_all_finite(&unloaded);
    assert!(
        loaded_ratio < unloaded_ratio * 0.85,
        "loaded did not decay faster: loaded={loaded_ratio} unloaded={unloaded_ratio}"
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn body_coupled_string_stays_finite_and_decays_across_extremes() {
    let sample_rate = 48_000.0;
    for &frequency_hz in &[100.0_f32, 200.0, 280.0, 440.0, 110.0] {
        for &loop_gain in &[0.985_f32, 0.999] {
            let output = render_string_model(
                sample_rate,
                StringModelParams {
                    frequency_hz,
                    loop_filter_cutoff_hz: 12_000.0,
                    loop_filter_resonance: 0.2,
                    loop_gain,
                    dispersion: 0.0,
                    strike_position: 0.3,
                    pickup_position: 0.6,
                    body_mode: StringBodyMode::Guitar,
                    ..StringModelParams::default()
                },
                24_000,
                TestExcitation::ShapedPluck,
            );
            assert_all_finite(&output);
            let early = audio_window_metrics(&output[512..2_560], sample_rate).rms;
            let late = audio_window_metrics(&output[20_000..22_048], sample_rate).rms;
            assert!(
                late < early,
                "frequency_hz={frequency_hz} loop_gain={loop_gain} rang up: early={early} late={late}"
            );
        }
    }
}

#[test]
fn frequency_dependent_damping_decays_high_partials_faster() {
    let sample_rate = 48_000.0;
    let f0 = 330.0;
    let output = render_string_model(
        sample_rate,
        StringModelParams {
            frequency_hz: f0,
            loop_filter_cutoff_hz: 700.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.8,
            dispersion: 0.0,
            body_mode: StringBodyMode::Disabled,
            ..StringModelParams::default()
        },
        28_800,
        TestExcitation::Impulse,
    );
    let magnitude = |start: usize, freq: f32| {
        dft_magnitude_at(&output[start..start + 2_048], sample_rate, freq)
    };
    let high_partial = 5.0 * f0;
    let fundamental_ratio = magnitude(2_880, f0) / magnitude(480, f0).max(1.0e-12);
    let high_ratio = magnitude(2_880, high_partial) / magnitude(480, high_partial).max(1.0e-12);

    assert_all_finite(&output);
    assert!(
        high_ratio < fundamental_ratio * 0.72,
        "high partial should decay faster: high_ratio={high_ratio}, fundamental_ratio={fundamental_ratio}"
    );
}

#[test]
fn loop_gain_audibly_controls_string_decay() {
    let sample_rate = 48_000.0;
    let f0 = 165.0;
    let mut retention = Vec::new();
    for loop_gain in [0.5, 0.8, 0.95] {
        let output = render_string_model(
            sample_rate,
            StringModelParams {
                frequency_hz: f0,
                loop_filter_cutoff_hz: 800.0,
                loop_filter_resonance: 0.0,
                loop_gain,
                dispersion: 0.0,
                body_mode: StringBodyMode::Disabled,
                ..StringModelParams::default()
            },
            48_000,
            TestExcitation::Impulse,
        );
        assert_all_finite(&output);
        let magnitude =
            |start: usize| dft_magnitude_at(&output[start..start + 2_048], sample_rate, f0);
        retention.push(magnitude(9_600) / magnitude(480).max(1.0e-12));
    }

    assert!(
        retention[0] < retention[1] && retention[1] < retention[2],
        "decay should lengthen with loop gain: {retention:?}"
    );
}

fn string_params(params: StringModelParams) -> String1dParams {
    params.string_params()
}

fn render_string_model(
    sample_rate: f32,
    params: StringModelParams,
    sample_count: usize,
    excitation: TestExcitation,
) -> Vec<f32> {
    let mut string = StringModel::new(sample_rate);
    render_string_with_model(&mut string, sample_rate, params, sample_count, excitation)
}

fn render_string_with_model(
    string: &mut StringModel,
    sample_rate: f32,
    params: StringModelParams,
    sample_count: usize,
    excitation: TestExcitation,
) -> Vec<f32> {
    (0..sample_count)
        .map(|index| {
            let input = excitation_sample(excitation, index, sample_rate, params.frequency_hz);
            string.process(input, params)
        })
        .collect()
}

pub(super) fn excitation_sample(
    excitation: TestExcitation,
    index: usize,
    sample_rate: f32,
    frequency_hz: f32,
) -> f32 {
    match excitation {
        TestExcitation::Impulse => (index == 0) as u8 as f32 * 0.8,
        TestExcitation::ShapedPluck => {
            let t = index as f32 / sample_rate;
            if t > 0.008 {
                return 0.0;
            }
            let envelope = (1.0 - t / 0.008).max(0.0);
            let phase = std::f32::consts::TAU * frequency_hz.max(20.0) * t;
            envelope * phase.sin() * 0.75
        }
    }
}
