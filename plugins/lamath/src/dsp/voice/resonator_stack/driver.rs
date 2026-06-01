//! Force-dependent physical driver layer (M8, ADR-0017).
//!
//! The driver sits between the selected excitation and the waveguide resonator and
//! runs **inside the 2x oversampled inner loop** (ADR-0016), so its nonlinearity
//! interacts with the resonator at the higher rate. Each sub-sample it receives the
//! excitation, the player `effort` (from the M2 effort/energy bus), and the
//! resonator's coupled-back `feedback` sample (the resonator-coupled feedback seam
//! that a self-oscillating reed needs).
//!
//! - `PassThrough` (`DriverConfig::Sample`) is transparent: the existing sample /
//!   sidechain excitation reaches the resonator unchanged, so a patch with no
//!   physical driver renders exactly as before.
//! - `Pick` (`DriverConfig::Pick`) is a feed-forward contact: a force-dependent
//!   low-pass on the excitation whose bandwidth widens with playing effort, so a
//!   harder strike is brighter (the brightness-versus-force behavior).
//! - `Reed` (`DriverConfig::Reed`) is a self-oscillating wind valve (McIntyre-
//!   Schumacher-Woodhouse / Smith): the player effort sets the mouth pressure, the
//!   nonlinear reed table reflects the bore's returning wave (the `feedback`), and
//!   above a pressure threshold the bore self-oscillates (a regime change, not gain).

use lindelion_dsp_utils::{filters::OnePoleLowpass, math};

use crate::{BowConfig, DriverConfig, PickConfig, ReedConfig};

/// Pick contact low-pass bandwidth at the softest setting / lowest effort.
const PICK_MIN_CUTOFF_HZ: f32 = 300.0;
/// Pick contact low-pass bandwidth at the hardest setting / full effort.
const PICK_MAX_CUTOFF_HZ: f32 = 12_000.0;

/// Mouth pressure at full effort and full pressure depth (normalized wave units).
const REED_MAX_PRESSURE: f32 = 1.2;
/// Reed-table slope at the default stiffness (the `-0.8` MSW/STK reed slope); the
/// stiffness control scales it so a stiffer reed has a steeper table (brighter).
const REED_BASE_SLOPE: f32 = -0.8;
/// Reed-table offset at the default embouchure; the embouchure control biases it.
const REED_BASE_OFFSET: f32 = 0.6;
/// Gain applied to the resonator's coupled-back wave before the reed reads it as
/// bore pressure, compensating the radiated-output level so the bore→reed→bore loop
/// can self-oscillate. Negative: the open bore end inverts the returning wave.
const REED_FEEDBACK_GAIN: f32 = -0.95;
/// How strongly the incoming sample/sidechain excitation perturbs the breath (lets a
/// note-on transient kick-start the oscillation; the sustained drive is the breath).
const REED_EXCITATION_COUPLING: f32 = 0.5;
/// Hard safety clamp on the reed output so the active element can never run away.
const REED_OUTPUT_LIMIT: f32 = 4.0;
/// Injection gain into the bore. The Tube's mouth is a lossy reflection (~0.36), so
/// the bore round-trip loses most of its energy; the reed must supply enough
/// small-signal gain to push the bore→reed→bore loop above unity and self-oscillate.
/// The reed-table nonlinearity then bounds the amplitude (clamped reflection), so a
/// higher injection gain raises the oscillation level, not a runaway.
const REED_INJECTION_GAIN: f32 = 4.0;

