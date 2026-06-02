use super::*;
use crate::assert_no_allocations;
use lindelion_dsp_utils::analysis::{assert_all_finite, dft_magnitude_at, peak_abs};

const SR: f32 = 48_000.0;

/// M1: with the engines unconfigured (no strike), the enabled body emits exactly
/// silence — summing it into the mix is inert.
#[test]
fn shared_body_render_is_silent() {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);

    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    body.render_add(&mut left, &mut right);

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert_eq!(peak_abs(&left), 0.0, "unfed body must be silent");
    assert_eq!(peak_abs(&right), 0.0, "unfed body must be silent");
}

#[test]
fn shared_body_render_does_not_allocate() {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    assert_no_allocations("shared_body_render", || {
        body.render_add(&mut left, &mut right);
    });
}

/// A short impulse driving the strike's excitation layer.
const IMPULSE: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

fn impulse_strike() -> BodyStrike<'static> {
    strike_at(1.0, 220.0)
}

fn strike_at(force_gain: f32, base_frequency: f32) -> BodyStrike<'static> {
    BodyStrike {
        selected: SelectedExcitations::from_single(&IMPULSE, SR),
        force_gain,
        base_frequency,
        pitch_ratio: 1.0,
    }
}

/// M2: a strike configures the body's idiophone stack and injects excitation, so the
/// body rings. The default patch is Modal on slot A (Parallel, mix_a = 1).
#[test]
fn shared_body_strike_produces_ring() {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(impulse_strike());

    let mut left = vec![0.0; 2_048];
    let mut right = vec![0.0; 2_048];
    body.render_add(&mut left, &mut right);

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(peak_abs(&left) > 0.0, "a strike must ring the body");
}

/// Render a fresh enabled body struck once at sample 0 and return `samples` of its
/// left-channel ring.
fn struck_ring(samples: usize) -> Vec<f32> {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(impulse_strike());
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    body.render_add(&mut left, &mut right);
    left
}

/// M2: a second overlapping strike adds a fresh injector without re-configuring, so the
/// first strike's decaying ring keeps propagating untouched — the body is not choked.
///
/// The body runs LTI here (energy/effort/drive-gate are zero until the M5 follower), so
/// "superimpose without choking" is exactly the superposition identity: the two-strike
/// ring over `[300, 600)` equals the first strike's continuing decay plus a second
/// strike's fresh response. A choke (state reset on the second strike) would drop the
/// first strike's decay and break the identity. (A bare peak-vs-residual check is unsound
/// here — the modal onset envelope beats, so an earlier window's peak can exceed a later
/// one regardless of choking.)
#[test]
fn shared_body_overlapping_strikes_superimpose() {
    const PRE: usize = 300;
    const POST: usize = 300;

    // Two strikes: one at sample 0, one at sample PRE.
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    let mut left = vec![0.0; PRE + POST];
    let mut right = vec![0.0; PRE + POST];
    body.strike(impulse_strike());
    body.render_add(&mut left[..PRE], &mut right[..PRE]);
    let pre_strike_ring = peak_abs(&left[..PRE]);
    assert!(pre_strike_ring > 0.0, "first strike must ring");
    body.strike(impulse_strike());
    body.render_add(&mut left[PRE..], &mut right[PRE..]);
    assert_all_finite(&left);

    // First strike's ring if it had never been re-struck (the decay that a choke would
    // discard), and a second strike's fresh response on a clean body.
    let first_alone = struck_ring(PRE + POST);
    let second_alone = struck_ring(POST);

    let mut max_error = 0.0_f32;
    for i in 0..POST {
        let superposed = first_alone[PRE + i] + second_alone[i];
        max_error = max_error.max((left[PRE + i] - superposed).abs());
    }
    assert!(
        max_error < 1e-3,
        "two-strike ring must equal the first strike's continuing decay plus the second \
         strike's fresh response (superposition, no choke); max error {max_error}",
    );
}

/// M2: the steady audio-thread strike + feed path is allocation-free (ADR-0001). The
/// body is armed once before the assert so the measured path is a strike onto an
/// already-configured body plus the injector feed.
#[test]
fn shared_body_strike_does_not_allocate() {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(impulse_strike());

    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    assert_no_allocations("shared_body_strike", || {
        body.strike(impulse_strike());
        body.render_add(&mut left, &mut right);
    });
}

