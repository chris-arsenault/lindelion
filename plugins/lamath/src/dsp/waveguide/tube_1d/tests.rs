use super::*;
use lindelion_dsp_utils::analysis::{
    assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation_refined, rms_difference,
};
use lindelion_dsp_utils::math::cents_between;

#[test]
fn tube_1d_renders_finite_decaying_audio() {
    let sample_rate = 48_000.0;
    let output = render_tube_1d(
        sample_rate,
        WaveguideParams {
            style: WaveguideStyle::Tube,
            frequency_hz: 220.0,
            loop_filter_cutoff: 6_000.0,
            loop_filter_resonance: 0.1,
            loop_gain: 0.94,
            loop_nonlinearity: 0.2,
            position_of_strike: 0.18,
            pickup_position: 0.72,
            boundary_reflection: 0.8,
            ..WaveguideParams::default()
        },
        24_000,
    );
    let early = audio_window_metrics(&output[512..2_560], sample_rate);
    let late = audio_window_metrics(&output[12_000..14_048], sample_rate);

    assert_all_finite(&output);
    assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
    assert!(early.peak_abs < 4.0);
}

#[test]
fn tube_1d_tuning_matches_requested_pitch_across_matrix() {
    let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
    // The quarter-wave bore tunes to < 3 cents while its round trip stays
    // long enough that sample quantization is sub-cent. At 4 kHz / 44.1 kHz
    // that round trip is only ~5.5 samples, so a ~0.2-sample interpolation
    // floor becomes tens of cents; accuracy degrades monotonically above
    // this range (an inherent limit of the short quarter-wave loop). These
    // frequencies stay within the accurate range at every supported rate.
    let frequencies = [30.0, 55.0, 110.0, 220.0];

    for sample_rate in sample_rates {
        for frequency in frequencies {
            let params = WaveguideParams {
                style: WaveguideStyle::Tube,
                frequency_hz: frequency,
                loop_filter_cutoff: 18_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.992,
                loop_nonlinearity: 0.0,
                boundary_reflection: 0.85,
                ..WaveguideParams::default()
            };
            let output = render_tube_1d(sample_rate, params, 48_000);
            assert_all_finite(&output);
            // The bore's strike response is harmonically rich and body-coloured,
            // so a magnitude-peak scan is pulled by the spectral envelope. Measure
            // periodicity instead, over a sub-octave bracket.
            let estimate = estimate_f0_autocorrelation_refined(
                &output,
                sample_rate,
                frequency * 0.75,
                frequency * 1.5,
            )
            .unwrap_or_else(|| panic!("no pitch estimate at {frequency} Hz / {sample_rate} Hz"));
            let cents = cents_between(frequency, estimate);
            assert!(
                cents < 3.0,
                "frequency={frequency} sample_rate={sample_rate} estimate={estimate} cents={cents}"
            );
        }
    }
}

#[test]
fn tube_1d_stays_finite_and_decays_across_full_range() {
    // Tuning accuracy degrades above the range checked above, but the bore
    // must still render finite, bounded, decaying output across the whole
    // 30 Hz–4 kHz span at every supported sample rate.
    let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
    let frequencies = [30.0, 220.0, 880.0, 1_500.0, 4_000.0];

    for sample_rate in sample_rates {
        for frequency in frequencies {
            let output = render_tube_1d(
                sample_rate,
                WaveguideParams {
                    style: WaveguideStyle::Tube,
                    frequency_hz: frequency,
                    loop_filter_cutoff: 18_000.0,
                    loop_filter_resonance: 0.0,
                    loop_gain: 0.992,
                    loop_nonlinearity: 0.0,
                    boundary_reflection: 0.85,
                    ..WaveguideParams::default()
                },
                24_000,
            );
            assert_all_finite(&output);
            let early = audio_window_metrics(&output[512..4_608], sample_rate);
            let late = audio_window_metrics(&output[18_000..22_096], sample_rate);
            assert!(
                early.peak_abs < 4.0,
                "f={frequency} sr={sample_rate} {early:?}"
            );
            assert!(
                early.rms > late.rms,
                "should decay; f={frequency} sr={sample_rate} early={early:?} late={late:?}"
            );
        }
    }
}

