//! Bowed-contact friction kernel in velocity waves.
//!
//! The bow is a point friction contact on the traveling-wave string. Working in
//! velocity waves makes the discrete contact solve exact: the string's incoming
//! (history) velocity at the contact is the sum of the two rail samples at the
//! contact position, and a transverse force `F` applied at that point moves the
//! contact velocity by `F / (2·Z₀)` while radiating `F / (2·Z₀)` outgoing on each
//! rail. No numerical differentiation and no per-sample unit constants are
//! involved, so the contact behaves identically at every host sample rate.
//!
//! The stick/slip decision is the Friedlander construction: the friction curve
//! against the string's load line `v_surface = v_h + (Y_c + Y_t)·F` (the bow
//! grips the string surface, which gives both transversely and torsionally;
//! only the transverse share radiates — see `BOW_TORSIONAL_ADMITTANCE`). Sticking is a velocity
//! constraint (`v = v_bow`) feasible while the required force stays inside the
//! static cone; sliding solves the load line against the velocity-weakening
//! kinetic curve with a bracketed root solve (the curve/load-line intersection is
//! multivalued in the falling region, where a naive fixed-point iteration
//! diverges). The McIntyre–Schumacher–Woodhouse hysteresis rule picks the branch:
//! a sticking contact sticks while it can; a sliding contact keeps sliding while
//! a sliding solution exists, and on stick release the solve jumps to the
//! large-relative-velocity branch.

use lindelion_dsp_utils::math;

