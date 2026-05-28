use crate::{interpolation, math};

#[derive(Debug, Clone, PartialEq)]
pub struct DelayLine {
    buffer: Vec<f32>,
    write_index: usize,
}

impl DelayLine {
    pub fn new(max_delay_samples: usize) -> Self {
        let len = max_delay_samples.max(1) + 4;
        Self {
            buffer: vec![0.0; len],
            write_index: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_index = 0;
    }

    pub fn push(&mut self, sample: f32) {
        self.buffer[self.write_index] = math::snap_to_zero(sample);
        self.write_index = (self.write_index + 1) % self.buffer.len();
    }

    pub fn add_at(&mut self, delay_samples: f32, sample: f32) {
        let sample = math::snap_to_zero(sample);
        if sample == 0.0 {
            return;
        }

        let max_delay = (self.buffer.len() - 2) as f32;
        let delay_samples = finite_delay_samples(delay_samples, max_delay);
        let write_position = self.write_index as f32 - 1.0 - delay_samples;
        let index_floor = write_position.floor();
        let fraction = write_position - index_floor;
        let index_a = index_floor as isize;
        let index_b = index_a + 1;
        let len = self.buffer.len() as isize;
        let wrapped_a = index_a.rem_euclid(len) as usize;
        let wrapped_b = index_b.rem_euclid(len) as usize;

        self.buffer[wrapped_a] =
            math::snap_to_zero(self.buffer[wrapped_a] + sample * (1.0 - fraction));
        self.buffer[wrapped_b] = math::snap_to_zero(self.buffer[wrapped_b] + sample * fraction);
    }

    pub fn read(&self, delay_samples: f32) -> f32 {
        let max_delay = (self.buffer.len() - 2) as f32;
        let delay_samples = finite_delay_samples(delay_samples, max_delay);
        let read_index = self.write_index as f32 - 1.0 - delay_samples;
        math::snap_to_zero(interpolation::linear_wrapped(&self.buffer, read_index))
    }
}

fn finite_delay_samples(delay_samples: f32, max_delay: f32) -> f32 {
    if delay_samples.is_finite() {
        delay_samples.clamp(0.0, max_delay)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FirstOrderAllpass {
    coefficient: f32,
    z1: f32,
}

impl Default for FirstOrderAllpass {
    fn default() -> Self {
        Self {
            coefficient: 0.0,
            z1: 0.0,
        }
    }
}

impl FirstOrderAllpass {
    pub fn set_coefficient(&mut self, coefficient: f32) {
        self.coefficient = if coefficient.is_finite() {
            coefficient.clamp(-0.5, 1.0)
        } else {
            0.0
        };
    }

    pub fn set_fractional_delay(&mut self, fractional_delay: f32) {
        let fractional_delay = if fractional_delay.is_finite() {
            fractional_delay.clamp(0.0, 0.999_999)
        } else {
            0.0
        };
        self.coefficient = (1.0 - fractional_delay) / (1.0 + fractional_delay);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let input = math::snap_to_zero(input);
        let coefficient = math::snap_to_zero(self.coefficient).clamp(-0.5, 1.0);
        self.coefficient = coefficient;
        self.z1 = math::snap_to_zero(self.z1);
        let output = coefficient * (input - self.z1);
        let output = output + self.z1;
        self.z1 = math::snap_to_zero(input - coefficient * output);
        math::snap_to_zero(output)
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::assert_all_finite;

    #[test]
    fn integer_delay_returns_previous_sample() {
        let mut delay = DelayLine::new(8);
        delay.push(1.0);
        delay.push(2.0);
        delay.push(3.0);

        assert_eq!(delay.read(0.0), 3.0);
        assert_eq!(delay.read(1.0), 2.0);
        assert_eq!(delay.read(2.0), 1.0);
    }

    #[test]
    fn fractional_delay_interpolates() {
        let mut delay = DelayLine::new(8);
        delay.push(0.0);
        delay.push(1.0);

        assert!((delay.read(0.5) - 0.5).abs() < 0.000_01);
    }

    #[test]
    fn add_at_injects_fractional_tap_energy() {
        let mut delay = DelayLine::new(8);
        delay.push(0.0);
        delay.push(0.0);
        delay.add_at(0.5, 1.0);

        assert!((delay.read(0.0) - 0.5).abs() < 0.000_01);
        assert!((delay.read(1.0) - 0.5).abs() < 0.000_01);
    }

    #[test]
    fn allpass_is_stable_for_impulse() {
        let mut allpass = FirstOrderAllpass::default();
        allpass.set_fractional_delay(0.37);
        let mut output = Vec::new();

        output.push(allpass.process(1.0));
        for _ in 0..256 {
            output.push(allpass.process(0.0));
        }

        assert_all_finite(&output);
        assert!(output.iter().all(|sample| sample.abs() <= 1.0));
    }

    #[test]
    fn allpass_accepts_negative_dispersion_coefficients() {
        let mut allpass = FirstOrderAllpass::default();
        allpass.set_coefficient(-0.62);
        let mut output = Vec::new();

        output.push(allpass.process(1.0));
        for _ in 0..256 {
            output.push(allpass.process(0.0));
        }

        assert_all_finite(&output);
        assert!(output.iter().all(|sample| sample.abs() <= 1.25));
    }

    #[test]
    fn delay_line_sanitizes_non_finite_samples() {
        let mut delay = DelayLine::new(8);

        delay.push(f32::NAN);
        delay.add_at(f32::NAN, f32::INFINITY);

        assert_eq!(delay.read(f32::NAN), 0.0);
        assert_eq!(delay.read(0.0), 0.0);
    }

    #[test]
    fn allpass_recovers_from_non_finite_state() {
        let mut allpass = FirstOrderAllpass {
            coefficient: f32::NAN,
            z1: f32::NAN,
        };

        assert_eq!(allpass.process(f32::NAN), 0.0);
        assert!(allpass.process(0.5).is_finite());
    }
}
