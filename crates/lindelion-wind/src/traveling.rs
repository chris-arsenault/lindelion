use lindelion_dsp_utils::{
    delay::DelayLine,
    filters::{Biquad, BiquadCoefficients},
};

use super::core;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundarySide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundarySamples {
    pub left: f32,
    pub right: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickupSamples {
    pub left: f32,
    pub right: f32,
}

impl PickupSamples {
    pub fn average(self) -> f32 {
        (self.left + self.right) * 0.5
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JunctionSamples {
    pub from_bell: f32,
    pub from_mouth: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TravelingWavePair {
    leftward: DelayLine,
    rightward: DelayLine,
}

impl TravelingWavePair {
    pub fn new(sample_rate: f32, lowest_frequency_hz: f32, cycle_divisor: f32) -> Self {
        let max_delay = core::max_delay_samples(sample_rate, lowest_frequency_hz, cycle_divisor);
        Self {
            leftward: DelayLine::new(max_delay),
            rightward: DelayLine::new(max_delay),
        }
    }

    pub fn capacity(&self) -> usize {
        self.leftward.capacity().min(self.rightward.capacity())
    }

    pub fn clear(&mut self) {
        self.leftward.clear();
        self.rightward.clear();
    }

    pub fn boundary_samples(&self, one_way_delay_samples: f32) -> BoundarySamples {
        BoundarySamples {
            left: self.leftward.read(one_way_delay_samples),
            right: self.rightward.read(one_way_delay_samples),
        }
    }

    pub fn pickup_samples(
        &self,
        one_way_delay_samples: f32,
        pickup_position: f32,
    ) -> PickupSamples {
        PickupSamples {
            left: self.leftward.read(complementary_position_delay_samples(
                one_way_delay_samples,
                pickup_position,
            )),
            right: self.rightward.read(core::position_delay_samples(
                one_way_delay_samples,
                pickup_position,
            )),
        }
    }

    pub fn junction_samples(&self, one_way_delay_samples: f32, position: f32) -> JunctionSamples {
        JunctionSamples {
            from_bell: self.leftward.read(complementary_position_delay_samples(
                one_way_delay_samples,
                position,
            )),
            from_mouth: self.rightward.read(core::position_delay_samples(
                one_way_delay_samples,
                position,
            )),
        }
    }

    pub fn add_junction_correction(
        &mut self,
        one_way_delay_samples: f32,
        position: f32,
        correction: f32,
    ) {
        self.leftward.add_at(
            complementary_position_delay_samples(one_way_delay_samples, position),
            correction,
        );
        self.rightward.add_at(
            core::position_delay_samples(one_way_delay_samples, position),
            correction,
        );
    }

    pub fn push(&mut self, leftward_sample: f32, rightward_sample: f32) {
        self.leftward.push(leftward_sample);
        self.rightward.push(rightward_sample);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryFilters {
    left: Biquad,
    right: Biquad,
}

impl BoundaryFilters {
    pub fn new() -> Self {
        Self {
            left: Biquad::new(BiquadCoefficients::identity()),
            right: Biquad::new(BiquadCoefficients::identity()),
        }
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    pub fn set_coefficients(&mut self, left: BiquadCoefficients, right: BiquadCoefficients) {
        self.left.set_coefficients(left);
        self.right.set_coefficients(right);
    }

    pub fn process(&mut self, side: BoundarySide, input: f32) -> f32 {
        match side {
            BoundarySide::Left => self.left.process(input),
            BoundarySide::Right => self.right.process(input),
        }
    }
}

fn complementary_position_delay_samples(loop_delay_samples: f32, position: f32) -> f32 {
    core::position_delay_samples(loop_delay_samples, 1.0 - position)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traveling_wave_pair_maps_boundaries_and_pickup_positions() {
        let mut waves = TravelingWavePair::new(48_000.0, 20.0, 2.0);
        waves.push(1.0, -1.0);

        let boundary = waves.boundary_samples(0.0);
        let pickup = waves.pickup_samples(0.0, 0.75);

        assert_eq!(boundary.left, 1.0);
        assert_eq!(boundary.right, -1.0);
        assert_eq!(pickup.average(), 0.0);
    }
}