/// Bow friction characteristic (exponential stick-slip): the static (stick) and
/// dynamic (slip) friction coefficients. The curve `μ_d + (μ_s−μ_d)·e^(−|Δv|/v0)`
/// is high near zero relative velocity (the string sticks to the bow) and falls to
/// the dynamic floor as the string slips — the negative-slope region whose
/// negative resistance sustains the Helmholtz motion.
const BOW_STATIC_FRICTION: f32 = 0.9;
const BOW_DYNAMIC_FRICTION: f32 = 0.2;
/// Bow normal force at full effort and full pressure depth (normalized wave units).
const BOW_MAX_FORCE: f32 = 1.0;
/// Bow velocity mapped from the `bow_speed` control. Kept near the slip-velocity
/// scale so the small-signal operating point sits on the steep (high-gain) shoulder
/// of the friction curve, where the negative resistance can build the oscillation.
const BOW_MIN_SPEED: f32 = 0.02;
const BOW_MAX_SPEED: f32 = 0.3;
/// Slip velocity `v0` (friction-curve width) mapped from the `friction` control:
/// a *higher* friction control narrows it (sharper stick-slip → scratchier).
const BOW_SMOOTH_SLIP_VELOCITY: f32 = 0.3;
const BOW_SHARP_SLIP_VELOCITY: f32 = 0.04;
/// How strongly the incoming sample/sidechain excitation perturbs the bow (lets a
/// note-on transient kick-start the motion; the sustained drive is the friction).
const BOW_EXCITATION_COUPLING: f32 = 0.5;
/// Injection gain of the friction force into the string. Above a lock threshold
/// (~0.08 here) the negative-resistance friction region overcomes the loop loss and
/// the string self-oscillates; the limit-cycle amplitude then scales with this gain.
/// M11 P9: lowered from 4.0 — at 4.0 the locked cycle ran to energy-bus RMS ~8.7
/// (far above full scale, clipping everything downstream). 0.12 keeps a robust lock
/// margin above the threshold while settling at a sane forte level (~0.3 RMS), which
/// the output makeup and master limiter can then stage.
const BOW_INJECTION_GAIN: f32 = 0.12;
/// Hard safety clamp on the bow output so the active element can never run away.
/// M11 P9: lowered from 4.0 to bound the per-sub-sample drive near the (now much
/// smaller) friction level while still admitting the note-on excitation kick.
const BOW_OUTPUT_LIMIT: f32 = 0.5;

#[derive(Debug, Default)]
pub(super) enum Driver {
    /// Transparent: the excitation passes to the resonator unchanged.
    #[default]
    PassThrough,
    Pick(PickDriver),
    Reed(ReedDriver),
    Bow(BowDriver),
}

impl Driver {
    pub(super) fn from_config(config: DriverConfig, sample_rate: f32) -> Self {
        match config {
            DriverConfig::Sample => Self::PassThrough,
            DriverConfig::Pick(pick) => Self::Pick(PickDriver::new(pick, sample_rate)),
            DriverConfig::Reed(reed) => Self::Reed(ReedDriver::new(reed)),
            DriverConfig::Bow(bow) => Self::Bow(BowDriver::new(bow)),
        }
    }

    /// Transform one (oversampled) excitation sample. `effort` is the player force
    /// from the effort/energy bus; `feedback` is the resonator's coupled-back sample
    /// from the previous sub-sample (the two-way driver<->resonator coupling);
    /// `drive_gate` is the note-state drive gate (1 while held, releasing to 0 after
    /// note-off) that lets a self-sustaining driver (reed/bow) stop driving so the
    /// resonator rings out. The feed-forward drivers (pass-through/pick) ignore it.
    pub(super) fn process(
        &mut self,
        excitation: f32,
        effort: f32,
        feedback: f32,
        drive_gate: f32,
    ) -> f32 {
        match self {
            Self::PassThrough => excitation,
            Self::Pick(pick) => pick.process(excitation, effort),
            Self::Reed(reed) => reed.process(excitation, effort, feedback, drive_gate),
            Self::Bow(bow) => bow.process(excitation, effort, feedback, drive_gate),
        }
    }

    pub(super) fn reset(&mut self) {
        match self {
            Self::PassThrough => {}
            Self::Pick(pick) => pick.reset(),
            Self::Reed(reed) => reed.reset(),
            Self::Bow(bow) => bow.reset(),
        }
    }
}

/// Feed-forward pick/hammer contact: a one-pole low-pass on the excitation whose
/// cutoff rises with effort and hardness, so a harder strike passes more high
/// frequencies (brighter). Contact time lowers the cutoff ceiling (a longer contact
/// is mellower). It only colors the attack — during sustain the excitation is ~0.
#[derive(Debug)]
pub(super) struct PickDriver {
    contact: OnePoleLowpass,
    hardness: f32,
    contact_time: f32,
    sample_rate: f32,
}

impl PickDriver {
    fn new(config: PickConfig, sample_rate: f32) -> Self {
        Self {
            contact: OnePoleLowpass::default(),
            hardness: math::finite_clamp(config.hardness, 0.0, 1.0, 0.5),
            contact_time: math::finite_clamp(config.contact_time, 0.0, 1.0, 0.5),
            sample_rate,
        }
    }