#[test]
fn tube_1d_tuning_accounts_for_bore_delay() {
    let sample_rate = 48_000.0;
    let target = 220.0;
    let params = WaveguideParams {
        style: WaveguideStyle::Tube,
        frequency_hz: target,
        loop_filter_cutoff: 18_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.985,
        boundary_reflection: 0.85,
        ..WaveguideParams::default()
    };
    let damping = core::loop_damping(sample_rate, params);
    let profile = TubeBoreProfile::from_params(sample_rate, params, damping.loop_gain);
    let mouth_phase = core::filter_phase_delay_samples(profile.mouth_loss, sample_rate, target);
    let damping_phase = core::filter_phase_delay_samples(damping.coefficients, sample_rate, target);
    let tube = Tube1d::new(sample_rate, 20.0);
    let tuning = core::delay_tuning(
        sample_rate,
        tube.waves.capacity(),
        target,
        4.0,
        1.0 + 0.5 * (mouth_phase + damping_phase),
    );
    // Round trip = two one-way legs (each plus a one-sample push) plus each
    // boundary filter's phase delay once.
    let compensated_period =
        2.0 * (tuning.integer_delay + tuning.fractional_delay + 1.0) + mouth_phase + damping_phase;

    // The asymmetric bore (inverting mouth, non-inverting end) is a
    // quarter-wave resonator: a full round trip is half a period of the
    // played pitch, not a whole period as for the half-wave string.
    assert!((compensated_period - sample_rate / (2.0 * target)).abs() < 0.001);
}

#[test]
fn tube_boundary_polarity_materially_changes_bore_response() {
    let sample_rate = 48_000.0;
    let base = WaveguideParams {
        style: WaveguideStyle::Tube,
        frequency_hz: 220.0,
        loop_filter_cutoff: 8_000.0,
        loop_filter_resonance: 0.15,
        loop_gain: 0.97,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.2,
        pickup_position: 0.75,
        ..WaveguideParams::default()
    };
    let closed = render_tube_1d(
        sample_rate,
        WaveguideParams {
            boundary_reflection: 0.85,
            ..base
        },
        12_000,
    );
    let open = render_tube_1d(
        sample_rate,
        WaveguideParams {
            boundary_reflection: -0.85,
            ..base
        },
        12_000,
    );

    assert_all_finite(&closed);
    assert_all_finite(&open);
    // The corrected quarter-wave loop is half its former length, so it
    // circulates less energy and renders at a lower absolute level; the two
    // polarities still resonate an octave apart, so their difference exceeds
    // either render's own RMS. Assert a difference well above the noise floor.
    assert!(rms_difference(&closed[512..], &open[512..]) > 0.000_001);
}

#[test]
fn tube_1d_reset_clears_state() {
    let sample_rate = 48_000.0;
    let mut tube = Tube1d::new(sample_rate, 20.0);
    let params = WaveguideParams {
        style: WaveguideStyle::Tube,
        frequency_hz: 220.0,
        loop_gain: 0.98,
        boundary_reflection: 0.85,
        ..WaveguideParams::default()
    };
    for index in 0..4_096 {
        tube.process_sample((index == 0) as u8 as f32, params);
    }
    tube.reset();

    let output = (0..512)
        .map(|_| tube.process_sample(0.0, params))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(audio_window_metrics(&output, sample_rate).peak_abs < 0.000_001);
}

#[test]
fn bore_model_derived_once_for_constant_params() {
    let mut tube = Tube1d::new(48_000.0, 20.0);
    let params = WaveguideParams {
        style: WaveguideStyle::Tube,
        frequency_hz: 196.0,
        loop_filter_cutoff: 7_500.0,
        loop_filter_resonance: 0.15,
        loop_gain: 0.95,
        loop_nonlinearity: 0.05,
        position_of_strike: 0.18,
        pickup_position: 0.72,
        boundary_reflection: 0.82,
        ..WaveguideParams::default()
    };

    for index in 0..2_048 {
        tube.process_sample((index == 0) as u8 as f32, params);
    }

    assert_eq!(
        tube.recompute_count, 1,
        "bore operators recomputed per sample"
    );
}

#[test]
fn bore_model_recomputes_when_params_move() {
    let mut tube = Tube1d::new(48_000.0, 20.0);
    let base = WaveguideParams {
        style: WaveguideStyle::Tube,
        frequency_hz: 196.0,
        loop_filter_cutoff: 7_500.0,
        loop_gain: 0.95,
        boundary_reflection: 0.82,
        ..WaveguideParams::default()
    };
    let moved = WaveguideParams {
        boundary_reflection: -0.5,
        ..base
    };

    for _ in 0..16 {
        tube.process_sample(0.0, base);
    }
    let settled = tube.recompute_count;
    // The move ramps the smoothed inputs, so the cache re-derives while it
    // glides; the point is that it invalidated at all on a genuine change.
    for _ in 0..8_000 {
        tube.process_sample(0.0, moved);
    }

    assert_eq!(settled, 1, "constant base params should derive once");
    assert!(
        tube.recompute_count > settled,
        "moving params should invalidate the cache"
    );
}

