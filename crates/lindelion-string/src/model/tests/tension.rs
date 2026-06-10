use super::{TestExcitation, excitation_sample};
use crate::{StringBodyMode, StringModel, StringModelParams, StringModelSwitches};
use lindelion_dsp_utils::analysis::{assert_all_finite, audio_window_metrics};

/// Render and return `(output, one_way_delay_samples per sample)` for a pluck
/// scaled by `scale`. Tension modulation is internal (driven by the string's
/// own stored energy), so the delay trace is the direct observation of it.
fn render_delay_trace(
    sample_rate: f32,
    params: StringModelParams,
    sample_count: usize,
    scale: f32,
) -> (Vec<f32>, Vec<f32>) {
    let mut string = StringModel::new(sample_rate);
    let mut output = Vec::with_capacity(sample_count);
    let mut delays = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        let input = scale
            * excitation_sample(
                TestExcitation::ShapedPluck,
                index,
                sample_rate,
                params.frequency_hz,
            );
        let (sample, probe) = string.process_with_bow_contact_probe(input, params, None);
        output.push(sample);
        delays.push(probe.one_way_delay_samples);
    }
    (output, delays)
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

fn min_in(window: &[f32]) -> f32 {
    window.iter().copied().fold(f32::INFINITY, f32::min)
}

#[test]
fn hard_pluck_sharpens_then_settles_and_soft_pluck_barely_moves() {
    let sample_rate = 48_000.0;
    let params = tension_test_params();

    let (hard_output, hard_delays) = render_delay_trace(sample_rate, params, 48_000, 1.0);
    let (_, soft_delays) = render_delay_trace(sample_rate, params, 48_000, 0.1);
    assert_all_finite(&hard_output);

    // The first sample carries the unmodulated tuning (no stored energy yet).
    let nominal = hard_delays[0];
    assert!(nominal > 10.0, "nominal delay implausible: {nominal}");

    let hard_dip = min_in(&hard_delays[..8_000]) / nominal;
    let soft_dip = min_in(&soft_delays[..8_000]) / nominal;
    let hard_late = min_in(&hard_delays[40_000..]) / nominal;

    // A full pluck should sharpen audibly (>= ~10 cents of delay shortening)
    // but stay inside the +40 cent design depth.
    assert!(
        hard_dip < 0.994,
        "hard pluck did not sharpen: dip ratio {hard_dip}"
    );
    assert!(
        hard_dip > 0.976,
        "hard pluck sharpened past the design depth: dip ratio {hard_dip}"
    );
    // A soft pluck stays essentially in tune.
    assert!(
        soft_dip > 0.999,
        "soft pluck should stay in tune: dip ratio {soft_dip}"
    );
    // The bloom settles back toward nominal as the note rings down.
    assert!(
        hard_late > hard_dip,
        "tension should relax as energy decays: dip={hard_dip} late={hard_late}"
    );
}

#[test]
fn tension_switch_off_keeps_the_delay_nominal() {
    let sample_rate = 48_000.0;
    let params = StringModelParams {
        switches: StringModelSwitches {
            tension_modulation_enabled: false,
            ..StringModelSwitches::default()
        },
        ..tension_test_params()
    };
    let (output, delays) = render_delay_trace(sample_rate, params, 12_000, 1.0);

    assert_all_finite(&output);
    let nominal = delays[0];
    assert!(
        delays.iter().all(|&delay| (delay - nominal).abs() < 1.0e-3),
        "delay moved with tension disabled"
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn tension_stays_finite_and_bounded_under_extreme_excitation() {
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
        let mut string = StringModel::new(sample_rate);
        let output: Vec<f32> = (0..24_000)
            .map(|index| {
                let excitation = match index % 7 {
                    0 => 1_000.0,
                    1 => f32::NAN,
                    2 => f32::INFINITY,
                    3 => -5.0,
                    _ => (index as f32 * 0.013).sin() * 50.0,
                };
                string.process(excitation, params)
            })
            .collect();

        assert_all_finite(&output);
        assert!(
            audio_window_metrics(&output, sample_rate).peak_abs < 600.0,
            "frequency_hz={frequency_hz} peak too high"
        );
    }
}
