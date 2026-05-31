use super::*;
use crate::dsp::{
    constants::WAVEGUIDE_PICKUP_POSITION,
    render_metrics::{RenderExcitation, render_response},
};
use lindelion_dsp_utils::analysis::{
    assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation,
    estimate_f0_autocorrelation_refined, rms_difference,
};
use lindelion_dsp_utils::math::cents_between;

#[test]
fn string_1d_renders_finite_decaying_audio() {
    let sample_rate = 48_000.0;
    let output = render_string_1d(
        sample_rate,
        String1dParams {
            frequency_hz: 220.0,
            loop_filter_cutoff: 6_000.0,
            loop_filter_resonance: 0.1,
            loop_gain: 0.82,
            loop_nonlinearity: 0.0,
            dispersion: 0.0,
            strike_position: 0.34,
            pickup_position: 0.78,
        },
        24_000,
        RenderExcitation::ShapedPluck,
    );
    let early = audio_window_metrics(&output[512..2_560], sample_rate);
    let late = audio_window_metrics(&output[12_000..14_048], sample_rate);

    assert_all_finite(&output);
    assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
    assert!(early.peak_abs < 4.0);
}

#[test]
fn string_1d_pitch_tracks_target_matrix() {
    for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
        for target_hz in [110.0, 220.0, 440.0, 880.0] {
            let output = render_string_1d(
                sample_rate,
                String1dParams {
                    frequency_hz: target_hz,
                    loop_filter_cutoff: 18_000.0,
                    loop_filter_resonance: 0.0,
                    loop_gain: 0.99,
                    loop_nonlinearity: 0.0,
                    dispersion: 0.0,
                    strike_position: 0.37,
                    pickup_position: 0.73,
                },
                (sample_rate * 0.32) as usize,
                RenderExcitation::Impulse,
            );
            let estimate = estimate_f0_autocorrelation(
                &output[1_024..],
                sample_rate,
                target_hz * 0.8,
                target_hz * 1.25,
            )
            .unwrap();
            let cents = cents_between(target_hz, estimate);

            assert!(
                cents < 80.0,
                "sample_rate={sample_rate}, target_hz={target_hz}, estimate={estimate}, cents={cents}"
            );
        }
    }
}

#[test]
fn string_1d_strike_position_changes_response() {
    // The body now radiates the bridge force (M7 removed the pickup tap), but the
    // strike position still shapes which harmonics the string excites, so it
    // changes the bridge force the body radiates.
    let sample_rate = 48_000.0;
    let base = String1dParams::from_waveguide(WaveguideParams {
        frequency_hz: 240.0,
        loop_filter_cutoff: 12_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.96,
        loop_nonlinearity: 0.0,
        ..WaveguideParams::default()
    });
    let center_strike = render_string_1d(
        sample_rate,
        String1dParams {
            strike_position: 0.5,
            ..base
        },
        4_096,
        RenderExcitation::ShapedPluck,
    );
    let edge_strike = render_string_1d(
        sample_rate,
        String1dParams {
            strike_position: 0.12,
            ..base
        },
        4_096,
        RenderExcitation::ShapedPluck,
    );

    assert_all_finite(&center_strike);
    assert_all_finite(&edge_strike);
    let difference = rms_difference(&center_strike[256..], &edge_strike[256..]);
    assert!(difference > 0.000_01, "difference={difference}");
}

#[test]
fn string_1d_dispersion_materially_changes_render() {
    let sample_rate = 48_000.0;
    let base = String1dParams::from_waveguide(WaveguideParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 18_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.985,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.37,
        ..WaveguideParams::default()
    });
    let natural = render_string_1d(sample_rate, base, 16_000, RenderExcitation::ShapedPluck);
    let dispersed = render_string_1d(
        sample_rate,
        String1dParams {
            dispersion: 0.85,
            ..base
        },
        16_000,
        RenderExcitation::ShapedPluck,
    );

    assert_all_finite(&natural);
    assert_all_finite(&dispersed);
    assert!(rms_difference(&natural[512..], &dispersed[512..]) > 0.000_001);
}

