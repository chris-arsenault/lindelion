use lindelion_dsp_utils::{
    delay::DelayLine,
    filters::{Biquad, BiquadCoefficients},
    math,
};

use super::core::{self, PositionTap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundarySide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BoundarySamples {
    pub left: f32,
    pub right: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PickupSamples {
    pub left: f32,
    pub right: f32,
}

impl PickupSamples {
    pub(super) fn average(self) -> f32 {
        (self.left + self.right) * 0.5
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TravelingWavePair {
    leftward: DelayLine,
    rightward: DelayLine,
}

impl TravelingWavePair {
    pub(super) fn new(sample_rate: f32, lowest_frequency_hz: f32, cycle_divisor: f32) -> Self {
        let max_delay = core::max_delay_samples(sample_rate, lowest_frequency_hz, cycle_divisor);
        Self {
            leftward: DelayLine::new(max_delay),
            rightward: DelayLine::new(max_delay),
        }
    }

    pub(super) fn capacity(&self) -> usize {
        self.leftward.capacity().min(self.rightward.capacity())
    }

    pub(super) fn clear(&mut self) {
        self.leftward.clear();
        self.rightward.clear();
    }

    pub(super) fn boundary_samples(&self, one_way_delay_samples: f32) -> BoundarySamples {
        BoundarySamples {
            left: self.leftward.read(one_way_delay_samples),
            right: self.rightward.read(one_way_delay_samples),
        }
    }

    pub(super) fn pickup_samples(
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

    pub(super) fn push(&mut self, leftward_sample: f32, rightward_sample: f32) {
        self.leftward.push(leftward_sample);
        self.rightward.push(rightward_sample);
    }

    pub(super) fn add_symmetric_excitation(
        &mut self,
        one_way_delay_samples: f32,
        excitation_taps: [PositionTap; 3],
        excitation: f32,
    ) {
        let excitation = math::snap_to_zero(excitation) * 0.5;
        for tap in excitation_taps {
            self.leftward.add_at(
                core::position_delay_samples(one_way_delay_samples, tap.position),
                math::snap_to_zero(excitation * tap.gain),
            );
            self.rightward.add_at(
                complementary_position_delay_samples(one_way_delay_samples, tap.position),
                math::snap_to_zero(excitation * tap.gain),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct BoundaryFilters {
    left: Biquad,
    right: Biquad,
}

impl BoundaryFilters {
    pub(super) fn new() -> Self {
        Self {
            left: Biquad::new(BiquadCoefficients::identity()),
            right: Biquad::new(BiquadCoefficients::identity()),
        }
    }

    pub(super) fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    pub(super) fn set_coefficients(&mut self, left: BiquadCoefficients, right: BiquadCoefficients) {
        self.left.set_coefficients(left);
        self.right.set_coefficients(right);
    }

    pub(super) fn process(&mut self, side: BoundarySide, input: f32) -> f32 {
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

    #[test]
    fn boundary_filter_pair_can_share_or_split_coefficients() {
        let mut filters = BoundaryFilters::new();
        filters.set_coefficients(
            BiquadCoefficients::identity(),
            BiquadCoefficients::identity(),
        );

        assert_eq!(filters.process(BoundarySide::Left, 0.25), 0.25);
        assert_eq!(filters.process(BoundarySide::Right, -0.5), -0.5);
    }
}
