use crate::{
    BowContactDrive, BowParams, StringBodyMode, StringModel, StringModelParams, StringModelProbe,
};
use lindelion_dsp_utils::{
    analysis::{assert_all_finite, audio_window_metrics, estimate_f0_autocorrelation},
    math::cents_between,
};

const GATE_RAMP_SECONDS: f32 = 0.006;

fn bow_model_params(frequency_hz: f32) -> StringModelParams {
    StringModelParams {
        frequency_hz,
        loop_filter_cutoff_hz: 9_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.99,
        dispersion: 0.2,
        strike_position: 0.3,
        pickup_position: 0.7,
        body_mode: StringBodyMode::Violin,
        ..StringModelParams::default()
    }
}

/// Drive the model with a gated bow contact and return `(output, probes)`.
fn render_bowed(
    sample_rate: f32,
    params: StringModelParams,
    bow: BowParams,
    effort: f32,
    sample_count: usize,
    release_at: Option<usize>,
) -> (Vec<f32>, Vec<StringModelProbe>) {
    let mut string = StringModel::new(sample_rate);
    let gate_coefficient = 1.0 - (-1.0 / (GATE_RAMP_SECONDS * sample_rate)).exp();
    let mut gate = 0.0_f32;
    let mut output = Vec::with_capacity(sample_count);
    let mut probes = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        let target = match release_at {
            Some(release) if index >= release => 0.0,
            _ => 1.0,
        };
        gate += (target - gate) * gate_coefficient;
        let contact = BowContactDrive {
            params: bow,
            effort,
            drive_gate: gate,
            stroke: 1.0,
        };
        let (sample, probe) = string.process_with_bow_contact_probe(0.0, params, Some(contact));
        output.push(sample);
        probes.push(probe);
    }
    (output, probes)
}

#[test]
fn default_bow_sustains_audible_pitched_tone() {
    let sample_rate = 48_000.0;
    let target_hz = 261.63;
    let (output, _) = render_bowed(
        sample_rate,
        bow_model_params(target_hz),
        BowParams::default(),
        0.8,
        43_200,
        None,
    );

    assert_all_finite(&output);
    let sustain = audio_window_metrics(&output[28_800..43_200], sample_rate);
    assert!(sustain.peak_abs < 1.5, "bow output too hot: {sustain:?}");
    // Objective audibility floor for the shipped default bow + violin body:
    // sustained level, not merely non-zero output (ADR-0032 spirit).
    assert!(
        sustain.rms > 0.02,
        "bowed sustain below audibility floor: {sustain:?}"
    );

    let estimate = estimate_f0_autocorrelation(
        &output[28_800..],
        sample_rate,
        target_hz * 0.7,
        target_hz * 1.4,
    )
    .expect("no pitch estimate for the bowed sustain");
    let cents = cents_between(target_hz, estimate);
    assert!(
        cents < 60.0,
        "bowed pitch off target: estimate={estimate} cents={cents}"
    );
}

#[test]
fn bow_exhibits_stick_slip_alternation() {
    let sample_rate = 48_000.0;
    let (output, probes) = render_bowed(
        sample_rate,
        bow_model_params(261.63),
        BowParams::default(),
        0.8,
        24_000,
        None,
    );
    assert_all_finite(&output);

    // Steady bowing is stick-slip motion: the contact must alternate between
    // sticking (slip_direction 0 is unobservable here, but the force probe
    // changes regime) rather than sticking or sliding forever. Count sign
    // structure on the late window via the solved force trace.
    let window = &probes[12_000..24_000];
    let sticking_samples = window
        .iter()
        .filter(|probe| probe.bow_wave_correction.abs() > 0.0)
        .count();
    assert!(
        sticking_samples > window.len() / 2,
        "bow contact mostly inactive: {sticking_samples}/{} active",
        window.len()
    );
    let force_peak = window
        .iter()
        .map(|probe| probe.bow_force.abs())
        .fold(0.0_f32, f32::max);
    let force_floor = window
        .iter()
        .map(|probe| probe.bow_force.abs())
        .fold(f32::INFINITY, f32::min);
    assert!(
        force_peak > force_floor * 3.0,
        "no stick/slip force alternation: peak={force_peak} floor={force_floor}"
    );
}