#[test]
fn string_1d_reset_clears_state() {
    let sample_rate = 48_000.0;
    let params = String1dParams::from_waveguide(WaveguideParams {
        frequency_hz: 220.0,
        loop_gain: 0.98,
        ..WaveguideParams::default()
    });
    let mut string = String1d::new(sample_rate);
    let _ = render_response(
        sample_rate,
        params.frequency_hz,
        4_096,
        RenderExcitation::Impulse,
        |sample| string.process_sample(sample, params),
    );
    string.reset();

    let output = (0..512)
        .map(|_| string.process_sample(0.0, params))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(audio_window_metrics(&output, sample_rate).peak_abs < 0.000_001);
}

#[test]
fn string_1d_low_note_near_capacity_tracks_target() {
    // A low note near the buffer-capacity limit must still track its target;
    // this guards the shared frequency sanitizer used by both the delay length
    // and the filter-delay compensation.
    let sample_rate = 48_000.0;
    let target_hz = 35.0;
    let output = render_string_1d(
        sample_rate,
        String1dParams {
            frequency_hz: target_hz,
            loop_filter_cutoff: 16_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.99,
            loop_nonlinearity: 0.0,
            dispersion: 0.0,
            strike_position: 0.4,
            pickup_position: 0.7,
        },
        32_000,
        RenderExcitation::Impulse,
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
fn string_1d_resonant_loop_decays_without_high_frequency_growth() {
    // Regression for the loop filter being applied at both terminations: with a
    // low cutoff and a resonant loop, the round-trip peak gain exceeded 1 and
    // the partials grew over time. Applying the filter once per round trip keeps
    // the decay monotonic.
    let sample_rate = 48_000.0;
    let output = render_string_1d(
        sample_rate,
        String1dParams {
            frequency_hz: 220.0,
            loop_filter_cutoff: 2_400.0,
            loop_filter_resonance: 0.1,
            loop_gain: 0.975,
            loop_nonlinearity: 0.0,
            dispersion: 0.0,
            strike_position: 0.42,
            pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
        },
        24_000,
        RenderExcitation::ShapedPluck,
    );
    let early = audio_window_metrics(&output[512..2_560], sample_rate);
    let late = audio_window_metrics(&output[20_000..22_048], sample_rate);

    assert_all_finite(&output);
    assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
}

#[test]
fn string_model_derived_once_for_constant_params() {
    let mut string = String1d::new(48_000.0);
    let params = String1dParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.9,
        loop_nonlinearity: 0.0,
        dispersion: 0.2,
        strike_position: 0.35,
        pickup_position: 0.62,
    };

    for index in 0..2_048 {
        string.process_sample((index == 0) as u8 as f32, params);
    }

    assert_eq!(
        string.recompute_count, 1,
        "string operators recomputed per sample"
    );
}

#[test]
fn continuous_input_step_ramps_over_multiple_samples() {
    let mut string = String1d::new(48_000.0);
    let base = String1dParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.9,
        loop_nonlinearity: 0.0,
        dispersion: 0.0,
        strike_position: 0.35,
        pickup_position: 0.62,
    };

    // First sample primes the smoother converged to the base target.
    string.process_sample(0.0, base);
    assert_eq!(
        string.loop_gain.current(),
        0.9,
        "smoother should initialize converged to the first target"
    );

    // Step the loop gain down; a single sample must only inch toward it.
    let stepped = String1dParams {
        loop_gain: 0.5,
        ..base
    };
    string.process_sample(0.0, stepped);
    let after_one = string.loop_gain.current();
    assert!(
        after_one > 0.6 && after_one < 0.9,
        "single-sample step should ramp, not jump: {after_one}"
    );

    // After enough samples it snaps exactly onto the target.
    for _ in 0..8_000 {
        string.process_sample(0.0, stepped);
    }
    assert_eq!(string.loop_gain.current(), 0.5);
}

