use lindelion_dsp_utils::math;

const REED_PRESSURE_FLOOR: f32 = 0.60;
const REED_PRESSURE_CEIL: f32 = 0.76;
const REED_EFFORT_THRESHOLD: f32 = 0.1;
const REED_SUBTHRESHOLD_PRESSURE: f32 = 0.45;
const REED_CLOSING_PRESSURE: f32 = 1.2;
const REED_REST_OPENING: f32 = 1.0;
const REED_MAX_OPENING: f32 = 1.5;
const REED_FLOW_GAIN: f32 = 1.0;
const REED_FEEDBACK_COUPLING: f32 = 1.0;
const REED_EXCITATION_COUPLING: f32 = 0.5;
const REED_OUTPUT_LIMIT: f32 = 1.5;
const REED_JUNCTION_ITERATIONS: usize = 8;
const REED_BREATH_NOISE: f32 = 0.02;
const REED_BREATH_RAMP_SECONDS: f32 = 0.004;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReedParams {
    pub pressure_depth: f32,
    pub stiffness: f32,
    pub embouchure: f32,
}

impl Default for ReedParams {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            stiffness: 0.5,
            embouchure: 0.5,
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
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReedDriver {
    pressure_depth: f32,
    closing_pressure: f32,
    rest_opening: f32,
    flow: f32,
    breath: f32,
    breath_coeff: f32,
    noise: u32,
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
            breath: 0.0,
            breath_coeff: 1.0 - (-1.0 / (REED_BREATH_RAMP_SECONDS * sample_rate)).exp(),
            noise: 0x9E37_79B9,
        }
    }

    pub fn set_params(&mut self, params: ReedParams) {
        let params = params.sanitized();
        self.pressure_depth = params.pressure_depth;
        self.closing_pressure = REED_CLOSING_PRESSURE * (0.5 + params.stiffness);
        self.rest_opening = REED_REST_OPENING * (1.3 - 0.6 * params.embouchure);
    }

    pub fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
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
        for _ in 0..REED_JUNCTION_ITERATIONS {
            let delta_p = breath - 2.0 * p_minus - REED_FLOW_GAIN * flow;
            flow = 0.5 * flow + 0.5 * self.reed_flow(delta_p);
        }
        self.flow = flow;

        let output = p_minus + REED_FLOW_GAIN * flow;
        math::finite_clamp(output, -REED_OUTPUT_LIMIT, REED_OUTPUT_LIMIT, 0.0)
    }

    pub fn reset(&mut self) {
        self.flow = 0.0;
        self.breath = 0.0;
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
        let opening =
            (self.rest_opening - delta_p / self.closing_pressure).clamp(0.0, REED_MAX_OPENING);
        opening * delta_p.signum() * delta_p.abs().sqrt()
    }
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
