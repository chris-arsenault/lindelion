use super::{TestExcitation, excitation_sample};
use crate::{StringBodyMode, StringModel, StringModelParams};
use lindelion_dsp_utils::{
    analysis::{assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation_refined},
    math::cents_between,
};

#[test]
fn tension_drive_sharpens_pitch_and_settles() {
    let sample_rate = 48_000.0;
    let params = tension_test_params();
    let estimate = |samples: &[f32]| {
        estimate_f0_autocorrelation_refined(samples, sample_rate, 180.0, 260.0).unwrap()
    };

    let nominal = render_string_with_drive(sample_rate, params, 24_000, |_| 0.0);
    let nominal_f0 = estimate(&nominal[2_000..10_000]);
    assert!(
        cents_between(220.0, nominal_f0).abs() < 20.0,
        "nominal_f0={nominal_f0}"
    );

    let low = render_string_with_drive(sample_rate, params, 24_000, |_| 0.005);
    let high = render_string_with_drive(sample_rate, params, 24_000, |_| 0.012);
    let low_f0 = estimate(&low[2_000..10_000]);
    let high_f0 = estimate(&high[2_000..10_000]);
    assert!(high_f0 > low_f0 + 1.0, "low_f0={low_f0} high_f0={high_f0}");

    let bloom = render_string_with_drive(sample_rate, params, 24_000, |index| {
        (0.012 * (1.0 - index as f32 / 6_000.0)).max(0.0)
    });
    let early_f0 = estimate(&bloom[512..3_072]);
    let late_f0 = estimate(&bloom[8_000..12_000]);
    assert!(
        early_f0 > late_f0 + 1.0,
        "early_f0={early_f0} late_f0={late_f0}"
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn tension_stays_finite_and_bounded_under_extreme_drive() {
    let sample_rate = 48_000.0;
    for &(frequency_hz, cutoff, loop_gain) in &[
        (55.0, 2_000.0, 0.99),
        (220.0, 14_000.0, 0.999),
        (1_000.0, 8_000.0, 0.95),
    ] {
        let params = StringModelParams {
            frequency_hz,
            loop_filter_cutoff_hz: cutoff,
            loop_filter_resonance: 0.3,
            loop_gain,
            loop_nonlinearity: 0.5,
            dispersion: 0.5,
            strike_position: 0.3,
            pickup_position: 0.6,
            ..StringModelParams::default()
        };
        let output =
            render_string_with_drive(sample_rate, params, 24_000, |index| match index % 7 {
                0 => 1_000.0,
                1 => f32::NAN,
                2 => f32::INFINITY,
                3 => -5.0,
                _ => (index as f32 * 0.013).sin() * 50.0,
            });

        assert_all_finite(&output);
        assert!(
            audio_window_metrics(&output, sample_rate).peak_abs < 8.0,
            "frequency_hz={frequency_hz} peak too high"
        );
    }
}

fn tension_test_params() -> StringModelParams {
    StringModelParams {
        frequency_hz: 220.0,
        loop_filter_cutoff_hz: 14_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.995,
        dispersion: 0.0,
        strike_position: 0.3,
        pickup_position: 0.6,
        body_mode: StringBodyMode::Disabled,
        ..StringModelParams::default()
    }
}

fn render_string_with_drive(
    sample_rate: f32,
    params: StringModelParams,
    sample_count: usize,
    drive: impl Fn(usize) -> f32,
) -> Vec<f32> {
    let mut string = StringModel::new(sample_rate);
    (0..sample_count)
        .map(|index| {
            string.set_tension_drive(drive(index));
            let input = excitation_sample(
                TestExcitation::ShapedPluck,
                index,
                sample_rate,
                params.frequency_hz,
            );
            string.process(input, params)
        })
        .collect()
}