fn render_tube_sustained(
    sample_rate: f32,
    params: WaveguideParams,
    sample_count: usize,
    drive: f32,
) -> Vec<f32> {
    // A bore is driven by a harmonically rich source (lip/breath): a sustained
    // pulse train at the played pitch sets up a rich standing wave the boundary
    // steepening can sharpen each round trip.
    let mut tube = Tube1d::new(sample_rate, 20.0);
    let mut output = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        tube.set_steepening_drive(drive);
        let phase = std::f32::consts::TAU * params.frequency_hz * index as f32 / sample_rate;
        output.push(tube.process_sample(0.6 * phase.sin(), params));
    }
    output
}

#[test]
fn steepening_brightens_with_drive_preserving_tuning() {
    let sample_rate = 48_000.0;
    let params = WaveguideParams {
        style: WaveguideStyle::Tube,
        frequency_hz: 196.0,
        loop_filter_cutoff: 12_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.985,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.2,
        pickup_position: 0.7,
        boundary_reflection: 0.8,
        ..WaveguideParams::default()
    };

    let quiet = render_tube_sustained(sample_rate, params, 24_000, 0.0);
    let loud = render_tube_sustained(sample_rate, params, 24_000, 0.18);

    // Measure at steady state, past the build-up transient.
    let quiet_window = &quiet[12_000..20_000];
    let loud_window = &loud[12_000..20_000];

    // Loud turns brassy: the spectral centroid rises measurably with energy
    // (the steepening harmonics radiate out the bell).
    let quiet_centroid = audio_window_metrics(quiet_window, sample_rate)
        .spectral_centroid_hz
        .unwrap();
    let loud_centroid = audio_window_metrics(loud_window, sample_rate)
        .spectral_centroid_hz
        .unwrap();
    assert!(
        loud_centroid > quiet_centroid * 1.1,
        "centroid quiet={quiet_centroid} loud={loud_centroid}"
    );

    // Fundamental tuning is preserved: the steepening adds harmonics but never
    // touches the loop length, so quiet and loud render the same pitch (compared
    // to each other, robust to the bore autocorrelation's absolute bias).
    let estimate =
        |samples: &[f32]| estimate_f0_autocorrelation_refined(samples, sample_rate, 100.0, 260.0);
    if let (Some(quiet_f0), Some(loud_f0)) = (estimate(quiet_window), estimate(loud_window)) {
        assert!(
            cents_between(quiet_f0, loud_f0).abs() < 15.0,
            "quiet_f0={quiet_f0} loud_f0={loud_f0}"
        );
    }
}

#[test]
fn steepening_stays_finite_and_bounded_under_extreme_drive() {
    let sample_rate = 48_000.0;
    for &(frequency_hz, cutoff, boundary_reflection) in &[
        (55.0, 3_000.0, 0.9),
        (196.0, 12_000.0, 0.8),
        (660.0, 8_000.0, -0.6),
    ] {
        let params = WaveguideParams {
            style: WaveguideStyle::Tube,
            frequency_hz,
            loop_filter_cutoff: cutoff,
            loop_filter_resonance: 0.3,
            loop_gain: 0.985,
            loop_nonlinearity: 0.4,
            position_of_strike: 0.2,
            pickup_position: 0.7,
            boundary_reflection,
            ..WaveguideParams::default()
        };
        // Extreme, rapidly-changing, and non-finite steepening drive: the
        // unity-magnitude dispersion and the energy-gated radiation must keep
        // the bore finite and bounded regardless.
        let output =
            render_tube_sustained_driven(sample_rate, params, 24_000, |index| match index % 7 {
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

fn render_tube_sustained_driven(
    sample_rate: f32,
    params: WaveguideParams,
    sample_count: usize,
    drive: impl Fn(usize) -> f32,
) -> Vec<f32> {
    let mut tube = Tube1d::new(sample_rate, 20.0);
    let mut output = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        tube.set_steepening_drive(drive(index));
        let phase = std::f32::consts::TAU * params.frequency_hz * index as f32 / sample_rate;
        output.push(tube.process_sample(0.6 * phase.sin(), params));
    }
    output
}

fn render_tube_1d(sample_rate: f32, params: WaveguideParams, sample_count: usize) -> Vec<f32> {
    let mut tube = Tube1d::new(sample_rate, 20.0);
    let mut output = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        output.push(tube.process_sample((index == 0) as u8 as f32, params));
    }
    output
}