    fn process(&mut self, excitation: f32, effort: f32) -> f32 {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        // Brightness rises with hardness and effort; a soft/quiet strike keeps some
        // bandwidth so it is not silent, a hard/loud strike opens it up.
        let brightness = self.hardness * (0.2 + 0.8 * effort);
        // A longer contact time lowers the reachable ceiling (mellower attack).
        let ceiling = PICK_MAX_CUTOFF_HZ
            + (PICK_MIN_CUTOFF_HZ * 3.0 - PICK_MAX_CUTOFF_HZ) * self.contact_time;
        let cutoff = PICK_MIN_CUTOFF_HZ + (ceiling - PICK_MIN_CUTOFF_HZ) * brightness;
        self.contact.set_cutoff(cutoff, self.sample_rate);
        self.contact.process(excitation)
    }

    fn reset(&mut self) {
        self.contact.reset();
    }
}

/// Self-oscillating reed/lip valve (McIntyre-Schumacher-Woodhouse; Smith PASP). The
/// effort sets the mouth pressure; the nonlinear reed table reflects the bore's
/// returning wave (`feedback`). Below a pressure threshold the reed stays near its
/// rest point and the bore decays to silence; above it the bore self-oscillates.
#[derive(Debug)]
pub(super) struct ReedDriver {
    pressure_depth: f32,
    /// Reed-table slope (from stiffness) — steeper reed closes faster (brighter).
    slope: f32,
    /// Reed-table offset (from embouchure) — biases the rest opening.
    offset: f32,
}

impl ReedDriver {
    fn new(config: ReedConfig) -> Self {
        let stiffness = math::finite_clamp(config.stiffness, 0.0, 1.0, 0.5);
        let embouchure = math::finite_clamp(config.embouchure, 0.0, 1.0, 0.5);
        Self {
            pressure_depth: math::finite_clamp(config.pressure_depth, 0.0, 1.0, 0.5),
            // Stiffer reed -> steeper table (|slope| larger): 0.5x..1.5x the base.
            slope: REED_BASE_SLOPE * (0.5 + stiffness),
            // Embouchure biases the table offset around its default.
            offset: REED_BASE_OFFSET + (embouchure - 0.5) * 0.4,
        }
    }

    fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let drive_gate = math::finite_clamp(drive_gate, 0.0, 1.0, 1.0);
        // Mouth pressure from the player effort (squared so soft playing stays well
        // below the oscillation threshold and hard playing crosses it), gated by the
        // note-state drive gate so note-off releases the breath and the bore rings out.
        let mouth_pressure = self.pressure_depth * effort * effort * REED_MAX_PRESSURE * drive_gate;
        let breath = mouth_pressure + REED_EXCITATION_COUPLING * math::snap_to_zero(excitation);
        // Bore pressure: the resonator's returning wave, inverted at the open end.
        let bore = REED_FEEDBACK_GAIN * math::finite_or(feedback, 0.0);
        // Pressure across the reed and the nonlinear reed-table reflection (clamped
        // to a passive [-1, 1], so the reed reflects but never amplifies the wave).
        let pressure_diff = bore - breath;
        let reflection =
            math::finite_clamp(self.offset + self.slope * pressure_diff, -1.0, 1.0, 0.0);
        let output = REED_INJECTION_GAIN * (breath + pressure_diff * reflection);
        math::finite_clamp(output, -REED_OUTPUT_LIMIT, REED_OUTPUT_LIMIT, 0.0)
    }

    fn reset(&mut self) {
        // Stateless across the reset boundary (the bore holds the oscillation state).
    }
}

/// Continuous bow friction driver (exponential stick-slip; McIntyre-Schumacher-
/// Woodhouse). The player effort sets the bow normal force; the friction force
/// depends on the relative velocity `Δv = v_bow − v_string` (the string velocity
/// read from the resonator `feedback`). The negative-slope region of the friction
/// curve sustains the Helmholtz motion while the string is driven; the friction
/// saturation and the output clamp bound the limit cycle. Like the reed, the drive
/// is `effort`-based, so it sustains even when the sample excitation has decayed.
#[derive(Debug)]
pub(super) struct BowDriver {
    pressure_depth: f32,
    /// Bow velocity `v_bow` (from the bow_speed control).
    bow_speed: f32,
    /// Slip velocity `v0` (friction-curve width; from the friction control).
    slip_velocity: f32,
}

