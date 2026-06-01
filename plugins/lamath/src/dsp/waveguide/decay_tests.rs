//! M11 P2 decay-structure tests: the raised T60 cap's near-unity-loop stability
//! and the re-tuned String voice (multi-second tail, `loop_gain` as the audible
//! decay control, and the body audibly shaping per-note decay). Heavy multi-second
//! renders, so gated behind `integration-tests` and run via `make test-integration`.

use super::*;
use crate::dsp::{
    constants::WAVEGUIDE_LOOP_GAIN,
    render_metrics::{RenderExcitation, render_waveguide_response},
};
use lindelion_dsp_utils::analysis::{
    assert_all_finite, partial_t60_seconds, peak_abs, rms, spectral_centroid_trajectory,
};

/// M11 P2 step 1: the raised T60 cap drives the loop to near-unity gain for the
/// longest decays. A 6 s render at the maximum loop gain must stay bounded,
/// finite, and non-growing — the near-unity-loop stability the phase requires.
/// (The audible String *T60* is gated by the M7 body absorption, not this cap;
/// that voicing target is step 2. The cap mapping itself is unit-tested in
/// `core::tests::raised_decay_cap_extends_max_t60_past_old_limit`.)
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn raised_t60_cap_keeps_near_unity_loop_stable() {
    let sample_rate = 48_000.0;
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: 330.0,
        loop_filter_cutoff: 8_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.999,
        loop_nonlinearity: 0.0,
        dispersion: 0.0,
        ..WaveguideParams::default()
    };
    let output = render_waveguide_response(sample_rate, params, 288_000, RenderExcitation::Impulse);
    assert_all_finite(&output);
    assert!(peak_abs(&output) < 4.0, "peak_abs={}", peak_abs(&output));

    // Non-growing: the long tail decays rather than accumulating energy.
    let early = rms(&output[..48_000]);
    let late = rms(&output[240_000..]);
    assert!(
        late < early,
        "near-unity-loop tail should decay, not grow: early={early}, late={late}"
    );
}

/// M11 P2 step 2: the default String voice rings to a multi-second tail with
/// frequency-dependent darkening, and the body's broadband loss no longer
/// overrides `loop_gain` as the decay control. Measured at 165 Hz (a guitar body
/// modal gap, so the broadband decay capacity is isolated from modal wolf-note
/// shortening — modal coloring is covered by the body tests).
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn string_default_pluck_rings_to_target_with_darkening() {
    let sample_rate = 48_000.0;
    // 165 Hz (E3): a representative played note clear of the dense low body-mode
    // field, so it rings to the String's free-pluck *capacity* rather than the
    // body-shortened decay of notes sitting on a body resonance (that per-note
    // variation is the body's audible signature — see
    // `body_audibly_shapes_per_note_string_decay` — and its midrange evenness is
    // re-tuned against the living tail in P8).
    let f0 = 165.0;
    // The default String voice: only the played frequency is overridden.
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: f0,
        ..WaveguideParams::default()
    };
    let output = render_waveguide_response(sample_rate, params, 288_000, RenderExcitation::Impulse);
    assert_all_finite(&output);
    assert!(peak_abs(&output) < 4.0, "peak_abs={}", peak_abs(&output));

    // (a) Sustain long enough to matter: the default free pluck rings ~5–6 s. P8's
    // body-loading decouple (the loop is loaded by only a fraction of the modal
    // admittance) cut the residual body loss on in-gap notes too, lengthening this
    // free-pluck capacity from the P2-era ~4–5 s.
    let t60 = partial_t60_seconds(&output, sample_rate, f0, 4_096, 4_096)
        .expect("default String fundamental should measurably decay");
    assert!(
        (4.5..=6.5).contains(&t60),
        "default String T60 should ring ~5–6 s: {t60}"
    );

    // (b) Frequency-dependent darkening: the spectral centroid falls over the tail
    // (the loop filter rolls the highest partials off first). The strength of the
    // per-partial T60(f) contrast at low cutoffs is covered separately by
    // `frequency_dependent_damping_decays_high_partials_faster_and_matches_target_t60`.
    let trajectory = spectral_centroid_trajectory(&output, sample_rate, 2_048, 16_384);
    assert!(
        trajectory.len() >= 2,
        "trajectory too short: {trajectory:?}"
    );
    assert!(
        trajectory.first().unwrap() > trajectory.last().unwrap(),
        "tail should darken: {trajectory:?}"
    );
}

