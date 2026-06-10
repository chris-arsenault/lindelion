use lindelion_dsp_utils::{filters::OnePoleLowpass, math::finite_clamp};

use crate::core;

const PICK_MIN_CUTOFF_HZ: f32 = 300.0;
const PICK_MAX_CUTOFF_HZ: f32 = 12_000.0;
/// Time constant of a bow-stroke change: the arm decelerates through zero and
/// accelerates the other way over tens of milliseconds. A fresh stroke starts
/// from rest (state snapped to 0), so a rearticulated note is the bow
/// accelerating onto the string in the new direction; the gate and this ramp
/// together shape the attack.
const BOW_STROKE_CHANGE_SECONDS: f32 = 0.025;

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
    pub position: f32,
    pub pressure: f32,
    pub speed: f32,
    pub friction: f32,
}

impl Default for BowParams {
    fn default() -> Self {
        Self {
            position: 0.12,
            pressure: 0.48,
            speed: 0.45,
            friction: 0.45,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BowContactDrive {
    pub params: BowParams,
    pub effort: f32,
    pub drive_gate: f32,
    /// Signed stroke direction in `[-1, 1]`, smoothed through bow changes;
    /// scales the bow velocity (down-bow positive, up-bow negative).
    pub stroke: f32,
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
            StringDriverMode::Bow => Self::Bow(BowDriver::new(bow, sample_rate)),
        }
    }

    pub fn set_params(&mut self, pick: PickParams, bow: BowParams) {
        match self {
            Self::PassThrough => {}
            Self::Pick(driver) => driver.set_params(pick),
            Self::Bow(driver) => driver.set_params(bow),
        }
    }

    pub fn process(
        &mut self,
        excitation: f32,
        effort: f32,
        _feedback: f32,
        _drive_gate: f32,
    ) -> f32 {
        self.process_excitation(excitation, effort)
    }

    pub fn process_excitation(&mut self, excitation: f32, effort: f32) -> f32 {
        match self {
            Self::PassThrough => excitation,
            Self::Pick(pick) => pick.process(excitation, effort),
            Self::Bow(_) => 0.0,
        }
    }

    /// Produce the bow-contact descriptor. The offset arguments are per-sample
    /// humanization drifts added to the smoothed parameter targets (position in
    /// absolute speaking-length fraction, speed/pressure in the unit parameter
    /// range); pass zeros for a machine-steady bow.
    pub fn bow_contact(
        &mut self,
        effort: f32,
        drive_gate: f32,
        position_offset: f32,
        speed_offset: f32,
        pressure_offset: f32,
    ) -> Option<BowContactDrive> {
        match self {
            Self::Bow(bow) => Some(bow.contact(
                effort,
                drive_gate,
                position_offset,
                speed_offset,
                pressure_offset,
            )),
            Self::PassThrough | Self::Pick(_) => None,
        }
    }

    /// Begin a fresh bow stroke in the opposite direction (rearticulation).
    /// Legato note changes do not call this — the stroke continues. No-op for
    /// non-bow drivers.
    pub fn flip_bow_stroke(&mut self) {
        if let Self::Bow(bow) = self {
            bow.flip_stroke();
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::PassThrough => {}
            Self::Pick(pick) => pick.reset(),
            Self::Bow(bow) => bow.reset(),
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

    fn set_params(&mut self, params: PickParams) {
        self.hardness = finite_clamp(params.hardness, 0.0, 1.0, 0.5);
        self.contact_time = finite_clamp(params.contact_time, 0.0, 1.0, 0.5);
    }

    fn reset(&mut self) {
        self.contact.reset();
    }
}

#[derive(Debug)]
pub struct BowDriver {
    target: BowParams,
    position: core::ScalarSmoother,
    pressure: core::ScalarSmoother,
    speed: core::ScalarSmoother,
    friction: core::ScalarSmoother,
    stroke_target: f32,
    stroke: f32,
    stroke_coefficient: f32,
}

impl BowDriver {
    pub fn new(params: BowParams, sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            target: sanitize_bow_params(params),
            position: core::ScalarSmoother::new(sample_rate),
            pressure: core::ScalarSmoother::new(sample_rate),
            speed: core::ScalarSmoother::new(sample_rate),
            friction: core::ScalarSmoother::new(sample_rate),
            stroke_target: 1.0,
            stroke: 1.0,
            stroke_coefficient: finite_clamp(
                1.0 / (BOW_STROKE_CHANGE_SECONDS * sample_rate),
                0.0,
                1.0,
                1.0,
            ),
        }
    }

    fn set_params(&mut self, params: BowParams) {
        self.target = sanitize_bow_params(params);
    }

    fn flip_stroke(&mut self) {
        // A fresh stroke begins from rest in the opposite direction.
        self.stroke_target = -self.stroke_target.signum();
        self.stroke = 0.0;
    }

    fn contact(
        &mut self,
        effort: f32,
        drive_gate: f32,
        position_offset: f32,
        speed_offset: f32,
        pressure_offset: f32,
    ) -> BowContactDrive {
        // Humanize offsets ride on top of the smoothed targets (the walks are
        // already slow and smooth); zeros leave the values bit-identical.
        let params = BowParams {
            position: finite_clamp(
                self.position.next(self.target.position) + position_offset,
                0.001,
                0.999,
                self.target.position,
            ),
            pressure: finite_clamp(
                self.pressure.next(self.target.pressure) + pressure_offset,
                0.0,
                1.0,
                self.target.pressure,
            ),
            speed: finite_clamp(
                self.speed.next(self.target.speed) + speed_offset,
                0.0,
                1.0,
                self.target.speed,
            ),
            friction: self.friction.next(self.target.friction),
        };
        self.stroke += self.stroke_coefficient * (self.stroke_target - self.stroke);
        BowContactDrive {
            params,
            effort: finite_clamp(effort, 0.0, 1.0, 0.0),
            drive_gate: finite_clamp(drive_gate, 0.0, 1.0, 1.0),
            stroke: finite_clamp(self.stroke, -1.0, 1.0, 1.0),
        }
    }

    fn reset(&mut self) {
        self.position.reset();
        self.pressure.reset();
        self.speed.reset();
        self.friction.reset();
        self.stroke_target = 1.0;
        self.stroke = 1.0;
    }
}

fn sanitize_bow_params(params: BowParams) -> BowParams {
    let fallback = BowParams::default();
    BowParams {
        position: finite_clamp(params.position, 0.001, 0.999, fallback.position),
        pressure: finite_clamp(params.pressure, 0.0, 1.0, fallback.pressure),
        speed: finite_clamp(params.speed, 0.0, 1.0, fallback.speed),
        friction: finite_clamp(params.friction, 0.0, 1.0, fallback.friction),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_flip_starts_from_rest_and_ramps_to_the_opposite_direction() {
        let mut bow = BowDriver::new(BowParams::default(), 48_000.0);
        assert_eq!(bow.contact(0.8, 1.0, 0.0, 0.0, 0.0).stroke, 1.0);

        bow.flip_stroke();
        let first = bow.contact(0.8, 1.0, 0.0, 0.0, 0.0).stroke;
        assert!(
            first.abs() < 0.01,
            "fresh stroke should start near rest: {first}"
        );
        let mut last = first;
        let mut crossed_in = 0;
        for sample in 1..4_800 {
            let stroke = bow.contact(0.8, 1.0, 0.0, 0.0, 0.0).stroke;
            assert!(stroke <= last + 1.0e-6, "stroke should ramp monotonically");
            if crossed_in == 0 && stroke < -0.9 {
                crossed_in = sample;
            }
            last = stroke;
        }
        assert!(last < -0.95, "stroke should settle toward -1: {last}");
        assert!(
            crossed_in > 48,
            "stroke change should take milliseconds, not samples: {crossed_in}"
        );
    }

    #[test]
    fn bow_contact_descriptor_sanitizes_effort_gate_and_params() {
        let mut bow = BowDriver::new(BowParams::default(), 48_000.0);
        for effort_step in 0..=10 {
            let effort = effort_step as f32 / 10.0;
            let contact = bow.contact(effort, 1.0, 0.0, 0.0, 0.0);
            assert!(contact.effort.is_finite());
            assert!(contact.drive_gate.is_finite());
            assert_eq!(contact.params, sanitize_bow_params(BowParams::default()));
        }
    }
}