impl BowDriver {
    fn new(config: BowConfig) -> Self {
        let pressure_depth = math::finite_clamp(config.pressure_depth, 0.0, 1.0, 0.5);
        let bow_speed = math::finite_clamp(config.bow_speed, 0.0, 1.0, 0.5);
        let friction = math::finite_clamp(config.friction, 0.0, 1.0, 0.5);
        Self {
            pressure_depth,
            bow_speed: BOW_MIN_SPEED + (BOW_MAX_SPEED - BOW_MIN_SPEED) * bow_speed,
            // A higher friction control narrows v0 (sharper stick-slip).
            slip_velocity: BOW_SMOOTH_SLIP_VELOCITY
                + (BOW_SHARP_SLIP_VELOCITY - BOW_SMOOTH_SLIP_VELOCITY) * friction,
        }
    }

    fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let drive_gate = math::finite_clamp(drive_gate, 0.0, 1.0, 1.0);
        // String velocity at the bow contact (the resonator's returning wave).
        let string_velocity = math::finite_or(feedback, 0.0);
        let relative_velocity = self.bow_speed - string_velocity;
        // Bow normal force, gated by the note-state drive gate so note-off lifts the
        // bow and the string rings out at its natural decay.
        let normal_force = self.pressure_depth * effort * BOW_MAX_FORCE * drive_gate;
        // Exponential stick-slip coefficient: μ_d + (μ_s − μ_d)·e^(−|Δv|/v0).
        let mu = BOW_DYNAMIC_FRICTION
            + (BOW_STATIC_FRICTION - BOW_DYNAMIC_FRICTION)
                * (-(relative_velocity.abs()) / self.slip_velocity).exp();
        let friction = normal_force * mu * relative_velocity.signum();
        let drive = BOW_INJECTION_GAIN * friction
            + BOW_EXCITATION_COUPLING * math::snap_to_zero(excitation);
        math::finite_clamp(drive, -BOW_OUTPUT_LIMIT, BOW_OUTPUT_LIMIT, 0.0)
    }

    fn reset(&mut self) {
        // Stateless across the reset boundary (the string holds the oscillation).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_through_driver_is_transparent() {
        // The pass-through driver returns the excitation untouched for any effort or
        // feedback, so the waveguide receives exactly what it did before the seam.
        let mut driver = Driver::PassThrough;
        for &sample in &[-1.5_f32, -0.3, 0.0, 0.25, 0.9, 1.5] {
            for &effort in &[0.0_f32, 0.5, 1.0] {
                for &feedback in &[-2.0_f32, 0.0, 0.7] {
                    assert_eq!(driver.process(sample, effort, feedback, 1.0), sample);
                }
            }
        }
    }

    #[test]
    fn bow_driver_process_does_not_allocate() {
        // The bow runs per sub-sample inside the 2x realtime loop (ADR-0001), so its
        // friction evaluation must not allocate.
        use crate::assert_no_allocations;
        let mut bow = BowDriver::new(BowConfig::default());
        // Settle any lazy init outside the asserted region.
        for index in 0..64 {
            bow.process(0.0, 0.85, (index as f32 * 0.01).sin(), 1.0);
        }
        assert_no_allocations("bow_driver_process", || {
            for index in 0..512 {
                bow.process(0.0, 0.85, (index as f32 * 0.01).sin(), 1.0);
            }
        });
    }

    #[test]
    fn bow_output_is_bounded_across_effort_and_feedback() {
        let mut bow = BowDriver::new(BowConfig::default());
        for effort_step in 0..=10 {
            let effort = effort_step as f32 / 10.0;
            for feedback_step in -20..=20 {
                let feedback = feedback_step as f32 / 5.0;
                let out = bow.process(0.0, effort, feedback, 1.0);
                assert!(
                    out.is_finite() && out.abs() <= BOW_OUTPUT_LIMIT,
                    "out={out}"
                );
            }
        }
    }

    #[test]
    fn reed_output_is_bounded_across_effort_and_feedback() {
        let mut reed = ReedDriver::new(ReedConfig::default());
        for effort_step in 0..=10 {
            let effort = effort_step as f32 / 10.0;
            for feedback_step in -20..=20 {
                let feedback = feedback_step as f32 / 5.0;
                let out = reed.process(0.0, effort, feedback, 1.0);
                assert!(
                    out.is_finite() && out.abs() <= REED_OUTPUT_LIMIT,
                    "out={out}"
                );
            }
        }
    }
}