/// Transverse contact admittance `Y_c = 1/(2·Z₀)` in normalized wave units.
/// The bridge junction already defines the string's characteristic admittance
/// as `G_s = 1` (`body::STRING_BRIDGE_ADMITTANCE`), so `Y_c` is exactly
/// `G_s/2`: a point force radiates equally into the two string halves in
/// parallel. Derived, not tunable.
pub(crate) const BOW_CONTACT_ADMITTANCE: f32 = 0.5;
/// Torsional contact admittance (torsion-as-resistance reduction). The bow
/// grips the string *surface*, whose velocity is transverse plus `radius·ω`
/// from torsion; torsional waves are strongly damped internally and return no
/// coherent echo, so they act as a pure resistance in the load line: the
/// friction force sees `Y_c + Y_t`, but only the `Y_c` share radiates into
/// the transverse rails — the torsional share is absorbed. This compliance
/// absorbs slip transients, widening the stable bowing band (both the
/// Helmholtz regime at low pressure and the raucous band over the Schelleng
/// maximum) and reducing its pitch dependence. Measured torsional surface
/// admittances on violin strings are a few tenths of the transverse value.
pub(crate) const BOW_TORSIONAL_ADMITTANCE: f32 = 0.15;
/// Total surface admittance the friction force works against.
pub(crate) const BOW_SURFACE_ADMITTANCE: f32 = BOW_CONTACT_ADMITTANCE + BOW_TORSIONAL_ADMITTANCE;
/// Share of the contact force that radiates as transverse waves.
pub(crate) const BOW_RADIATED_FRACTION: f32 = BOW_CONTACT_ADMITTANCE / BOW_SURFACE_ADMITTANCE;
/// Time constant of the torsional low-frequency relief. The resistance
/// reduction is valid only for oscillating force components: at DC the string
/// cannot twist indefinitely under bounded torque, so a sustained mean force
/// must see the stiff transverse-only load line (otherwise a permanent-stick
/// fixed point appears inside the static cone and the bow drags the string at
/// constant velocity forever instead of building to the Helmholtz break).
/// The model tracks the slow mean force and feeds `Y_t · F_mean` back into
/// the lead, which cancels the torsional give exactly at DC while leaving
/// fast components fully resistive. The corner sits near 400 Hz: torsional
/// waves resonate well above the transverse fundamental (≈5.7× on real
/// strings), so the resistance picture holds for slip transients but not for
/// fundamental-rate or sub-fundamental motion — a lower corner audibly
/// enables stable period-doubled (sub-octave) locks under overpressure.
pub(crate) const BOW_TORSION_RELIEF_SECONDS: f32 = 0.000_4;
/// Static (sticking) friction coefficient of rosin on string.
pub(crate) const BOW_MU_STATIC: f32 = 0.9;
/// Kinetic friction floor at large relative speed.
pub(crate) const BOW_MU_DYNAMIC: f32 = 0.2;
/// Normal-force scale (normalized force units, `Z₀ = 1`) at pressure 1. Sized
/// from the Schelleng cone: at the default bow (speed ≈ 0.13 wave-velocity
/// units, β ≈ 0.12) the maximum bow force is `2·Z₀·v_b/(β(μs−μd)) ≈ 3.1` and the
/// minimum is roughly a tenth of that, so full pressure reaches the crush
/// boundary and the default pressure (≈0.5) sits mid-cone.
pub(crate) const BOW_NORMAL_FORCE_MAX: f32 = 2.5;
/// Bow velocity range (wave-velocity units) across the speed parameter. The
/// scale anchors bowed string amplitude to the same wave units a full pluck
/// reaches, since Helmholtz amplitude rides on bow velocity.
pub(crate) const BOW_SPEED_MIN: f32 = 0.02;
pub(crate) const BOW_SPEED_MAX: f32 = 0.32;
/// Stribeck velocity scale (wave-velocity units) of the falling friction curve
/// across the friction parameter: a large scale weakens gently (smooth, broad
/// stick), a small one drops sharply toward `μ_d` (grippy rosin, scratch-prone).
pub(crate) const BOW_SMOOTH_SLIP_VELOCITY: f32 = 0.10;
pub(crate) const BOW_SHARP_SLIP_VELOCITY: f32 = 0.015;
/// Bow weight at zero stroke speed, as a fraction of the full normal force.
/// Zero: the arm's weight rides *exactly* with the stroke speed, which keeps
/// the Schelleng overpressure ratio invariant through stroke starts and
/// direction changes (force and speed scale together, so the ratio's stroke
/// factor cancels). Any positive floor guarantees a crush instant at every
/// stroke start — the maximum bow force collapses as the speed passes zero
/// while the floored weight stays on the string, which renders as an onset
/// scratch on every separated note.
pub(crate) const BOW_STROKE_WEIGHT_FLOOR: f32 = 0.0;
/// Dynamics floor: played effort scales bow velocity and normal force together
/// by `FLOOR + (1−FLOOR)·effort`. Loudness rides primarily on bow speed
/// (Helmholtz amplitude ∝ v_b), and scaling the force by the same factor keeps
/// the contact at the same point of the Schelleng cone across the dynamic
/// range instead of sliding from flautando into crush.
pub(crate) const BOW_EFFORT_FLOOR: f32 = 0.25;
/// Safety clamp on the per-sample velocity correction. The solve already bounds
/// `|c| ≤ μ_s·N·Y_c ≤ μ_s·BOW_NORMAL_FORCE_MAX·Y_c = 1.125` (the slip-noise
/// scale stays below the static cone); the clamp only catches non-finite or
/// corrupted state.
pub(crate) const BOW_CORRECTION_LIMIT: f32 = 1.2;
/// Depth of the rosin (slip) noise: the sliding friction force fluctuates with
/// the rosin/hair surface, so the kinetic curve is scaled by
/// `1 + depth·noise` while the contact slides (sticking transmits almost no
/// surface noise). Because the noise is gated by the slip phase it is
/// re-excited once per period — the pitch-synchronous broadband bed between
/// the harmonics that a measured bowed tone shows at ≈ −55..−75 dB relative
/// to the fundamental (Iowa MIS arco reference). Without it the model renders
/// a numerically pure line spectrum that reads as a bell/organ, not a bow.
pub(crate) const BOW_SLIP_NOISE_DEPTH: f32 = 0.30;
/// One-pole bandwidth of the rosin-noise source in Hz: surface noise is
/// broadband but not white to the very top; a few kHz matches the measured
/// inter-harmonic bed's gentle high roll-off.
pub(crate) const BOW_SLIP_NOISE_BANDWIDTH_HZ: f32 = 6_500.0;
/// Bounds on the slip-noise friction scale (safety: keeps the slide solve's
/// bracket and passivity intact for any noise sample).
pub(crate) const BOW_SLIP_NOISE_SCALE_MIN: f32 = 0.4;
pub(crate) const BOW_SLIP_NOISE_SCALE_MAX: f32 = 1.6;

