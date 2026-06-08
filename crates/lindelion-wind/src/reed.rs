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
const REED_LOOP_PHASE_COUPLING: f32 = 1.25;
const REED_EFFORT_PHASE_SLOPE: f32 = 0.35;
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
}

impl Default for ReedParams {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            stiffness: 0.5,
            embouchure: 0.5,
            aperture_inertia: 1.0,
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
    breath: f32,
    breath_coeff: f32,
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
            breath: 0.0,
            breath_coeff: 1.0 - (-1.0 / (REED_BREATH_RAMP_SECONDS * sample_rate)).exp(),
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
        self.aperture = self.aperture.clamp(0.0, REED_MAX_OPENING);
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
        let turbulence = REED_BREATH_NOISE * mouth * self.next_noise();
        let breath = mouth + REED_EXCITATION_COUPLING * math::snap_to_zero(excitation) + turbulence;
        let p_minus = REED_FEEDBACK_COUPLING * math::finite_or(feedback, 0.0);

        let mut flow = self.flow;
        let mut delta_p = 0.0;
        for _ in 0..REED_JUNCTION_ITERATIONS {
            delta_p = breath - 2.0 * p_minus - REED_FLOW_GAIN * flow;
            flow = 0.5 * flow + 0.5 * self.reed_flow(delta_p);
        }
        self.update_aperture(delta_p);
        let source_flow = self.turbulent_reed_flow(flow, delta_p);
        self.flow = source_flow;

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

    pub fn reset(&mut self) {
        self.flow = 0.0;
        self.aperture = self.rest_opening.clamp(0.0, REED_MAX_OPENING);
        self.aperture_velocity = 0.0;
        self.breath = 0.0;
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

    fn turbulent_reed_flow(&mut self, flow: f32, delta_p: f32) -> f32 {
        let opening = self.aperture.clamp(0.04, REED_MAX_OPENING);
        let jet_velocity = (flow / opening).abs();
        let velocity_drive = ((jet_velocity - REED_TURBULENCE_VELOCITY_THRESHOLD)
            / REED_TURBULENCE_VELOCITY_RANGE)
            .clamp(0.0, 1.0);
        let pressure_drive = (delta_p.abs() / self.closing_pressure.max(0.1)).clamp(0.0, 1.0);
        let drive = (velocity_drive * pressure_drive).sqrt();
        if drive <= f32::EPSILON {
            return flow;
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
        math::snap_to_zero(coherent_flow + noise)
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
    pub fn aperture_phase_delay_samples(&self, frequency_hz: f32, effort: f32) -> f32 {
        if self.aperture_inertia <= f32::EPSILON {
            return 0.0;
        }
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        let coupling = REED_LOOP_PHASE_COUPLING * (1.0 + REED_EFFORT_PHASE_SLOPE * (1.0 - effort));
        coupling * aperture_filter_phase_delay(self.aperture_omega, self.sample_rate, frequency_hz)
    }
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

fn aperture_omega(params: ReedParams, sample_rate: f32) -> f32 {
    let frequency_hz = REED_APERTURE_MAX_HZ
        - (REED_APERTURE_MAX_HZ - REED_APERTURE_MIN_HZ) * params.aperture_inertia
        + 650.0 * params.stiffness;
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
