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

    /// Windowed read of the two rails **upstream** of the contact: each tap
    /// delay is reduced by `delay_advance` samples, so the cells read are the
    /// incoming waves that will arrive at the contact `delay_advance` samples
    /// from now. Contact writes only ever land downstream (their delay grows
    /// every sample), so an upstream read can never observe the contact's own
    /// outgoing corrections — the caller delays the result by `delay_advance`
    /// to obtain the uncontaminated incoming wave at the contact "now".
    pub(super) fn contact_average_samples(
        &self,
        one_way_delay_samples: f32,
        contact_position: f32,
        half_width: f32,
        delay_advance: f32,
    ) -> PickupSamples {
        let contact_position = math::finite_clamp(contact_position, 0.001, 0.999, 0.5);
        let half_width = math::finite_or(half_width, 0.0).max(0.0);
        let delay_advance = math::finite_or(delay_advance, 0.0).max(0.0);
        let mut averaged = PickupSamples {
            left: 0.0,
            right: 0.0,
        };
        for (offset, gain) in BOW_CONTACT_TAPS {
            let position = math::finite_clamp(
                contact_position + offset * half_width,
                0.001,
                0.999,
                contact_position,
            );
            averaged.left += gain
                * self.leftward.read(
                    (complementary_position_delay_samples(one_way_delay_samples, position)
                        - delay_advance)
                        .max(0.0),
                );
            averaged.right += gain
                * self.rightward.read(
                    (core::position_delay_samples(one_way_delay_samples, position) - delay_advance)
                        .max(0.0),
                );
        }
        averaged
    }

    pub(super) fn push(&mut self, leftward_sample: f32, rightward_sample: f32) {
        self.leftward.push(leftward_sample);
        self.rightward.push(rightward_sample);
    }

    /// Radiate the bow's outgoing waves: a point force `F` at the contact sends
    /// a velocity wave `F/(2·Z₀)` (= the solved velocity correction) outward on
    /// *each* rail. The correction is distributed over the finite-width contact
    /// window (gains sum to 1 per rail), so each rail carries the full
    /// correction once. The write lands one sample downstream of the contact in
    /// each rail's travel direction: emitted now, it must *arrive back* at the
    /// contact only via the endpoint reflections — a write at the bare contact
    /// delay would be re-read at full center-tap weight by the next sample's
    /// history read (the contact would mostly observe its own last force, a
    /// positive feedback that pins the contact in a false permanent stick).
    pub(super) fn apply_bow_contact_correction(
        &mut self,
        one_way_delay_samples: f32,
        position: f32,
        half_width: f32,
        velocity_correction: f32,
    ) {
        let position = math::finite_clamp(position, 0.001, 0.999, 0.5);
        let half_width = math::finite_or(half_width, 0.0).max(0.0);
        for (offset, gain) in BOW_CONTACT_TAPS {
            let position =
                math::finite_clamp(position + offset * half_width, 0.001, 0.999, position);
            let correction = math::snap_to_zero(velocity_correction * gain);
            self.leftward.add_at(
                complementary_position_delay_samples(one_way_delay_samples, position) + 1.0,
                correction,
            );
            self.rightward.add_at(
                core::position_delay_samples(one_way_delay_samples, position) + 1.0,
                correction,
            );
        }
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

/// Finite-width bow ribbon approximation: reads average and writes distribute
/// over a five-tap window (offsets in half-widths, gains summing to 1) instead
/// of a single point, smoothing the contact the way the hair band does. The
/// window is wider than one sample of wave travel, so a small fraction of a
/// correction is re-read at the outer taps the next sample — physically the
/// waves crossing the ribbon, accepted as part of the approximation.
const BOW_CONTACT_TAPS: [(f32, f32); 5] = [
    (-1.0, 0.0625),
    (-0.5, 0.25),
    (0.0, 0.375),
    (0.5, 0.25),
    (1.0, 0.0625),
];

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

    /// The string's restoring echo against a sustained contact correction: with
    /// per-end velocity reflection `-r`, a constant correction stream `c` must
    /// see a steady incoming-wave response of `-2r/(1+r)·c ≈ -c` (the velocity-
    /// wave image of a static deflection's restoring force). If the contact's
    /// history read leaked its own outgoing writes (window/interpolation overlap),
    /// this factor collapses toward zero and a bow can pin in a false permanent
    /// stick/slide instead of building to the Helmholtz break.
    #[test]
    fn contact_history_read_sees_full_restoring_echo() {
        let mut waves = TravelingWavePair::new(48_000.0, 20.0, 2.0);
        let d = 91.66_f32;
        let r = 0.995_f32;
        let c = 0.1_f32;
        let p = 0.12_f32;
        let hw = 0.012_f32;
        let advance = 6.0_f32;
        let mut fifo_left = [0.0_f32; 6];
        let mut fifo_right = [0.0_f32; 6];
        let mut fifo_index = 0_usize;
        let mut last_ratio = 0.0_f32;
        for _ in 0..120_000 {
            let b = waves.boundary_samples(d);
            let advanced = waves.contact_average_samples(d, p, hw, advance);
            last_ratio = (fifo_left[fifo_index] + fifo_right[fifo_index]) / c;
            fifo_left[fifo_index] = advanced.left;
            fifo_right[fifo_index] = advanced.right;
            fifo_index = (fifo_index + 1) % 6;
            waves.push(-r * b.right, -r * b.left);
            waves.apply_bow_contact_correction(d, p, hw, c);
        }
        let expected = -2.0 * r / (1.0 + r);
        assert!(
            (last_ratio - expected).abs() < 0.05,
            "restoring echo factor {last_ratio}, expected ≈ {expected}"
        );
    }

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