/// M3 (ADR-0031 decision 3): a second strike at a new pitch retunes the live body
/// ring-preserving — it does **not** reset the first strike's decaying energy. Striking
/// again with zero force injects nothing, so any remaining sound is purely the preserved
/// ring; after the retune that ring's energy sits at the new pitch.
#[test]
fn shared_body_zero_force_restrike_moves_existing_ring_to_new_pitch() {
    let freq_a = 220.0;
    let freq_b = 370.0;
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);

    // Strike A and ring a short tail.
    let mut pre_left = vec![0.0; 512];
    let mut pre_right = vec![0.0; 512];
    body.strike(strike_at(1.0, freq_a));
    body.render_add(&mut pre_left, &mut pre_right);
    assert!(peak_abs(&pre_left) > 0.0, "first strike must ring");

    // Zero-force restrike at B: retune only, no new excitation.
    body.strike(strike_at(0.0, freq_b));
    let mut post_left = vec![0.0; 4_096];
    let mut post_right = vec![0.0; 4_096];
    body.render_add(&mut post_left, &mut post_right);

    assert_all_finite(&post_left);
    assert!(
        peak_abs(&post_left) > 0.0,
        "the preserved ring must still sound with no new excitation (no reset)",
    );
    let mag_a = dft_magnitude_at(&post_left, SR, freq_a);
    let mag_b = dft_magnitude_at(&post_left, SR, freq_b);
    assert!(
        mag_b > mag_a,
        "the existing ring must retune to the latest strike: mag_b {mag_b} <= mag_a {mag_a}",
    );
}

/// M3 (ADR-0031 decision 3): with a normal-force second strike at a new pitch, the
/// body's tuning tracks the most recent strike.
#[test]
fn shared_body_forced_restrike_tracks_latest_pitch() {
    let freq_a = 220.0;
    let freq_b = 370.0;
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);

    let mut pre_left = vec![0.0; 512];
    let mut pre_right = vec![0.0; 512];
    body.strike(strike_at(1.0, freq_a));
    body.render_add(&mut pre_left, &mut pre_right);

    body.strike(strike_at(1.0, freq_b));
    let mut post_left = vec![0.0; 4_096];
    let mut post_right = vec![0.0; 4_096];
    body.render_add(&mut post_left, &mut post_right);

    assert_all_finite(&post_left);
    let mag_a = dft_magnitude_at(&post_left, SR, freq_a);
    let mag_b = dft_magnitude_at(&post_left, SR, freq_b);
    assert!(
        mag_b > mag_a,
        "tuning must track the most recent strike: mag_b {mag_b} <= mag_a {mag_a}",
    );
}

/// M4 (ADR-0031 decision 4): a key-switch damp ramps the body toward silence over the
/// choke ramp — audible through the ramp, fully silent by its end — rather than cutting
/// instantly. The 60 ms ramp is ~2880 samples at 48 kHz, well within the 4096 rendered.
#[test]
fn shared_body_damp_ramps_to_silence() {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(impulse_strike());

    let mut pre_left = vec![0.0; 512];
    let mut pre_right = vec![0.0; 512];
    body.render_add(&mut pre_left, &mut pre_right);
    assert!(
        peak_abs(&pre_left) > 0.0,
        "the body must ring before the damp"
    );

    body.damp();
    let mut post_left = vec![0.0; 4_096];
    let mut post_right = vec![0.0; 4_096];
    body.render_add(&mut post_left, &mut post_right);

    assert_all_finite(&post_left);
    assert!(
        peak_abs(&post_left[..64]) > 0.0,
        "the damp must ramp, not cut the ring instantly",
    );
    assert_eq!(
        peak_abs(&post_left[3_584..]),
        0.0,
        "the damp must silence the body within the ramp",
    );
}

/// M4 (ADR-0001): the damp ramp and its end-of-ramp clear run on the audio thread and
/// must not allocate. The rendered block spans the full ramp so `silence()` (the clear)
/// is exercised inside the assert.
#[test]
fn shared_body_damp_does_not_allocate() {
    let patch = ResonatorSynthPatch::default();
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(impulse_strike());

    let mut left = vec![0.0; 4_096];
    let mut right = vec![0.0; 4_096];
    assert_no_allocations("shared_body_damp", || {
        body.damp();
        body.render_add(&mut left, &mut right);
    });
}