#[test]
fn string_model_recomputes_when_params_move() {
    let mut string = String1d::new(48_000.0);
    let base = String1dParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.9,
        loop_nonlinearity: 0.0,
        dispersion: 0.2,
        strike_position: 0.35,
        pickup_position: 0.62,
    };
    let moved = String1dParams {
        loop_gain: 0.7,
        ..base
    };

    for _ in 0..16 {
        string.process_sample(0.0, base);
    }
    let settled = string.recompute_count;
    // The move ramps the smoothed inputs, so the cache re-derives while it
    // glides; the point is that it invalidated at all (it did not stay warm
    // on a genuine change).
    for _ in 0..8_000 {
        string.process_sample(0.0, moved);
    }

    assert_eq!(settled, 1, "constant base params should derive once");
    assert!(
        string.recompute_count > settled,
        "moving params should invalidate the cache"
    );
}

#[test]
fn body_coupling_two_way_changes_partial_decay() {
    let sample_rate = 48_000.0;
    // Tune the string fundamental onto a strong guitar body resonance (the 100 Hz
    // air mode), where the two-way loading has the most effect.
    let params = String1dParams {
        frequency_hz: 100.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.99,
        loop_nonlinearity: 0.0,
        dispersion: 0.0,
        strike_position: 0.3,
        pickup_position: 0.6,
    };
    // Both renders radiate the body identically; only the loop loading differs.
    // scale 0.0: the body radiates but does not load the loop (output coloration
    // only, like a post-EQ). scale 1.0: the body loads the bridge reflection (the
    // two-way effect), so the on-resonance partial loses energy and decays faster.
    let render = |scale: f32| {
        let mut string = String1d::new(sample_rate);
        string.set_body_coupling_scale(scale);
        render_response(
            sample_rate,
            params.frequency_hz,
            16_000,
            RenderExcitation::ShapedPluck,
            |sample| string.process_sample(sample, params),
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
    // The body loading is the only difference, and it makes the on-resonance
    // partial decay measurably faster — the loop change a post-EQ cannot produce.
    assert!(
        loaded_ratio < unloaded_ratio * 0.85,
        "loaded did not decay faster: loaded={loaded_ratio} unloaded={unloaded_ratio}"
    );
}

#[test]
fn body_coupled_string_stays_finite_and_decays_across_extremes() {
    let sample_rate = 48_000.0;
    // The wave-digital bridge is passive (|R| <= 1), so the resonant body can never
    // ring the string loop up — even at high loop gain with the fundamental on a
    // body resonance.
    for &frequency_hz in &[100.0_f32, 200.0, 280.0, 440.0, 110.0] {
        for &loop_gain in &[0.985_f32, 0.999] {
            let params = String1dParams {
                frequency_hz,
                loop_filter_cutoff: 12_000.0,
                loop_filter_resonance: 0.2,
                loop_gain,
                loop_nonlinearity: 0.0,
                dispersion: 0.0,
                strike_position: 0.3,
                pickup_position: 0.6,
            };
            let output =
                render_string_1d(sample_rate, params, 24_000, RenderExcitation::ShapedPluck);
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
fn body_coupled_string_render_does_not_allocate() {
    use crate::assert_no_allocations;
    let params = String1dParams {
        frequency_hz: 200.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.97,
        loop_nonlinearity: 0.1,
        dispersion: 0.2,
        strike_position: 0.3,
        pickup_position: 0.6,
    };
    let mut string = String1d::new(48_000.0);
    // Prime the caches and the body modes outside the asserted region.
    for index in 0..1_024 {
        string.process_sample((index == 0) as u8 as f32, params);
    }
    assert_no_allocations("body_coupled_string_render", || {
        for index in 0..512 {
            string.set_tension_drive(0.1 + 0.05 * (index as f32 * 0.05).sin());
            string.process_sample(0.0, params);
        }
    });
}

fn render_string_with_drive(
    sample_rate: f32,
    params: String1dParams,
    sample_count: usize,
    drive: impl Fn(usize) -> f32,
) -> Vec<f32> {
    let mut string = String1d::new(sample_rate);
    let mut index = 0usize;
    render_response(
        sample_rate,
        params.frequency_hz,
        sample_count,
        RenderExcitation::ShapedPluck,
        |sample| {
            string.set_tension_drive(drive(index));
            index += 1;
            string.process_sample(sample, params)
        },
    )
}

#[test]
fn tension_drive_sharpens_pitch_and_settles() {
    let sample_rate = 48_000.0;
    let params = String1dParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 14_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.995,
        loop_nonlinearity: 0.0,
        dispersion: 0.0,
        strike_position: 0.3,
        pickup_position: 0.6,
    };
    let estimate = |samples: &[f32]| {
        estimate_f0_autocorrelation_refined(samples, sample_rate, 180.0, 260.0).unwrap()
    };

    // (c) Tuning preserved at zero drive (the two-way body now colors the pitch by
    // a few tens of cents near its resonances, so the tolerance reflects the body
    // coloration, not the clean string).
    let nominal = render_string_with_drive(sample_rate, params, 24_000, |_| 0.0);
    let nominal_f0 = estimate(&nominal[2_000..10_000]);
    assert!(
        cents_between(220.0, nominal_f0).abs() < 35.0,
        "nominal_f0={nominal_f0}"
    );

    // (a) Sharpening increases with drive (higher measured energy => higher pitch).
    let low = render_string_with_drive(sample_rate, params, 24_000, |_| 0.05);
    let high = render_string_with_drive(sample_rate, params, 24_000, |_| 0.15);
    let low_f0 = estimate(&low[2_000..10_000]);
    let high_f0 = estimate(&high[2_000..10_000]);
    assert!(high_f0 > low_f0 + 1.0, "low_f0={low_f0} high_f0={high_f0}");

    // (b) Decaying drive: the attack blooms sharp and settles back to nominal.
    let bloom = render_string_with_drive(sample_rate, params, 24_000, |index| {
        (0.15 * (1.0 - index as f32 / 6_000.0)).max(0.0)
    });
    // The two-way body adds loss, so the string decays faster; measure the settled
    // pitch where signal remains (after the drive envelope has returned to zero).
    let early_f0 = estimate(&bloom[512..3_072]);
    let late_f0 = estimate(&bloom[8_000..12_000]);
    assert!(
        early_f0 > late_f0 + 1.0,
        "early_f0={early_f0} late_f0={late_f0}"
    );
    assert!(
        cents_between(220.0, late_f0).abs() < 35.0,
        "late settled f0={late_f0}"
    );
}

#[test]
fn tension_stays_finite_and_bounded_under_extreme_drive() {
    let sample_rate = 48_000.0;
    for &(frequency_hz, loop_filter_cutoff, loop_gain) in &[
        (55.0, 2_000.0, 0.99),
        (220.0, 14_000.0, 0.999),
        (1_000.0, 8_000.0, 0.95),
    ] {
        let params = String1dParams {
            frequency_hz,
            loop_filter_cutoff,
            loop_filter_resonance: 0.3,
            loop_gain,
            loop_nonlinearity: 0.5,
            dispersion: 0.5,
            strike_position: 0.3,
            pickup_position: 0.6,
        };
        // Extreme, rapidly-changing, and non-finite drive: the bounded
        // `/(1+k*clamp(drive))` scheme and the input sanitizer must keep the
        // loop finite and within range regardless.
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

fn render_string_1d(
    sample_rate: f32,
    params: String1dParams,
    sample_count: usize,
    excitation: RenderExcitation,
) -> Vec<f32> {
    let mut string = String1d::new(sample_rate);
    render_response(
        sample_rate,
        params.frequency_hz,
        sample_count,
        excitation,
        |sample| string.process_sample(sample, params),
    )
}
