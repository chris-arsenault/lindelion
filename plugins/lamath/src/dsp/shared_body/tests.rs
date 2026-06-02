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
