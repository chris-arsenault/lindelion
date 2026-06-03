use lindelion_dsp_utils::{
    filters::OnePoleLowpass,
    math::{self, finite_clamp},
};

const PICK_MIN_CUTOFF_HZ: f32 = 300.0;
const PICK_MAX_CUTOFF_HZ: f32 = 12_000.0;
const BOW_STATIC_FRICTION: f32 = 0.9;
const BOW_DYNAMIC_FRICTION: f32 = 0.2;
const BOW_MAX_FORCE: f32 = 1.0;
const BOW_MIN_SPEED: f32 = 0.02;
const BOW_MAX_SPEED: f32 = 0.3;
const BOW_SMOOTH_SLIP_VELOCITY: f32 = 0.3;
const BOW_SHARP_SLIP_VELOCITY: f32 = 0.04;
const BOW_EXCITATION_COUPLING: f32 = 0.5;
const BOW_INJECTION_GAIN: f32 = 0.12;
const BOW_OUTPUT_LIMIT: f32 = 0.5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StringDriverMode {
    #[default]
    None,
    Pick,
    Bow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickParams {
    pub hardness: f32,
    pub contact_time: f32,
}

impl Default for PickParams {
    fn default() -> Self {
        Self {
            hardness: 0.55,
            contact_time: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BowParams {
    pub pressure_depth: f32,
    pub speed: f32,
    pub friction: f32,
}

impl Default for BowParams {
    fn default() -> Self {
        Self {
            pressure_depth: 0.45,
            speed: 0.45,
            friction: 0.45,
        }
    }
}

#[derive(Debug, Default)]
pub enum StringDriver {
    #[default]
    PassThrough,
    Pick(PickDriver),
    Bow(BowDriver),
}

impl StringDriver {
    pub fn from_mode(
        mode: StringDriverMode,
        pick: PickParams,
        bow: BowParams,
        sample_rate: f32,
    ) -> Self {
        match mode {
            StringDriverMode::None => Self::PassThrough,
            StringDriverMode::Pick => Self::Pick(PickDriver::new(pick, sample_rate)),
            StringDriverMode::Bow => Self::Bow(BowDriver::new(bow)),
        }
    }

    pub fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
        match self {
            Self::PassThrough => excitation,
            Self::Pick(pick) => pick.process(excitation, effort),
            Self::Bow(bow) => bow.process(excitation, effort, feedback, drive_gate),
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::PassThrough => {}
            Self::Pick(pick) => pick.reset(),
            Self::Bow(_) => {}
        }
    }
}

#[derive(Debug)]
pub struct PickDriver {
    contact: OnePoleLowpass,
    hardness: f32,
    contact_time: f32,
    sample_rate: f32,
}

impl PickDriver {
    pub fn new(params: PickParams, sample_rate: f32) -> Self {
        Self {
            contact: OnePoleLowpass::default(),
            hardness: finite_clamp(params.hardness, 0.0, 1.0, 0.5),
            contact_time: finite_clamp(params.contact_time, 0.0, 1.0, 0.5),
            sample_rate,
        }
    }

    fn process(&mut self, excitation: f32, effort: f32) -> f32 {
        let effort = finite_clamp(effort, 0.0, 1.0, 0.0);
        let brightness = self.hardness * (0.2 + 0.8 * effort);
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

#[derive(Debug)]
pub struct BowDriver {
    pressure_depth: f32,
    bow_speed: f32,
    slip_velocity: f32,
}

impl BowDriver {
    pub fn new(params: BowParams) -> Self {
        let pressure_depth = finite_clamp(params.pressure_depth, 0.0, 1.0, 0.5);
        let bow_speed = finite_clamp(params.speed, 0.0, 1.0, 0.5);
        let friction = finite_clamp(params.friction, 0.0, 1.0, 0.5);
        Self {
            pressure_depth,
            bow_speed: BOW_MIN_SPEED + (BOW_MAX_SPEED - BOW_MIN_SPEED) * bow_speed,
            slip_velocity: BOW_SMOOTH_SLIP_VELOCITY
                + (BOW_SHARP_SLIP_VELOCITY - BOW_SMOOTH_SLIP_VELOCITY) * friction,
        }
    }

    fn process(&mut self, excitation: f32, effort: f32, feedback: f32, drive_gate: f32) -> f32 {
        let effort = finite_clamp(effort, 0.0, 1.0, 0.0);
        let drive_gate = finite_clamp(drive_gate, 0.0, 1.0, 1.0);
        let relative_velocity = self.bow_speed - math::finite_or(feedback, 0.0);
        let friction = self.friction(relative_velocity);
        let force = BOW_MAX_FORCE * self.pressure_depth * effort * drive_gate;
        let bowed = friction * force * BOW_INJECTION_GAIN;
        let excitation = BOW_EXCITATION_COUPLING * math::snap_to_zero(excitation);
        finite_clamp(bowed + excitation, -BOW_OUTPUT_LIMIT, BOW_OUTPUT_LIMIT, 0.0)
    }

    fn friction(&self, relative_velocity: f32) -> f32 {
        let slip = self.slip_velocity.max(0.000_1);
        let speed = relative_velocity.abs();
        let mu = BOW_DYNAMIC_FRICTION
            + (BOW_STATIC_FRICTION - BOW_DYNAMIC_FRICTION) * (-(speed / slip)).exp();
        mu * relative_velocity.signum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bow_output_is_bounded_across_effort_and_feedback() {
        let mut bow = BowDriver::new(BowParams::default());
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
}
