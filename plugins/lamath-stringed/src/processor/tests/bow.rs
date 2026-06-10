//! Bow-driver and humanize behavior tests.

#![allow(clippy::wildcard_imports)]

use super::*;

#[test]
fn bow_contact_sustains_without_runaway_crescendo() {
    let bowed = render_held_note(bow_smooth_patch(), 60, 1.0, 48_000);
    let mid = rms_window(&bowed, 0.28, 0.12);
    let late = rms_window(&bowed, 0.78, 0.12);

    assert_all_finite(&bowed);
    assert!(peak_abs(&bowed) < 1.0, "peak={}", peak_abs(&bowed));
    // Objective audibility floor (not merely non-silence): the bowed voice is
    // carried entirely by the calibrated body radiation while the bow drives.
    assert!(late > 0.01, "bow late sustain too quiet: {late}");
    assert!(
        late < mid * 2.0,
        "bow should not run away: mid={mid}, late={late}"
    );
}

/// Coefficient of variation of short-window RMS: steady Helmholtz motion holds
/// a near-constant envelope, while a crushed (over-pressure) contact produces
/// irregular slip bursts with a strongly fluctuating envelope.
fn envelope_roughness(samples: &[f32]) -> f32 {
    let window = 1_024;
    let levels: Vec<f32> = samples.chunks_exact(window).map(rms).collect();
    let mean = levels.iter().sum::<f32>() / levels.len().max(1) as f32;
    if mean <= f32::EPSILON {
        return 0.0;
    }
    let variance = levels
        .iter()
        .map(|level| (level - mean) * (level - mean))
        .sum::<f32>()
        / levels.len().max(1) as f32;
    variance.sqrt() / mean
}

#[test]
fn bow_scratch_is_audible_and_distinct_from_smooth() {
    let smooth = render_held_note(bow_smooth_patch(), 60, 1.0, 36_000);
    let scratch = render_held_note(bow_scratch_patch(), 60, 1.0, 36_000);
    let smooth_tail = &smooth[12_000..];
    let scratch_tail = &scratch[12_000..];

    assert_all_finite(&smooth);
    assert_all_finite(&scratch);
    assert!(
        rms(scratch_tail) > 0.000_5,
        "scratch bow should be audible: {}",
        rms(scratch_tail)
    );
    assert!(
        rms_difference(smooth_tail, scratch_tail) > 0.000_05,
        "scratch should differ materially from smooth"
    );
    // Crushed bowing is *aperiodic*: the envelope churns instead of holding the
    // steady Helmholtz sustain (a centroid comparison is not a reliable
    // discriminator — raucous motion concentrates energy chaotically, not
    // necessarily higher).
    let smooth_roughness = envelope_roughness(smooth_tail);
    let scratch_roughness = envelope_roughness(scratch_tail);
    assert!(
        scratch_roughness > smooth_roughness * 1.5,
        "scratch should be rougher: smooth={smooth_roughness:.3} scratch={scratch_roughness:.3}"
    );
}

/// Normalized autocorrelation at the fundamental lag over the tail window.
fn tail_periodicity(samples: &[f32], f0: f32) -> f32 {
    let tail = &samples[12_000..];
    let lag_center = (SAMPLE_RATE / f0).round() as usize;
    let mut best = f32::MIN;
    for lag in lag_center - 4..=lag_center + 4 {
        let n = tail.len() - lag;
        let mut num = 0.0f64;
        let mut den_a = 0.0f64;
        let mut den_b = 0.0f64;
        for i in 0..n {
            num += (tail[i] * tail[i + lag]) as f64;
            den_a += (tail[i] * tail[i]) as f64;
            den_b += (tail[i + lag] * tail[i + lag]) as f64;
        }
        best = best.max((num / (den_a * den_b).sqrt().max(1e-12)) as f32);
    }
    best
}