/// Upstream advance (and FIFO length, in samples) of the incoming-wave reads.
/// The contact's history read must never observe its own outgoing writes: the
/// finite-width read window (±1 sample) plus the cubic interpolation stencil
/// (±2) around each tap overlaps the freshly written outgoing field if read at
/// the contact itself, feeding ~30% of the last force straight back into the
/// next history sample — enough positive feedback to flatten the string's
/// restoring echo from ≈ −1 to ≈ −0.4 per unit correction and pin the contact
/// in a false sliding equilibrium (no Helmholtz motion). Reading 6 samples
/// upstream (where writes never land — they only age downstream) and delaying
/// the result 6 samples in a FIFO yields the exact incoming wave at the
/// contact with zero geometric error.
pub(crate) const BOW_READ_ADVANCE_SAMPLES: usize = 6;
/// Minimum contact distance from either string end, in samples: the advanced
/// read plus its window/interpolation stencil must stay inside the rail. A
/// fixed clearance in *samples* is also the physical behavior — a violinist's
/// bow sits at a fixed distance from the bridge while the stopped length
/// shrinks, so the relative bowing point grows toward the middle on short
/// (high) strings.
pub(crate) const BOW_BOUNDARY_CLEARANCE_SAMPLES: f32 = 10.0;

/// Intonation servo (the player's finger): the slip→stick capture happens
/// once per Helmholtz cycle, so capture intervals measure the *sounding*
/// period directly. The model trims its tuning state toward the played target
/// with a slow multiplicative correction, absorbing the net of the two real
/// pitch mechanisms a player compensates for — friction-hysteresis flattening
/// (grows with pressure) and sustained tension sharpening (grows with
/// amplitude). Adaptation only runs while consecutive intervals agree (steady
/// Helmholtz); raucous/chaotic regimes freeze the correction instead of
/// chasing noise.
pub(crate) const BOW_TUNE_SERVO_SECONDS: f32 = 0.15;
/// Captures count as a stable cycle when consecutive intervals agree within
/// this relative tolerance and sit inside the plausible window around the
/// target period.
pub(crate) const BOW_TUNE_INTERVAL_TOLERANCE: f32 = 0.12;
pub(crate) const BOW_TUNE_INTERVAL_WINDOW: (f32, f32) = (0.6, 1.6);
/// Correction bounds: ±60 cents of delay trim, covering the measured net
/// detune range with margin while keeping the delay well inside the buffer.
pub(crate) const BOW_TUNE_FACTOR_MIN: f32 = 0.966;
pub(crate) const BOW_TUNE_FACTOR_MAX: f32 = 1.035;
/// Relaxation time of the correction toward neutral while the bow is not
/// engaged, so a following pluck is not colored by stale bow intonation.
pub(crate) const BOW_TUNE_RELEASE_SECONDS: f32 = 0.1;

/// Coarse bracketing segments + bisection refinements of the sliding solve.
/// The scan walks `|c| ∈ (0, |Δv|]` from the stick end of the load line, so the
/// first sign change is the largest-relative-velocity sliding branch (the MSW
/// jump target); bisection then converges to ~`|Δv|/2^18`. Fixed counts keep
/// the kernel allocation-free and deterministic on the audio thread.
const SLIP_SCAN_SEGMENTS: usize = 8;
const SLIP_BISECT_ITERATIONS: usize = 18;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BowContactSolve {
    /// Velocity correction `c = F·Y_c` to radiate on each rail at the contact.
    pub velocity_correction: f32,
    /// Contact force in normalized force units (probe/diagnostic value).
    pub force: f32,
    pub sticking: bool,
    /// Sign of the relative slide while slipping, 0.0 while sticking.
    pub slip_direction: f32,
}

impl BowContactSolve {
    fn released() -> Self {
        Self {
            velocity_correction: 0.0,
            force: 0.0,
            sticking: false,
            slip_direction: 0.0,
        }
    }

    fn stick(correction: f32) -> Self {
        Self {
            velocity_correction: correction,
            force: correction / BOW_CONTACT_ADMITTANCE,
            sticking: true,
            slip_direction: 0.0,
        }
    }

    fn slide(correction: f32, direction: f32) -> Self {
        Self {
            velocity_correction: correction,
            force: correction / BOW_CONTACT_ADMITTANCE,
            sticking: false,
            slip_direction: direction,
        }
    }
}

