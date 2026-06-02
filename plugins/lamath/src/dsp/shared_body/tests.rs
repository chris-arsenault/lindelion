use super::*;
use crate::assert_no_allocations;
use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs};

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
    BodyStrike {
        selected: SelectedExcitations::from_single(&IMPULSE, SR),
        force_gain: 1.0,
        base_frequency: 220.0,
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
