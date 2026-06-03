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

// Beating single-reed valve (Schumacher / Smith PASP). The reed is a pressure-driven
// valve whose *opening* shrinks to zero as the blowing pressure difference rises to a
// closing pressure (the reed beats shut), and the air *flow* through that opening
// follows the orifice (Bernoulli) square-root law. The product — opening (falling) times
// √Δp (rising) — is the hump-shaped flow characteristic whose falling side both sustains
// and self-limits the bore oscillation at its tuned fundamental.

/// The reed's usable mouth-pressure window at the nominal pressure depth: from just above the
/// oscillation threshold (`FLOOR`, softest playing) to a hard fortissimo blow (`CEIL`, full
/// effort), in units where the reed beats fully shut at the closing pressure (default 1.2).
/// ADR-0032 item B: measurement showed the bore holds its fundamental across the low/mid
/// register at any blowing pressure — the earlier "overblow above CEIL" was an autocorrelation
/// octave artifact on the odd-harmonic spectrum, not a real instability. Soft → γ≈0.5 (quiet,
/// mellow), loud → γ≈0.63 (louder), well below the closing pressure so the reed never chokes.
/// The dynamic range itself stays modest (the beating-reed limit-cycle amplitude saturates ~10
/// dB, which is physically correct for a wind voice); the *primary* dynamic — brightness with
/// effort (the cuivré) — rides on top via the effort-referenced bore steepening, since the
/// reed's own spectrum is nearly blowing-pressure-invariant. A wider ceiling (tested to γ≈0.71)
/// bought only ~2 dB more level at the cost of a brightness droop at the top of the mid register
/// and a louder portamento transient, so the window stays narrow. The top octave genuinely
/// period-doubles (squeaks) when overblown at fortissimo — real reed behaviour, kept on purpose,
/// not clamped away (the player blows the altissimo gently).
const REED_PRESSURE_FLOOR: f32 = 0.60;
const REED_PRESSURE_CEIL: f32 = 0.76;
/// Effort below this leaves the reed below its oscillation pressure (breathy near-silence);
/// the audible velocity range lives above it, mapped into the stable window above.
const REED_EFFORT_THRESHOLD: f32 = 0.1;
/// Mouth pressure at the top of the sub-threshold ramp — kept below the oscillation point so
/// very soft playing stays silent, then the window jumps in above the threshold.
const REED_SUBTHRESHOLD_PRESSURE: f32 = 0.45;
/// Blowing pressure difference at which the reed beats fully shut at the default
/// embouchure/stiffness. Stiffness raises it (a stiffer reed needs more blow to close,
/// so it stays open and bright longer); embouchure lowers the rest opening.
const REED_CLOSING_PRESSURE: f32 = 1.2;
/// Reed channel rest opening (fully open = 1) at the default embouchure.
const REED_REST_OPENING: f32 = 1.0;
/// How far the reed can be pushed *open* on negative Δp (suction) past its rest opening.
const REED_MAX_OPENING: f32 = 1.5;
/// Bore characteristic-impedance scaling of the reed flow into the outgoing bore wave.
/// Near unity: the reed is a flow source feeding the (normalized) bore, no brute-force
/// injection gain — the hump's falling side, not a clamp, bounds the limit cycle.
const REED_FLOW_GAIN: f32 = 1.0;
/// Coupling of the bore's returning wave into the mouthpiece pressure the reed senses.
const REED_FEEDBACK_COUPLING: f32 = 1.0;
/// How strongly the incoming sample/sidechain excitation perturbs the breath (lets a
/// note-on transient kick-start the oscillation; the sustained drive is the breath).
const REED_EXCITATION_COUPLING: f32 = 0.5;
/// Hard safety clamp on the reed output — a backstop only; the reed flow bounds itself.
const REED_OUTPUT_LIMIT: f32 = 1.5;
/// Damped fixed-point iterations that solve the implicit reed scattering junction each
/// sample so the reed feels the mouthpiece pressure it creates (a consistent junction,
/// no extra loop delay). A handful converges for the bounded hump flow.
const REED_JUNCTION_ITERATIONS: usize = 8;
/// Breath turbulence: a little flow-noise on the mouth pressure (scaled by it, so it grows
/// with blowing). Physically real (the breath sound) and it dithers the drive just enough to
/// break the exact period-2 sub-harmonic lock a hard-blown reed can fall into — a musically
/// sound small imperfection rather than a sterile lock.
const REED_BREATH_NOISE: f32 = 0.02;
/// Breath-onset ramp time: the mouth pressure rises to its target over this long on a note
/// onset, so the reed speaks without a click (and a slurred note change glides in level).
const REED_BREATH_RAMP_SECONDS: f32 = 0.004;

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
            DriverConfig::Reed(reed) => Self::Reed(ReedDriver::new(reed, sample_rate)),
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

    /// A self-oscillating *wind* driver (the reed) **terminates the bore mouth**: its output
    /// is the mouth-scattered wave that replaces the bore's passive mouth reflection, not a
    /// strike-position excitation. The struck/pick/bow drivers inject at the strike position
    /// instead, so the bore keeps its own boundary.
    pub(super) fn terminates_boundary(&self) -> bool {
        matches!(self, Self::Reed(_))
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
    /// Blowing pressure difference at which the reed beats shut (from stiffness).
    closing_pressure: f32,
    /// Reed channel rest opening (from embouchure).
    rest_opening: f32,
    /// Last solved reed flow — seeds the junction solve for fast convergence.
    flow: f32,
    /// Smoothed mouth pressure — a one-pole breath envelope so the breath ramps in over a few
    /// ms on a note onset (a real reed can't blow instantly) instead of stepping, which would
    /// click. Also damps any control-rate breath jump.
    breath: f32,
    /// One-pole coefficient for the breath ramp (derived from the oversampled rate).
    breath_coeff: f32,
    /// Deterministic breath-turbulence PRNG state (xorshift32).
    noise: u32,
}