#[test]
fn humanize_zero_is_inert_and_deterministic() {
    // At humanize 0 the variance source contributes exact zeros, so two
    // independently-seeded processors render byte-identically.
    let patch = StringPatch {
        humanize: 0.0,
        phrasing: 0.0,
        vibrato: 0.0,
        ..StringPatch::default()
    };
    let first = render_held_note(patch.clone(), 60, 0.9, 24_000);
    let second = render_held_note(patch, 60, 0.9, 24_000);
    assert_eq!(first, second, "humanize 0 must be deterministic");
}

#[test]
fn humanize_moves_both_drivers_materially() {
    let dry_pluck = render_held_note(
        StringPatch {
            humanize: 0.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        24_000,
    );
    let humanized_pluck = render_held_note(
        StringPatch {
            humanize: 1.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        24_000,
    );
    let dry_bow = render_held_note(
        StringPatch {
            humanize: 0.0,
            ..bow_smooth_patch()
        },
        60,
        1.0,
        24_000,
    );
    let humanized_bow = render_held_note(
        StringPatch {
            humanize: 1.0,
            ..bow_smooth_patch()
        },
        60,
        1.0,
        24_000,
    );
    assert_all_finite(&humanized_pluck);
    assert_all_finite(&humanized_bow);
    assert!(
        rms_difference(&dry_pluck[512..], &humanized_pluck[512..]) > 0.000_01,
        "humanize should be audible on the pluck path"
    );
    assert!(
        rms_difference(&dry_bow[512..], &humanized_bow[512..]) > 0.000_1,
        "humanize should be audible on the bow path"
    );
}

#[test]
fn nominal_humanize_keeps_the_smooth_bow_in_regime_and_in_tune() {
    // Knob law: 0.5 is the nominal humanization — still steady Helmholtz
    // motion (no walk-induced crush) and near pitch parity.
    let patch = StringPatch {
        humanize: 0.5,
        ..bow_smooth_patch()
    };
    let output = render_held_note(patch, 60, 1.0, 48_000);
    assert_all_finite(&output);
    let f0 = midi_note_to_hz(60.0);
    let periodicity = tail_periodicity(&output, f0);
    // Seed-robust tripwire: the walks are instance-seeded, so the realization
    // varies with test execution order; the bound must hold for any seed.
    assert!(
        periodicity > 0.8,
        "nominal humanize should not crush the smooth bow: periodicity={periodicity:.3}"
    );
    let estimate = lindelion_dsp_utils::analysis::estimate_f0_autocorrelation_refined(
        &output[24_000..],
        SAMPLE_RATE,
        f0 * 0.9,
        f0 * 1.1,
    )
    .expect("no pitch estimate");
    let cents = lindelion_dsp_utils::math::cents_between(f0, estimate);
    assert!(
        cents.abs() < 12.0,
        "nominal humanize should stay near pitch parity: {cents:+.1} cents"
    );
}

#[test]
fn full_humanize_approaches_but_does_not_live_in_crunch() {
    // Knob law: 1.0 approaches the unmusical — the Schelleng axes may brush
    // the crush boundary, but the tone must remain a pitched bowed note, not
    // sustained crunch.
    let patch = StringPatch {
        humanize: 1.0,
        ..bow_smooth_patch()
    };
    let output = render_held_note(patch, 60, 1.0, 48_000);
    assert_all_finite(&output);
    let f0 = midi_note_to_hz(60.0);
    let periodicity = tail_periodicity(&output, f0);
    assert!(
        periodicity > 0.6,
        "full humanize should stay a pitched note: periodicity={periodicity:.3}"
    );
}

#[test]
fn bow_rearticulation_starts_a_fresh_stroke_but_legato_does_not() {
    // Separated notes rearticulate: the second note begins a fresh stroke from
    // rest (plus the gate dip), so the boundary carries a deep articulation
    // valley. Overlapping (legato) notes keep the stroke and only move the
    // stopped length, so the boundary stays comparatively level.
    let separated = render_phrase_with_patch(
        bow_smooth_patch(),
        &[(60, 0.0, 0.50, 1.0), (62, 0.56, 1.30, 1.0)],
    );
    let legato = render_phrase_with_patch(
        bow_smooth_patch(),
        &[(60, 0.0, 0.62, 1.0), (62, 0.56, 1.30, 1.0)],
    );
    assert_all_finite(&separated);
    assert_all_finite(&legato);

    let boundary_floor = |output: &[f32]| {
        // Deepest 10 ms RMS window across the note boundary region.
        let start = (0.48 * SAMPLE_RATE) as usize;
        let end = (0.66 * SAMPLE_RATE) as usize;
        let window = (0.010 * SAMPLE_RATE) as usize;
        output[start..end]
            .windows(window)
            .step_by(window / 2)
            .map(rms)
            .fold(f32::INFINITY, f32::min)
    };
    let sustain_after = |output: &[f32]| rms_window(output, 0.95, 0.20);

    let separated_floor = boundary_floor(&separated) / sustain_after(&separated).max(1.0e-9);
    let legato_floor = boundary_floor(&legato) / sustain_after(&legato).max(1.0e-9);
    assert!(
        separated_floor < legato_floor * 0.5,
        "rearticulation should carve a deeper boundary than legato: \
         separated={separated_floor:.4} legato={legato_floor:.4}"
    );
    assert!(
        sustain_after(&separated) > 0.005 && sustain_after(&legato) > 0.005,
        "both phrases should sustain the second note"
    );
}

#[test]
fn bow_drive_releases_after_note_off() {
    let held = render_held_note(bow_smooth_patch(), 60, 1.0, 48_000);
    let released = render_phrase_with_patch(bow_smooth_patch(), &[(60, 0.0, 0.35, 1.0)]);
    let held_tail = rms_window(&held, 0.78, 0.12);
    let released_tail = rms_window(&released, 0.78, 0.12);

    assert_all_finite(&released);
    assert!(
        released_tail < held_tail * 0.65,
        "released bow should ring down: held={held_tail}, released={released_tail}"
    );
}

#[test]
fn bow_drive_switch_off_uses_non_driven_pluck_path() {
    // Humanize pinned to zero: the walks are instance-seeded, so two
    // processors at nonzero humanize legitimately differ — this test asserts
    // path equivalence, not variance.
    let bow_off = StringPatch {
        driver: DriverSelection::Bow,
        humanize: 0.0,
        phrasing: 0.0,
        switches: ModelSwitches {
            bow_drive: false,
            ..ModelSwitches::default()
        },
        ..StringPatch::default()
    };
    let none = StringPatch {
        driver: DriverSelection::None,
        humanize: 0.0,
        phrasing: 0.0,
        vibrato: 0.0,
        ..StringPatch::default()
    };
    let bow_off_render = render_held_note(bow_off, 60, 0.9, 12_000);
    let none_render = render_held_note(none, 60, 0.9, 12_000);

    assert_all_finite(&bow_off_render);
    assert_all_finite(&none_render);
    assert!(
        rms_difference(&bow_off_render, &none_render) < 0.000_000_1,
        "bow-drive-off should match non-driven pluck path"
    );
}

#[test]
fn phrasing_zero_is_static_and_deterministic() {
    let patch = StringPatch {
        humanize: 0.0,
        phrasing: 0.0,
        ..bow_smooth_patch()
    };
    let first = render_held_note(patch.clone(), 60, 0.9, 24_000);
    let second = render_held_note(patch, 60, 0.9, 24_000);
    assert_eq!(first, second, "phrasing 0 must be deterministic");
}

#[test]
fn phrasing_shapes_attack_vibrato_and_release() {
    // One phrase-second of note then release: nominal phrasing must develop
    // the attack, bloom vibrato into the sustain, and taper the release
    // instead of gating it.
    let static_patch = StringPatch {
        phrasing: 0.0,
        ..bow_smooth_patch()
    };
    let phrased_patch = StringPatch {
        phrasing: 0.5,
        ..bow_smooth_patch()
    };
    // bow_smooth_patch pins vibrato to zero, so this exercises the note shape
    // (attack development, swell, release taper) in isolation.
    let schedule = [(60u8, 0.0f32, 1.0f32, 1.0f32)];
    let static_render = render_phrase_with_patch(static_patch, &schedule);
    let phrased = render_phrase_with_patch(phrased_patch, &schedule);
    assert_all_finite(&phrased);
    assert!(
        rms_difference(&static_render[4_800..], &phrased[4_800..]) > 0.000_1,
        "nominal phrasing should be audible"
    );
    // Reactive release: energy right after note-off (1.0 s) survives longer
    // with the taper than with the bare gate.
    let static_tail = rms_window(&static_render, 1.05, 0.1);
    let phrased_tail = rms_window(&phrased, 1.05, 0.1);
    assert!(
        phrased_tail > static_tail * 1.5,
        "release taper should sound past note-off: static={static_tail} phrased={phrased_tail}"
    );
}

#[test]
fn full_phrasing_vibrato_moves_pitch_within_musical_bounds() {
    let patch = StringPatch {
        phrasing: 1.0,
        vibrato: 1.0,
        humanize: 0.0,
        ..bow_smooth_patch()
    };
    let output = render_held_note(patch, 60, 1.0, 96_000);
    assert_all_finite(&output);
    let f0 = midi_note_to_hz(60.0);
    // Sustained window after vibrato bloom: per-slice pitch estimates must
    // wander (vibrato present) but stay within the theatrical depth.
    let mut estimates = Vec::new();
    for slice in 0..6 {
        let start = 48_000 + slice * 7_200;
        if let Some(estimate) = lindelion_dsp_utils::analysis::estimate_f0_autocorrelation_refined(
            &output[start..start + 7_200],
            SAMPLE_RATE,
            f0 * 0.9,
            f0 * 1.1,
        ) {
            estimates.push(estimate);
        }
    }
    assert!(estimates.len() >= 4, "pitch should remain trackable");
    let min = estimates.iter().copied().fold(f32::INFINITY, f32::min);
    let max = estimates.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let spread_cents = lindelion_dsp_utils::math::cents_between(min, max);
    assert!(
        spread_cents > 4.0,
        "full phrasing should show audible vibrato motion: {spread_cents:.1} cents"
    );
    assert!(
        spread_cents < 120.0,
        "vibrato should stay musical: {spread_cents:.1} cents"
    );
}

#[test]
fn host_expression_line_and_bend_shape_the_bowed_note() {
    use lindelion_plugin_shell::ControlEvent;
    let patch = StringPatch {
        humanize: 0.0,
        phrasing: 0.0,
        vibrato: 0.0,
        ..bow_smooth_patch()
    };
    let frames = 48_000;
    let render = |events_at_half: &[MidiEvent]| {
        let mut processor = StringProcessor::new(SAMPLE_RATE, patch.clone(), default_sources());
        let mut left = vec![0.0; frames / 2];
        let mut right = vec![0.0; frames / 2];
        processor.process(&[note_on(60, 1.0)], &mut left, &mut right);
        let mut left_tail = vec![0.0; frames / 2];
        let mut right_tail = vec![0.0; frames / 2];
        processor.process(events_at_half, &mut left_tail, &mut right_tail);
        left.extend_from_slice(&left_tail);
        left
    };

    let plain = render(&[]);
    let ducked = render(&[MidiEvent::Control(ControlEvent::ContinuousController {
        channel: 0,
        controller: 11,
        value: 0.2,
    })]);
    let plain_tail = rms_window(&plain, 0.8, 0.15);
    let ducked_tail = rms_window(&ducked, 0.8, 0.15);
    assert!(
        ducked_tail < plain_tail * 0.6,
        "CC11 should duck the dynamics line: plain={plain_tail} ducked={ducked_tail}"
    );

    let bent = render(&[MidiEvent::Control(ControlEvent::PitchBend {
        channel: 0,
        semitones: 1.0,
    })]);
    let f0 = midi_note_to_hz(60.0);
    let estimate = lindelion_dsp_utils::analysis::estimate_f0_autocorrelation_refined(
        &bent[36_000..],
        SAMPLE_RATE,
        f0,
        f0 * 1.2,
    )
    .expect("no pitch estimate for the bent note");
    let cents = lindelion_dsp_utils::math::cents_between(f0, estimate);
    assert!(
        (70.0..130.0).contains(&cents),
        "pitch bend should land near +100 cents: {cents:+.1}"
    );
}
