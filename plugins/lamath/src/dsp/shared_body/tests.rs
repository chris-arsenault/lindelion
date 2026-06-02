use super::*;
use crate::assert_no_allocations;
use crate::{
    EnvelopeConfig, MeshConfig, ModulationConfig, OutputConfig, ResonatorConfig, SurroundingConfig,
};
use lindelion_dsp_utils::analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms};

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

/// Render `samples` of a fresh enabled **Mesh** body struck once at `force`.
fn render_mesh_strike(force: f32, samples: usize) -> Vec<f32> {
    let patch = ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Mesh(MeshConfig::default()),
        ..ResonatorSynthPatch::default()
    };
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(strike_at(force, 220.0));
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    body.render_add(&mut left, &mut right);
    left
}

/// M5 (ADR-0029): the body's measured-energy follower drives the Mesh geometric
/// (von Kármán) nonlinearity. A nonlinear body breaks linear scaling — a hard strike is
/// not a scaled copy of a soft one. With `energy = 0` the Mesh core is linear, so the
/// scaled residual is ~0; once the follower feeds energy, the residual is significant.
#[test]
fn shared_body_energy_drives_mesh_nonlinearity() {
    let soft = render_mesh_strike(0.2, 4_096);
    let hard = render_mesh_strike(1.0, 4_096);
    let k = 5.0; // 1.0 / 0.2

    let peak = peak_abs(&hard);
    assert!(peak > 0.0, "the mesh body must ring");

    let mut residual = 0.0_f32;
    for index in 0..hard.len() {
        residual = residual.max((hard[index] - k * soft[index]).abs());
    }
    let relative = residual / peak;
    assert!(
        relative > 0.05,
        "the energy follower must drive the mesh nonlinearity (break linear scaling): \
         relative residual {relative}",
    );
}

/// Render `samples` of a fresh enabled default (Modal) body struck once at full force,
/// with the patch's output `saturation_drive` set to `drive`.
fn render_strike_with_saturation(drive: f32, samples: usize) -> Vec<f32> {
    let patch = ResonatorSynthPatch {
        output: OutputConfig {
            saturation_drive: drive,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    };
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(strike_at(1.0, 220.0));
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    body.render_add(&mut left, &mut right);
    left
}

/// M6 (ADR-0031): the body applies the patch's output saturation as a static post-body
/// coloration stage. The same strike rendered with saturation drive differs from the
/// undriven render — the coloration stage is active.
#[test]
fn shared_body_applies_output_saturation() {
    let clean = render_strike_with_saturation(0.0, 2_048);
    let driven = render_strike_with_saturation(0.9, 2_048);

    assert_all_finite(&driven);
    let mut max_diff = 0.0_f32;
    for index in 0..clean.len() {
        max_diff = max_diff.max((driven[index] - clean[index]).abs());
    }
    assert!(
        max_diff > 1.0e-3,
        "the output saturation coloration must change the body's output: max diff {max_diff}",
    );
}

/// M6 (ADR-0031): the body's decay is the envelope — its ring is not gated by a per-note
/// amp release. Even with a 1 ms patch amp release, the struck ring keeps sounding far
/// beyond it (the body passes `amp = 1.0`, stepping the envelope aside).
#[test]
fn shared_body_ring_not_gated_by_amp_release() {
    let patch = ResonatorSynthPatch {
        modulation: ModulationConfig {
            amp_envelope: EnvelopeConfig {
                release_ms: 1.0,
                ..EnvelopeConfig::default()
            },
            ..ModulationConfig::default()
        },
        ..ResonatorSynthPatch::default()
    };
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(impulse_strike());

    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];
    body.render_add(&mut left, &mut right);

    assert_all_finite(&left);
    assert!(
        peak_abs(&left[4_096..]) > 0.0,
        "the body ring must persist well past a short amp release (not envelope-gated)",
    );
}

/// Render `samples` of a fresh enabled default (Modal) body struck once at full force,
/// with the patch's `mechanical_noise` depth set to `depth`.
fn render_strike_with_mechanical_noise(depth: f32, samples: usize) -> Vec<f32> {
    let patch = ResonatorSynthPatch {
        surrounding: SurroundingConfig {
            mechanical_noise: depth,
            ..SurroundingConfig::default()
        },
        ..ResonatorSynthPatch::default()
    };
    let mut body = SharedBody::new(SR, &patch);
    body.set_enabled(true);
    body.strike(strike_at(1.0, 220.0));
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    body.render_add(&mut left, &mut right);
    left
}

/// M6 (ADR-0031): each strike arms the mechanical-noise attack burst. With noise depth the
/// early (attack) window carries extra broadband energy absent at depth 0, and the burst is
/// transient — once it decays the two renders converge (the resonator path is identical).
#[test]
fn shared_body_strike_fires_attack_noise() {
    let with_noise = render_strike_with_mechanical_noise(0.8, 4_096);
    let without = render_strike_with_mechanical_noise(0.0, 4_096);

    assert_all_finite(&with_noise);
    // The resonator path is identical in both renders (the noise is added after the stack
    // and never feeds `body_energy`), so the difference signal is exactly the attack noise.
    let diff: Vec<f32> = with_noise
        .iter()
        .zip(&without)
        .map(|(a, b)| a - b)
        .collect();
    let early_diff = rms(&diff[..256]);
    let late_diff = rms(&diff[3_072..]);
    assert!(
        early_diff > 1.0e-3,
        "the per-strike attack noise must fire in the early window: early {early_diff}",
    );
    assert!(
        early_diff > late_diff * 4.0,
        "the attack noise must be transient (decays away): early {early_diff} late {late_diff}",
    );
}