/// M11 P2/P8: the body audibly shapes per-note decay — a note sitting on the dense
/// low body-mode field (220 Hz, near the 200/230 Hz plate modes) rings down faster
/// than an in-gap note (165 Hz) at the same default voice — but, after the P8
/// loading decouple, it is no longer *choked*: both still sustain seconds. The
/// frequency-localized coloration is present (near-mode meaningfully shorter); the
/// over-damping P8 removed (near-mode no longer ~1 s) is gone.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn body_audibly_shapes_per_note_string_decay() {
    let sample_rate = 48_000.0;
    let render = |f0: f32| {
        render_waveguide_response(
            sample_rate,
            WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: f0,
                ..WaveguideParams::default()
            },
            288_000,
            RenderExcitation::Impulse,
        )
    };

    let in_gap = partial_t60_seconds(&render(165.0), sample_rate, 165.0, 4_096, 4_096)
        .expect("in-gap note should ring");
    let near_mode = partial_t60_seconds(&render(220.0), sample_rate, 220.0, 4_096, 4_096)
        .expect("near-mode note should ring");
    assert!(
        near_mode < in_gap * 0.8,
        "body modes should audibly shorten the near-mode note: near_mode={near_mode}, in_gap={in_gap}"
    );
    assert!(
        near_mode > 2.5,
        "P8: the near-mode note should still sustain seconds, not be choked: near_mode={near_mode}"
    );
}

/// M11 P2 step 2: with the body's broadband loss reduced, `loop_gain` is the
/// audible decay control again — a low loop gain rings down markedly faster than
/// the long default. Measured at an in-gap note (165 Hz) where the loop, not the
/// body modal field, is the binding decay constraint.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn loop_gain_audibly_controls_string_decay() {
    let sample_rate = 48_000.0;
    let f0 = 165.0;
    let render = |loop_gain: f32| {
        render_waveguide_response(
            sample_rate,
            WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: f0,
                loop_gain,
                ..WaveguideParams::default()
            },
            288_000,
            RenderExcitation::Impulse,
        )
    };

    let long = partial_t60_seconds(
        &render(WAVEGUIDE_LOOP_GAIN.default),
        sample_rate,
        f0,
        4_096,
        4_096,
    )
    .expect("default loop gain should ring");
    let short = partial_t60_seconds(&render(0.6), sample_rate, f0, 4_096, 4_096)
        .expect("low loop gain should ring down");
    assert!(
        short < long * 0.5,
        "loop_gain should audibly control decay: short={short}, long={long}"
    );
}

/// M11 P8: the String midrange sustains, not over-damped. After the P2 broadband
/// body cut, the *modal* body coupling dominated midrange decay — notes whose
/// fundamental lands on the dense plate-mode field (e.g. 392 Hz) died in ~1–2 s
/// while low/in-gap notes rang ~5 s. P8 reduces the modal coupling so the midrange
/// rings several seconds, keeping the body coloration present.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn string_midrange_sustains_not_over_damped() {
    let sample_rate = 48_000.0;
    for f0 in [392.0_f32, 220.0] {
        let params = WaveguideParams {
            style: WaveguideStyle::String,
            frequency_hz: f0,
            ..WaveguideParams::default()
        };
        let output =
            render_waveguide_response(sample_rate, params, 288_000, RenderExcitation::Impulse);
        assert_all_finite(&output);
        let t60 = partial_t60_seconds(&output, sample_rate, f0, 4_096, 4_096)
            .expect("midrange note should measurably decay");
        assert!(
            t60 > 3.0,
            "midrange ({f0} Hz) should sustain several seconds, not over-damp: T60={t60}"
        );
    }
}