/// Velocity-weakening kinetic friction coefficient at relative speed
/// `relative_speed ≥ 0` (the classic exponential Friedlander curve).
fn kinetic_friction(relative_speed: f32, slip_velocity: f32) -> f32 {
    let relative_speed = math::finite_or(relative_speed, 0.0).max(0.0);
    let slip_velocity = math::finite_or(slip_velocity, BOW_SMOOTH_SLIP_VELOCITY).max(1.0e-4);
    BOW_MU_DYNAMIC + (BOW_MU_STATIC - BOW_MU_DYNAMIC) * (-relative_speed / slip_velocity).exp()
}

/// One sample of bow-contact inputs.
///
/// `history_velocity` is the incoming-wave (force-free) string velocity at the
/// contact, `bow_velocity` the bow's transverse speed, `normal_force ≥ 0` the
/// bow pressure in normalized force units, `slip_velocity` the Stribeck scale.
/// `slip_friction_scale` modulates the *kinetic* curve only (rosin noise; 1.0
/// is the noiseless curve). `was_sticking`/`previous_slip_direction` carry the
/// hysteresis branch memory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BowContactInputs {
    pub history_velocity: f32,
    pub bow_velocity: f32,
    pub normal_force: f32,
    pub slip_velocity: f32,
    pub slip_friction_scale: f32,
    /// `Y_t · F_mean`: the torsional low-frequency relief (see
    /// `BOW_TORSION_RELIEF_SECONDS`), added to the lead so a sustained mean
    /// force sees the stiff transverse-only load line.
    pub torsion_relief_velocity: f32,
    pub was_sticking: bool,
    pub previous_slip_direction: f32,
}

/// Solve one sample of the bow contact.
pub(crate) fn solve_bow_contact(inputs: BowContactInputs) -> BowContactSolve {
    let BowContactInputs {
        history_velocity,
        bow_velocity,
        normal_force,
        slip_velocity,
        slip_friction_scale,
        torsion_relief_velocity,
        was_sticking,
        previous_slip_direction,
    } = inputs;
    let history_velocity = math::finite_or(history_velocity, 0.0);
    let bow_velocity = math::finite_or(bow_velocity, 0.0);
    let torsion_relief_velocity = math::finite_or(torsion_relief_velocity, 0.0);
    let normal_force = math::finite_or(normal_force, 0.0).max(0.0);
    let slip_friction_scale = math::finite_clamp(
        slip_friction_scale,
        BOW_SLIP_NOISE_SCALE_MIN,
        BOW_SLIP_NOISE_SCALE_MAX,
        1.0,
    );
    if normal_force <= f32::EPSILON {
        return BowContactSolve::released();
    }
    let sliding_force = normal_force * slip_friction_scale;

    // Velocity the bow leads the free string by; sticking must supply exactly
    // this surface-velocity correction, and the static cone bounds what it can
    // supply through the total (transverse + torsional) surface admittance.
    let lead = bow_velocity - history_velocity + torsion_relief_velocity;
    let static_capacity = BOW_MU_STATIC * normal_force * BOW_SURFACE_ADMITTANCE;
    let stick_feasible = lead.abs() <= static_capacity;
    let previous_slip_direction = math::finite_or(previous_slip_direction, 0.0);

    if was_sticking || previous_slip_direction == 0.0 {
        if stick_feasible {
            return BowContactSolve::stick(lead * BOW_RADIATED_FRACTION);
        }
        // Stick release: jump to the sliding branch (MSW rule). The scan-from-
        // zero bracketing lands on the largest-relative-velocity branch.
        let direction = if lead >= 0.0 { 1.0 } else { -1.0 };
        return slide_or_clamp(lead, sliding_force, slip_velocity, direction);
    }

    let direction = if lead >= 0.0 { 1.0 } else { -1.0 };
    if direction == previous_slip_direction {
        // Keep sliding while a sliding solution exists, even where sticking is
        // also feasible (the hysteretic, multivalued region); capture only when
        // the sliding branch disappears.
        if let Some(surface_correction) = solve_slide_correction(lead, sliding_force, slip_velocity)
        {
            return BowContactSolve::slide(surface_correction * BOW_RADIATED_FRACTION, direction);
        }
        if stick_feasible {
            return BowContactSolve::stick(lead * BOW_RADIATED_FRACTION);
        }
        return slide_or_clamp(lead, sliding_force, slip_velocity, direction);
    }

    // The string overtook the bow: capture if the static cone can hold it,
    // otherwise slide in the new direction.
    if stick_feasible {
        return BowContactSolve::stick(lead * BOW_RADIATED_FRACTION);
    }
    slide_or_clamp(lead, sliding_force, slip_velocity, direction)
}

