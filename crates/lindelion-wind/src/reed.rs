use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math,
};

use super::DEFAULT_BIQUAD_Q;

const REED_PRESSURE_FLOOR: f32 = 0.60;
const REED_PRESSURE_CEIL: f32 = 0.76;
const REED_EFFORT_THRESHOLD: f32 = 0.1;
const REED_SUBTHRESHOLD_PRESSURE: f32 = 0.45;
const REED_CLOSING_PRESSURE: f32 = 1.2;
const REED_REST_OPENING: f32 = 1.0;
const REED_MAX_OPENING: f32 = 1.5;
const REED_FLOW_GAIN: f32 = 1.0;
const REED_FLOW_PRESSURE_REGULARIZATION: f32 = 0.36;
const REED_FEEDBACK_COUPLING: f32 = 1.0;
const REED_EXCITATION_COUPLING: f32 = 0.5;
const REED_OUTPUT_LIMIT: f32 = 1.5;
const REED_JUNCTION_ITERATIONS: usize = 8;
const REED_BREATH_NOISE: f32 = 0.02;
const REED_BREATH_RAMP_SECONDS: f32 = 0.004;
const REED_APERTURE_MIN_HZ: f32 = 1_050.0;
const REED_APERTURE_MAX_HZ: f32 = 3_200.0;
const REED_APERTURE_DAMPING: f32 = 0.92;
// Embouchure tracking: the aperture resonance never sits below this multiple of the tracked
// sounding frequency. The inertial aperture's phase lag at the played mode rises with pitch and
// the reed's energy pumping falls as cos(lag) — fixed-resonance margin crosses zero just above
// C5 and the upper register goes silent. A player firms the embouchure ascending the register
// (stiffer/lighter effective reed = higher aperture resonance); 3.0 pins the lag at its A4 value
// without touching notes at or below A4 (3x440 sits below the default resonance).
const REED_APERTURE_TRACK_RATIO: f32 = 3.0;
// Tracked-embouchure loop-phase residual. REED_LOOP_PHASE_COUPLING was fitted at the base
// aperture resonance; with the embouchure tracking lifting the resonance above base, the real
// loop lag falls faster than the coupled model and tracked notes play sharp. The residual maps
// onto a smooth bump in the lift ratio (ω_tracked/ω_base), peaking near lift 1.45 — measured
// 0.36/0.55/0.76/0.84/0.70 one-way samples at B4/C5/D5/E5/G5 — and is trimmed here with a
// warm-bore-style fitted curve `A·y·e^(1−y)`, `y = (lift−1)/(peak−1)` (in round-trip units,
// halved downstream by REED_PHASE_ONE_WAY_FACTOR like the rest of the comp).
const REED_TRACKED_PHASE_TRIM_SAMPLES: f32 = 3.5;
const REED_TRACKED_PHASE_TRIM_PEAK_LIFT: f32 = 1.45;
const REED_LOOP_PHASE_COUPLING: f32 = 1.25;
const REED_EFFORT_PHASE_SLOPE: f32 = 0.35;
// Untracked (low-register) loop-phase law, fitted against sustained-pitch measurement at the
// shipped default across 131-415 Hz and velocities 0.31/0.79/1.0 (two independent law settings
// cross-validated the fit; one compensation sample moves the quarter-wave bore's period by two
// samples, i.e. cents-per-sample = 3462*f_bore/sr). The legacy law saturated the tube's
// 24-sample compensation clamp across the whole low register, freezing the compensation at 24
// round-trip samples for every effort: velocity ~0.79 happened to land in tune (the auditioned
// renders), full velocity played up to +14 cents sharp, and soft playing sagged 17-50 cents
// flat. The measured requirement is nearly frequency-flat in absolute time — `coupling*phi`
// anchors effort 1.0 (phi =~ 19.4 samples at the 96 kHz fit rate, requirement =~ 22.75) and the
// soft-playing leverage is an absolute round-trip time slope, not a multiple of phi (6.23
// samples at 96 kHz; expressed in seconds so the law holds at any model rate).
const REED_UNTRACKED_PHASE_COUPLING: f32 = 1.166;
const REED_UNTRACKED_EFFORT_PHASE_SECONDS: f32 = 6.23 / 96_000.0;
// Pre-break sag correction: the last ~4 semitones of the *unvented* bore need steeply more
// compensation than the flat law (measured +0.30/+0.36/+0.45/+0.67/+0.92 round-trip samples at
// E4..G#4, i.e. the backlog's G4/G#4 -5/-8 cent sag). The rise fits a logistic in the
// sounding-to-aperture-resonance ratio r = f/f_aperture_base (the nonlinear coupling's leverage
// growing as the aperture lag angle steepens), saturating at 1.5 samples at the 96 kHz fit rate
// (expressed in seconds) so high configured break notes stay bounded. Vented notes sit on the
// bare law (A4's measured requirement has no sag), so the correction gates on the embouchure
// being untracked.
const REED_PRE_BREAK_SAG_SECONDS: f32 = 1.5 / 96_000.0;
const REED_PRE_BREAK_SAG_KNEE_RATIO: f32 = 0.289;
const REED_PRE_BREAK_SAG_WIDTH_RATIO: f32 = 0.034;
// The tracked-embouchure law (the fitted coupling/slope/trim above) was approved by audition
// with the vented register within +-2.4 cents; it takes over from the untracked law as the
// embouchure tracking lifts the aperture, fully by lift 1.15 (~C5 at the default break).
const REED_TRACKED_LAW_BLEND_LIFT: f32 = 0.15;
const REED_TURBULENCE_VELOCITY_THRESHOLD: f32 = 0.42;
const REED_TURBULENCE_VELOCITY_RANGE: f32 = 0.90;
const REED_TURBULENT_LOSS_CUTOFF_HZ: f32 = 2_800.0;
const REED_TURBULENT_LOSS_DEPTH: f32 = 0.70;
const REED_TURBULENCE_LOW_CUTOFF_HZ: f32 = 3_200.0;
const REED_TURBULENCE_HIGH_CUTOFF_HZ: f32 = 8_200.0;
const REED_TURBULENCE_NOISE_GAIN: f32 = 0.95;
const REED_TURBULENCE_PRESSURE_NOISE_GAIN: f32 = 0.045;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReedParams {
    pub pressure_depth: f32,
    pub stiffness: f32,
    pub embouchure: f32,
    pub aperture_inertia: f32,
    /// Scale on the in-loop breath-noise dither. The dither's job is dodging the full bore's
    /// period-2 lock at hard blowing in the low register; in the vented register it is not
    /// needed for stability and its loop-circulated white noise reads as a breath wash burying
    /// the weak voiced lines, so the host scales it toward `0.0` above the break.
    pub breath_noise: f32,
    /// Sounding frequency the embouchure tracks (Hz), or `0.0` for no tracking. Above the
    /// register break the host supplies the played pitch so the aperture resonance keeps
    /// [`REED_APERTURE_TRACK_RATIO`]x headroom over the mode and the reed's pumping gain stops
    /// collapsing with pitch.
    pub tracking_frequency_hz: f32,
}