impl ReedDriver {
    fn new(config: ReedConfig, sample_rate: f32) -> Self {
        let stiffness = math::finite_clamp(config.stiffness, 0.0, 1.0, 0.5);
        let embouchure = math::finite_clamp(config.embouchure, 0.0, 1.0, 0.5);
        let sample_rate = sample_rate.max(1.0);
        Self {
            pressure_depth: math::finite_clamp(config.pressure_depth, 0.0, 1.0, 0.5),
            // Stiffer reed closes at a higher blowing pressure (0.5x..1.5x the base), so
            // it stays open — and bright — over a wider dynamic range before beating shut.
            closing_pressure: REED_CLOSING_PRESSURE * (0.5 + stiffness),
            // A tighter embouchure (higher control) narrows the rest opening.
            rest_opening: REED_REST_OPENING * (1.3 - 0.6 * embouchure),
            flow: 0.0,
            breath: 0.0,
            // ~4 ms one-pole breath ramp at the (oversampled) sample rate.
            breath_coeff: 1.0 - (-1.0 / (REED_BREATH_RAMP_SECONDS * sample_rate)).exp(),
            noise: 0x9E37_79B9,
        }
    }

    /// One deterministic breath-turbulence sample in `[-1, 1]` (xorshift32). Deterministic
    /// so offline and realtime renders stay bit-identical (the render-stability contract).
    fn next_noise(&mut self) -> f32 {
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let drive_gate = math::finite_clamp(drive_gate, 0.0, 1.0, 1.0);
        // Map playing effort to mouth pressure. Below a small threshold the reed stays under
        // its oscillation pressure — a breathy near-silence (a real reed needs a minimum
        // blow). Above the threshold the playing range maps into the reed's usable window
        // (FLOOR → CEIL), which *skips* the near-threshold zone where the bore would
        // period-double, so the whole audible velocity range stays on the fundamental. Scaled
        // by the patch pressure depth (0.5 = nominal). Note-off releases the gate to silence.
        let window = if effort <= REED_EFFORT_THRESHOLD {
            REED_SUBTHRESHOLD_PRESSURE * (effort / REED_EFFORT_THRESHOLD)
        } else {
            let above = (effort - REED_EFFORT_THRESHOLD) / (1.0 - REED_EFFORT_THRESHOLD);
            REED_PRESSURE_FLOOR + (REED_PRESSURE_CEIL - REED_PRESSURE_FLOOR) * above
        };
        let mouth_target = window * (self.pressure_depth * 2.0) * drive_gate;
        // Ramp the mouth pressure toward its target (a real reed can't blow instantly): this
        // removes the onset step that clicks, and makes a slurred note glide in level.
        self.breath += (mouth_target - self.breath) * self.breath_coeff;
        let mouth = self.breath;
        // Breath = ramped mouth pressure + the note-on excitation kick + flow turbulence (scaled
        // by the mouth pressure so it grows with blowing).
        let turbulence = REED_BREATH_NOISE * mouth * self.next_noise();
        let breath = mouth + REED_EXCITATION_COUPLING * math::snap_to_zero(excitation) + turbulence;
        // Incoming bore wave at the mouthpiece (p_minus).
        let p_minus = REED_FEEDBACK_COUPLING * math::finite_or(feedback, 0.0);
        // Solve the reed scattering junction (Smith PASP): mouthpiece pressure is
        // p = p_plus + p_minus = 2·p_minus + Z·u, so the pressure across the reed,
        // Δp = breath − p, depends on the very flow u it produces — an implicit equation
        // u = g(breath − 2·p_minus − Z·u). Solve by damped fixed-point so the reed feels
        // the pressure it creates; without it the junction is ~50% inconsistent, which
        // detunes the loop and period-doubles. g (`reed_flow`) is the beating-reed flow.
        let mut flow = self.flow;
        for _ in 0..REED_JUNCTION_ITERATIONS {
            let delta_p = breath - 2.0 * p_minus - REED_FLOW_GAIN * flow;
            flow = 0.5 * flow + 0.5 * self.reed_flow(delta_p);
        }
        self.flow = flow;
        // Outgoing bore wave = incoming wave + characteristic-impedance · reed flow.
        let output = p_minus + REED_FLOW_GAIN * flow;
        math::finite_clamp(output, -REED_OUTPUT_LIMIT, REED_OUTPUT_LIMIT, 0.0)
    }

    /// Beating-reed volume flow vs the pressure difference across the reed: the opening
    /// closes linearly to zero as the blowing Δp reaches the closing pressure (and stays
    /// shut beyond it; suction past the rest opening is bounded), and the air flows through
    /// that opening by the orifice (Bernoulli) signed-√ law. The product is the hump whose
    /// falling side both sustains and bounds the oscillation.
    fn reed_flow(&self, delta_p: f32) -> f32 {
        let opening =
            (self.rest_opening - delta_p / self.closing_pressure).clamp(0.0, REED_MAX_OPENING);
        opening * delta_p.signum() * delta_p.abs().sqrt()
    }

    fn reset(&mut self) {
        self.flow = 0.0;
        self.breath = 0.0;
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
        let mut reed = ReedDriver::new(ReedConfig::default(), 96_000.0);
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