fn slide_or_clamp(
    lead: f32,
    sliding_force: f32,
    slip_velocity: f32,
    direction: f32,
) -> BowContactSolve {
    match solve_slide_correction(lead, sliding_force, slip_velocity) {
        Some(surface_correction) => {
            BowContactSolve::slide(surface_correction * BOW_RADIATED_FRACTION, direction)
        }
        // No sliding root exists only when sticking is feasible (the load line
        // ends inside the static cone), so this is unreachable with consistent
        // inputs; clamp to the kinetic floor as a safe fallback.
        None => BowContactSolve::slide(
            direction * BOW_MU_DYNAMIC * sliding_force * BOW_CONTACT_ADMITTANCE,
            direction,
        ),
    }
}

/// Bracketed root solve of the sliding contact in *surface* velocity units:
/// find the magnitude `m ∈ (0, |Δv|]` where the load line meets the kinetic
/// friction curve, `m = N·(Y_c + Y_t)·μ(|Δv| − m)` (`N` here is the
/// noise-scaled sliding force). Returns the signed surface correction (the
/// caller radiates the `Y_c` share), or `None` when no sliding intersection
/// exists (capture).
fn solve_slide_correction(lead: f32, sliding_force: f32, slip_velocity: f32) -> Option<f32> {
    let lead_magnitude = lead.abs();
    if lead_magnitude <= f32::EPSILON {
        return None;
    }
    let direction = if lead >= 0.0 { 1.0 } else { -1.0 };
    let capacity = sliding_force * BOW_SURFACE_ADMITTANCE;
    let residual = |magnitude: f32| {
        magnitude - capacity * kinetic_friction(lead_magnitude - magnitude, slip_velocity)
    };

    // residual(0) < 0 always (friction is positive). Walk toward the stick end
    // of the load line; the first sign change brackets the sliding branch with
    // the largest relative velocity.
    let mut low = 0.0_f32;
    let mut high = None;
    for segment in 1..=SLIP_SCAN_SEGMENTS {
        let magnitude = lead_magnitude * segment as f32 / SLIP_SCAN_SEGMENTS as f32;
        if residual(magnitude) >= 0.0 {
            high = Some(magnitude);
            break;
        }
        low = magnitude;
    }
    let mut high = high?;

    for _ in 0..SLIP_BISECT_ITERATIONS {
        let midpoint = 0.5 * (low + high);
        if residual(midpoint) >= 0.0 {
            high = midpoint;
        } else {
            low = midpoint;
        }
    }
    let magnitude = 0.5 * (low + high);
    Some(direction * magnitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLVE_TOLERANCE: f32 = 1.0e-4;

    #[allow(clippy::too_many_arguments)]
    fn solve(
        history_velocity: f32,
        bow_velocity: f32,
        normal_force: f32,
        slip_velocity: f32,
        slip_friction_scale: f32,
        was_sticking: bool,
        previous_slip_direction: f32,
    ) -> BowContactSolve {
        solve_bow_contact(BowContactInputs {
            history_velocity,
            bow_velocity,
            normal_force,
            slip_velocity,
            slip_friction_scale,
            torsion_relief_velocity: 0.0,
            was_sticking,
            previous_slip_direction,
        })
    }

    fn slide_residual(solve: BowContactSolve, lead: f32, normal_force: f32, slip: f32) -> f32 {
        // Force-units residual: the load line works in surface velocity, the
        // solve radiates only the transverse share.
        let force = solve.velocity_correction / BOW_CONTACT_ADMITTANCE;
        let relative = (lead - BOW_SURFACE_ADMITTANCE * force).abs();
        force.abs() - normal_force * kinetic_friction(relative, slip)
    }

    #[test]
    fn small_lead_sticks_and_supplies_the_radiated_share_of_the_lead() {
        let solve = solve(0.05, 0.13, 1.0, 0.1, 1.0, true, 0.0);
        assert!(solve.sticking);
        // Surface velocity matches the bow; only the transverse share radiates.
        assert!((solve.velocity_correction - 0.08 * BOW_RADIATED_FRACTION).abs() < 1.0e-6);
        assert_eq!(solve.slip_direction, 0.0);
    }

    #[test]
    fn large_lead_slides_with_converged_force() {
        for &slip in &[BOW_SMOOTH_SLIP_VELOCITY, BOW_SHARP_SLIP_VELOCITY] {
            for &lead in &[0.6_f32, 1.2, -0.9] {
                let normal_force = 1.0;
                let solve = solve(0.0, lead, normal_force, slip, 1.0, true, 0.0);
                assert!(!solve.sticking, "lead={lead} slip={slip}");
                let residual = slide_residual(solve, lead, normal_force, slip);
                assert!(
                    residual.abs() < SOLVE_TOLERANCE,
                    "non-converged slide: lead={lead} slip={slip} residual={residual}"
                );
                assert_eq!(solve.slip_direction, lead.signum());
                // The relative slide keeps the slip direction's sign.
                let force = solve.velocity_correction / BOW_CONTACT_ADMITTANCE;
                assert!((lead - BOW_SURFACE_ADMITTANCE * force) * solve.slip_direction > 0.0);
            }
        }
    }

    #[test]
    fn sliding_force_is_bounded_by_the_static_cone() {
        for step in 0..200 {
            let lead = -2.0 + step as f32 * 0.02;
            let solve = solve(0.0, lead, 2.5, 0.015, 1.0, false, 1.0);
            let limit = BOW_MU_STATIC * 2.5 * BOW_CONTACT_ADMITTANCE + 1.0e-6;
            assert!(
                solve.velocity_correction.abs() <= limit,
                "lead={lead} correction={}",
                solve.velocity_correction
            );
        }
    }

    #[test]
    fn hysteresis_keeps_sliding_where_stick_is_also_feasible() {
        // Sharp curve, lead inside the static cone but with a sliding root: the
        // multivalued Friedlander region. A sticking contact sticks; a sliding
        // contact keeps sliding.
        let normal_force = 2.0;
        let slip = BOW_SHARP_SLIP_VELOCITY;
        let lead = 0.7 * BOW_MU_STATIC * normal_force * BOW_SURFACE_ADMITTANCE;
        let from_stick = solve(0.0, lead, normal_force, slip, 1.0, true, 0.0);
        let from_slide = solve(0.0, lead, normal_force, slip, 1.0, false, 1.0);
        assert!(from_stick.sticking);
        assert!(!from_slide.sticking, "sliding contact should stay sliding");
        assert!(
            slide_residual(from_slide, lead, normal_force, slip).abs() < SOLVE_TOLERANCE,
            "slide branch must be a true intersection"
        );
    }

    #[test]
    fn capture_happens_when_the_sliding_branch_disappears() {
        // Tiny lead: no sliding intersection remains, a sliding contact captures.
        let solve = solve(0.0, 0.02, 1.0, BOW_SMOOTH_SLIP_VELOCITY, 1.0, false, 1.0);
        assert!(solve.sticking, "expected capture: {solve:?}");
    }

    #[test]
    fn released_when_normal_force_is_zero() {
        let solve = solve(0.3, 0.5, 0.0, 0.1, 1.0, true, 0.0);
        assert_eq!(solve, BowContactSolve::released());
    }

    #[test]
    fn slip_noise_modulates_sliding_force_within_bounds() {
        let lead = 1.0;
        let normal_force = 1.0;
        let quiet = solve(0.0, lead, normal_force, 0.05, 1.0, false, 1.0);
        let rough = solve(0.0, lead, normal_force, 0.05, 1.3, false, 1.0);
        assert!(!quiet.sticking && !rough.sticking);
        assert!(
            rough.velocity_correction > quiet.velocity_correction,
            "noise scale should modulate the sliding force: quiet={quiet:?} rough={rough:?}"
        );
        // Extreme noise inputs clamp; the correction stays inside the cone.
        let extreme = solve(0.0, lead, normal_force, 0.05, 100.0, false, 1.0);
        let cone = BOW_MU_STATIC * normal_force * BOW_SLIP_NOISE_SCALE_MAX * BOW_CONTACT_ADMITTANCE;
        assert!(extreme.velocity_correction.abs() <= cone + 1.0e-6);
    }

    #[test]
    fn non_finite_inputs_resolve_finite() {
        let solve = solve(
            f32::NAN,
            f32::INFINITY,
            f32::NAN,
            -1.0,
            f32::NAN,
            false,
            f32::NAN,
        );
        assert!(solve.velocity_correction.is_finite());
        assert!(solve.force.is_finite());
    }
}