impl Default for ReedParams {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            stiffness: 0.5,
            embouchure: 0.5,
            aperture_inertia: 1.0,
            breath_noise: 1.0,
            tracking_frequency_hz: 0.0,
        }
    }
}

impl ReedParams {
    pub fn sanitized(self) -> Self {
        let fallback = Self::default();
        Self {
            pressure_depth: unit(self.pressure_depth, fallback.pressure_depth),
            stiffness: unit(self.stiffness, fallback.stiffness),
            embouchure: unit(self.embouchure, fallback.embouchure),
            aperture_inertia: unit(self.aperture_inertia, fallback.aperture_inertia),
            breath_noise: unit(self.breath_noise, fallback.breath_noise),
            tracking_frequency_hz: math::finite_clamp(
                self.tracking_frequency_hz,
                0.0,
                22_000.0,
                fallback.tracking_frequency_hz,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReedProcessTaps {
    pub breath: f32,
    pub feedback: f32,
    pub delta_p: f32,
    pub aperture: f32,
    pub raw_flow: f32,
    pub source_flow: f32,
    pub output: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReedDriver {
    pressure_depth: f32,
    closing_pressure: f32,
    rest_opening: f32,
    flow: f32,
    aperture: f32,
    aperture_velocity: f32,
    aperture_inertia: f32,
    aperture_omega: f32,
    aperture_lift: f32,
    aperture_base_hz: f32,
    tracking_active: bool,
    breath: f32,
    breath_coeff: f32,
    breath_noise_scale: f32,
    coherent_flow: f32,
    coherent_output: f32,
    turbulent_flow_lowpass: Biquad,
    turbulence_highpass: Biquad,
    turbulence_lowpass: Biquad,
    noise: u32,
    sample_rate: f32,
}

impl ReedDriver {
    pub fn new(params: ReedParams, sample_rate: f32) -> Self {
        let params = params.sanitized();
        let sample_rate = sample_rate.max(1.0);
        Self {
            pressure_depth: params.pressure_depth,
            closing_pressure: REED_CLOSING_PRESSURE * (0.5 + params.stiffness),
            rest_opening: REED_REST_OPENING * (1.3 - 0.6 * params.embouchure),
            flow: 0.0,
            aperture: REED_REST_OPENING * (1.3 - 0.6 * params.embouchure),
            aperture_velocity: 0.0,
            aperture_inertia: params.aperture_inertia,
            aperture_omega: aperture_omega(params, sample_rate),
            aperture_lift: aperture_lift(params),
            aperture_base_hz: aperture_base_frequency_hz(params),
            tracking_active: params.tracking_frequency_hz > 0.0,
            breath: 0.0,
            breath_coeff: 1.0 - (-1.0 / (REED_BREATH_RAMP_SECONDS * sample_rate)).exp(),
            breath_noise_scale: params.breath_noise,
            coherent_flow: 0.0,
            coherent_output: 0.0,
            turbulent_flow_lowpass: Biquad::new(BiquadCoefficients::lowpass(
                sample_rate,
                REED_TURBULENT_LOSS_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            turbulence_highpass: Biquad::new(BiquadCoefficients::highpass(
                sample_rate,
                REED_TURBULENCE_LOW_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            turbulence_lowpass: Biquad::new(BiquadCoefficients::lowpass(
                sample_rate,
                REED_TURBULENCE_HIGH_CUTOFF_HZ,
                DEFAULT_BIQUAD_Q,
            )),
            noise: 0x9E37_79B9,
            sample_rate,
        }
    }

    pub fn set_params(&mut self, params: ReedParams) {
        let params = params.sanitized();
        self.pressure_depth = params.pressure_depth;
        self.closing_pressure = REED_CLOSING_PRESSURE * (0.5 + params.stiffness);
        self.rest_opening = REED_REST_OPENING * (1.3 - 0.6 * params.embouchure);
        self.aperture_inertia = params.aperture_inertia;
        self.aperture_omega = aperture_omega(params, self.sample_rate);
        self.aperture_lift = aperture_lift(params);
        self.aperture_base_hz = aperture_base_frequency_hz(params);
        self.tracking_active = params.tracking_frequency_hz > 0.0;
        self.aperture = self.aperture.clamp(0.0, REED_MAX_OPENING);
        self.breath_noise_scale = params.breath_noise;
    }

    pub fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
        self.process_inner(excitation, effort, feedback, drive_gate, None)
    }

    pub fn process_with_taps(
        &mut self,
        excitation: f32,
        effort: f32,
        feedback: f32,
        drive_gate: f32,
        taps: &mut ReedProcessTaps,
    ) -> f32 {
        self.process_inner(excitation, effort, feedback, drive_gate, Some(taps))
    }

    fn process_inner(
        &mut self,
        excitation: f32,
        effort: f32,
        feedback: f32,
        drive_gate: f32,
        taps: Option<&mut ReedProcessTaps>,
    ) -> f32 {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let drive_gate = math::finite_clamp(drive_gate, 0.0, 1.0, 1.0);
        let window = if effort <= REED_EFFORT_THRESHOLD {
            REED_SUBTHRESHOLD_PRESSURE * (effort / REED_EFFORT_THRESHOLD)
        } else {
            let above = (effort - REED_EFFORT_THRESHOLD) / (1.0 - REED_EFFORT_THRESHOLD);
            REED_PRESSURE_FLOOR + (REED_PRESSURE_CEIL - REED_PRESSURE_FLOOR) * above
        };
        let mouth_target = window * (self.pressure_depth * 2.0) * drive_gate;
        self.breath += (mouth_target - self.breath) * self.breath_coeff;
        let mouth = self.breath;
        let turbulence = REED_BREATH_NOISE * self.breath_noise_scale * mouth * self.next_noise();
        let breath = mouth + REED_EXCITATION_COUPLING * math::snap_to_zero(excitation) + turbulence;
        let p_minus = REED_FEEDBACK_COUPLING * math::finite_or(feedback, 0.0);

        let mut flow = self.flow;
        let mut delta_p = 0.0;
        for _ in 0..REED_JUNCTION_ITERATIONS {
            delta_p = breath - 2.0 * p_minus - REED_FLOW_GAIN * flow;
            flow = 0.5 * flow + 0.5 * self.reed_flow(delta_p);
        }
        self.update_aperture(delta_p);
        let (source_flow, coherent_flow) = self.turbulent_reed_flow(flow, delta_p);
        self.flow = source_flow;
        // The coherent (turbulence-free) copy of the source wave, for radiation paths that
        // should not re-emit the shed jet noise as raw hiss. The in-loop breath dither is
        // inseparable from the junction solve and stays in both copies (it is ~30 dB below the
        // shed turbulence).
        self.coherent_flow = coherent_flow;
        self.coherent_output = math::finite_clamp(
            p_minus + REED_FLOW_GAIN * coherent_flow,
            -REED_OUTPUT_LIMIT,
            REED_OUTPUT_LIMIT,
            0.0,
        );

        let output = p_minus + REED_FLOW_GAIN * source_flow;
        let output = math::finite_clamp(output, -REED_OUTPUT_LIMIT, REED_OUTPUT_LIMIT, 0.0);
        if let Some(taps) = taps {
            *taps = ReedProcessTaps {
                breath,
                feedback: p_minus,
                delta_p,
                aperture: self.aperture,
                raw_flow: flow,
                source_flow,
                output,
            };
        }
        output
    }

    /// The most recent coherent (turbulence-free) source flow.
    pub fn coherent_source_flow(&self) -> f32 {
        self.coherent_flow
    }

    /// The most recent reed output computed from the coherent (turbulence-free) source flow.
    pub fn coherent_output(&self) -> f32 {
        self.coherent_output
    }

    pub fn reset(&mut self) {
        self.flow = 0.0;
        self.aperture = self.rest_opening.clamp(0.0, REED_MAX_OPENING);
        self.aperture_velocity = 0.0;
        self.breath = 0.0;
        self.coherent_flow = 0.0;
        self.coherent_output = 0.0;
        self.turbulent_flow_lowpass.reset();
        self.turbulence_highpass.reset();
        self.turbulence_lowpass.reset();
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn reed_flow(&self, delta_p: f32) -> f32 {
        let opening = if self.aperture_inertia <= f32::EPSILON {
            self.target_aperture(delta_p)
        } else {
            self.aperture.clamp(0.0, REED_MAX_OPENING)
        };
        opening * regularized_bernoulli_flow(delta_p)
    }

    /// Returns `(source_flow, coherent_flow)`: the bore-injected flow with the shed-jet noise,
    /// and the same flow without the noise injection.
    fn turbulent_reed_flow(&mut self, flow: f32, delta_p: f32) -> (f32, f32) {
        let opening = self.aperture.clamp(0.04, REED_MAX_OPENING);
        let jet_velocity = (flow / opening).abs();
        let velocity_drive = ((jet_velocity - REED_TURBULENCE_VELOCITY_THRESHOLD)
            / REED_TURBULENCE_VELOCITY_RANGE)
            .clamp(0.0, 1.0);
        let pressure_drive = (delta_p.abs() / self.closing_pressure.max(0.1)).clamp(0.0, 1.0);
        let drive = (velocity_drive * pressure_drive).sqrt();
        if drive <= f32::EPSILON {
            return (flow, flow);
        }

        let low_flow = self.turbulent_flow_lowpass.process(flow);
        let high_flow = flow - low_flow;
        let coherent_loss = (REED_TURBULENT_LOSS_DEPTH * drive).clamp(0.0, 0.94);
        let shed_flow = high_flow * coherent_loss;
        let coherent_flow = flow - shed_flow;

        let shed_amplitude =
            shed_flow.abs() + REED_TURBULENCE_PRESSURE_NOISE_GAIN * drive * delta_p.abs().sqrt();
        let noise = self.next_noise() * REED_TURBULENCE_NOISE_GAIN * shed_amplitude;
        let noise = self.turbulence_highpass.process(noise);
        let noise = self.turbulence_lowpass.process(noise);
        (
            math::snap_to_zero(coherent_flow + noise),
            math::snap_to_zero(coherent_flow),
        )
    }

    fn update_aperture(&mut self, delta_p: f32) {
        if self.aperture_inertia <= f32::EPSILON {
            self.aperture = self.target_aperture(delta_p);
            self.aperture_velocity = 0.0;
            return;
        }

        let target = self.target_aperture(delta_p);
        let acceleration = self.aperture_omega * self.aperture_omega * (target - self.aperture)
            - 2.0 * REED_APERTURE_DAMPING * self.aperture_omega * self.aperture_velocity;
        self.aperture_velocity += acceleration;
        self.aperture += self.aperture_velocity;
        if self.aperture <= 0.0 || self.aperture >= REED_MAX_OPENING {
            self.aperture = self.aperture.clamp(0.0, REED_MAX_OPENING);
            self.aperture_velocity = 0.0;
        }
    }

    fn target_aperture(&self, delta_p: f32) -> f32 {
        (self.rest_opening - delta_p / self.closing_pressure).clamp(0.0, REED_MAX_OPENING)
    }

    /// Effective phase delay (in samples) that the finite-inertia aperture adds to the
    /// reed→bore feedback loop at `frequency_hz` and the current `effort`, for the bore-length
    /// tuning to subtract. The inertial aperture is a minimum-phase 2nd-order lowpass on the
    /// pressure→opening map, so it lags the wave it gates; that lag lengthens the effective
    /// acoustic loop and flattens pitch. Two factors lift the raw filter phase delay to the
    /// phase the loop actually sees:
    /// - `REED_LOOP_PHASE_COUPLING`: the lag reaches the loop through the nonlinear flow
    ///   product `aperture·sign(Δp)·√|Δp|`, not as a plain series filter, so its effect is
    ///   stronger than the bare aperture-filter phase delay (measured ≈1.25× against the
    ///   instant-aperture pitch across the register).
    /// - `REED_EFFORT_PHASE_SLOPE`: the `√|Δp|` flow has higher incremental gain at the small
    ///   pressure swings of quiet playing, giving the lagged aperture more leverage, so the
    ///   loop phase (and thus the flatness) grows as effort drops. Without this term, soft
    ///   notes play flat relative to loud ones.
    ///
    /// Returns `0.0` for the massless (instant) aperture, which adds no loop phase.
    ///
    /// Two fitted laws cover the two embouchure regimes, crossfaded over the tracking lift:
    /// - **Untracked** (low register, and vented notes whose 3x tracking target sits below the
    ///   base resonance): anchor coupling at full effort, an absolute round-trip sample slope
    ///   for the soft-playing leverage, plus the logistic pre-break sag correction.
    /// - **Tracked** (lift > 1): the audition-approved coupling/effort-slope/lift-trim law,
    ///   unchanged.
    pub fn aperture_phase_delay_samples(&self, frequency_hz: f32, effort: f32) -> f32 {
        if self.aperture_inertia <= f32::EPSILON {
            return 0.0;
        }
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let phi = aperture_filter_phase_delay(self.aperture_omega, self.sample_rate, frequency_hz);
        let sag = if self.tracking_active {
            0.0
        } else {
            pre_break_sag_samples(
                frequency_hz / self.aperture_base_hz.max(1.0),
                self.sample_rate,
            )
        };
        let untracked = REED_UNTRACKED_PHASE_COUPLING * phi
            + REED_UNTRACKED_EFFORT_PHASE_SECONDS * self.sample_rate * (1.0 - effort)
            + sag;
        let tracked =
            REED_LOOP_PHASE_COUPLING * (1.0 + REED_EFFORT_PHASE_SLOPE * (1.0 - effort)) * phi
                - tracked_phase_trim_samples(self.aperture_lift);
        let blend = ((math::finite_or(self.aperture_lift, 1.0) - 1.0)
            / REED_TRACKED_LAW_BLEND_LIFT)
            .clamp(0.0, 1.0);
        (untracked + (tracked - untracked) * blend).max(0.0)
    }
}

fn pre_break_sag_samples(ratio: f32, sample_rate: f32) -> f32 {
    let ratio = math::finite_or(ratio, 0.0);
    REED_PRE_BREAK_SAG_SECONDS * sample_rate
        / (1.0 + (-(ratio - REED_PRE_BREAK_SAG_KNEE_RATIO) / REED_PRE_BREAK_SAG_WIDTH_RATIO).exp())
}

fn tracked_phase_trim_samples(lift: f32) -> f32 {
    let lift = math::finite_or(lift, 1.0).max(1.0);
    let y = (lift - 1.0) / (REED_TRACKED_PHASE_TRIM_PEAK_LIFT - 1.0);
    REED_TRACKED_PHASE_TRIM_SAMPLES * y * (1.0 - y).exp()
}

fn aperture_lift(params: ReedParams) -> f32 {
    let base_hz = aperture_base_frequency_hz(params);
    let tracked_hz = base_hz.max(REED_APERTURE_TRACK_RATIO * params.tracking_frequency_hz.max(0.0));
    (tracked_hz / base_hz.max(1.0)).max(1.0)
}

fn aperture_filter_phase_delay(omega: f32, sample_rate: f32, frequency_hz: f32) -> f32 {
    let theta = std::f32::consts::TAU * frequency_hz / sample_rate.max(1.0);
    if theta <= f32::EPSILON {
        return 0.0;
    }
    // Discrete aperture resonator (symplectic Euler, see `update_aperture`):
    //   a[n] = a1·a[n-1] − a2·a[n-2] + ω²·u[n],
    //   a1 = 2 − 2ζω − ω²,  a2 = 1 − 2ζω.
    // The numerator (ω²) is real, so the loop phase the aperture contributes is the phase of
    // the denominator evaluated at z = e^{jθ}; phase delay is that phase divided by θ.
    let a1 = 2.0 - 2.0 * REED_APERTURE_DAMPING * omega - omega * omega;
    let a2 = 1.0 - 2.0 * REED_APERTURE_DAMPING * omega;
    let (sin1, cos1) = theta.sin_cos();
    let (sin2, cos2) = (2.0 * theta).sin_cos();
    let re = 1.0 - a1 * cos1 + a2 * cos2;
    let im = a1 * sin1 - a2 * sin2;
    math::finite_clamp(im.atan2(re) / theta, 0.0, 24.0, 0.0)
}

fn aperture_base_frequency_hz(params: ReedParams) -> f32 {
    REED_APERTURE_MAX_HZ - (REED_APERTURE_MAX_HZ - REED_APERTURE_MIN_HZ) * params.aperture_inertia
        + 650.0 * params.stiffness
}

fn aperture_omega(params: ReedParams, sample_rate: f32) -> f32 {
    let frequency_hz = aperture_base_frequency_hz(params)
        .max(REED_APERTURE_TRACK_RATIO * params.tracking_frequency_hz.max(0.0));
    (std::f32::consts::TAU * frequency_hz / sample_rate.max(1.0)).clamp(0.0, 0.45)
}

fn regularized_bernoulli_flow(delta_p: f32) -> f32 {
    delta_p / (delta_p.abs() + REED_FLOW_PRESSURE_REGULARIZATION).sqrt()
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reed_driver_stays_finite_at_full_effort() {
        let mut reed = ReedDriver::new(ReedParams::default(), 48_000.0);
        for _ in 0..512 {
            let sample = reed.process(0.0, 1.0, 0.0, 1.0);
            assert!(sample.is_finite());
            assert!(sample.abs() <= REED_OUTPUT_LIMIT);
        }
    }
}