#[test]
fn bow_release_rings_down_without_silence_or_runaway() {
    let sample_rate = 48_000.0;
    let (output, _) = render_bowed(
        sample_rate,
        bow_model_params(220.0),
        BowParams::default(),
        0.7,
        48_000,
        Some(24_000),
    );
    assert_all_finite(&output);
    let held = audio_window_metrics(&output[16_000..23_000], sample_rate).rms;
    let tail = audio_window_metrics(&output[40_000..47_000], sample_rate).rms;
    assert!(held > 0.005, "held bow inaudible: {held}");
    assert!(
        tail < held,
        "released bow should decay: held={held} tail={tail}"
    );
}

#[test]
fn smooth_and_sharp_friction_render_distinct_contact_behavior() {
    let sample_rate = 48_000.0;
    let smooth = BowParams {
        friction: 0.1,
        ..BowParams::default()
    };
    let sharp = BowParams {
        friction: 0.95,
        ..BowParams::default()
    };
    let (smooth_output, _) = render_bowed(
        sample_rate,
        bow_model_params(261.63),
        smooth,
        0.8,
        24_000,
        None,
    );
    let (sharp_output, _) = render_bowed(
        sample_rate,
        bow_model_params(261.63),
        sharp,
        0.8,
        24_000,
        None,
    );
    assert_all_finite(&smooth_output);
    assert_all_finite(&sharp_output);
    let smooth_rms = audio_window_metrics(&smooth_output[12_000..], sample_rate).rms;
    let sharp_rms = audio_window_metrics(&sharp_output[12_000..], sample_rate).rms;
    assert!(smooth_rms > 0.005, "smooth bow inaudible: {smooth_rms}");
    assert!(sharp_rms > 0.005, "sharp bow inaudible: {sharp_rms}");
    let difference = lindelion_dsp_utils::analysis::rms_difference(
        &smooth_output[12_000..],
        &sharp_output[12_000..],
    );
    assert!(
        difference > 0.001,
        "friction had no audible effect: {difference}"
    );
}

/// Render a bowed sustain and return the pitch error in signed cents.
fn bowed_pitch_error_cents(hz: f32, bow: BowParams, effort: f32) -> f32 {
    use lindelion_dsp_utils::analysis::estimate_f0_autocorrelation_refined;
    let sample_rate = 48_000.0;
    let (output, _) = render_bowed(sample_rate, bow_model_params(hz), bow, effort, 57_600, None);
    let estimate =
        estimate_f0_autocorrelation_refined(&output[28_800..], sample_rate, hz * 0.8, hz * 1.2)
            .expect("no pitch estimate for bowed sustain");
    cents_between(hz, estimate) * (estimate - hz).signum()
}

/// The intonation servo holds the shipped default bow at pitch parity: the
/// friction-hysteresis flattening and the sustained tension sharpening are
/// both absorbed, like a player's finger (uncorrected they net 10–15 cents).
#[test]
fn default_bow_holds_pitch_parity() {
    let cents = bowed_pitch_error_cents(261.63, BowParams::default(), 0.8);
    assert!(
        cents.abs() <= 3.0,
        "default bow off pitch: {cents:+.1} cents"
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn bow_pitch_parity_holds_across_the_playing_range() {
    for hz in [130.81_f32, 261.63, 440.0, 523.25] {
        let cents = bowed_pitch_error_cents(hz, BowParams::default(), 0.8);
        assert!(cents.abs() <= 4.0, "{hz} Hz default: {cents:+.1} cents");
    }
    for pressure in [0.3_f32, 0.65, 0.8] {
        let cents = bowed_pitch_error_cents(
            261.63,
            BowParams {
                pressure,
                ..BowParams::default()
            },
            0.8,
        );
        assert!(cents.abs() <= 5.0, "pressure {pressure}: {cents:+.1} cents");
    }
    for speed in [0.25_f32, 0.7] {
        let cents = bowed_pitch_error_cents(
            261.63,
            BowParams {
                speed,
                ..BowParams::default()
            },
            0.8,
        );
        assert!(cents.abs() <= 6.0, "speed {speed}: {cents:+.1} cents");
    }
    // Absolute maximum effort runs a few cents sharp, as real ff bowing does.
    let cents = bowed_pitch_error_cents(261.63, BowParams::default(), 1.0);
    assert!(cents.abs() <= 12.0, "full effort: {cents:+.1} cents");
}
