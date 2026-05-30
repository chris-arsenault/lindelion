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
        math::snap_to_zero(interpolation::cubic_wrapped(&self.buffer, read_index))
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
        let output = coefficient * input + self.z1;
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
        // Cubic (Lagrange) interpolation reproduces a linear ramp exactly, so a
        // half-sample read lands on the exact interpolated value. Enough ramp
        // samples are pushed for the 4-point stencil to sit fully on the ramp.
        let mut delay = DelayLine::new(8);
        for sample in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            delay.push(sample);
        }

        assert!((delay.read(2.5) - 3.5).abs() < 0.000_01);
        assert!((delay.read(3.5) - 2.5).abs() < 0.000_01);
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
    fn allpass_impulse_response_matches_reference() {
        // First-order allpass H(z) = (c + z^-1) / (1 + c z^-1) has the closed-form
        // impulse response h[0] = c, h[n] = (1 - c^2)(-c)^(n-1) for n >= 1.
        // For c = 0.5: [0.5, 0.75, -0.375, 0.1875, -0.09375].
        let mut allpass = FirstOrderAllpass::default();
        allpass.set_coefficient(0.5);

        let mut response = Vec::new();
        response.push(allpass.process(1.0));
        for _ in 0..4 {
            response.push(allpass.process(0.0));
        }

        let expected = [0.5_f32, 0.75, -0.375, 0.1875, -0.09375];
        for (index, (got, want)) in response.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - want).abs() < 1e-6,
                "h[{index}] = {got}, expected {want}"
            );
        }
    }

    #[test]
    fn allpass_magnitude_response_is_flat() {
        // A true first-order allpass has unity magnitude at every frequency.
        // Evaluate |H(e^{jw})| = |sum_n h[n] e^{-jwn}| from the impulse response.
        for &coefficient in &[-0.5_f32, -0.25, 0.25, 0.5] {
            let mut allpass = FirstOrderAllpass::default();
            allpass.set_coefficient(coefficient);

            let mut response = Vec::new();
            response.push(allpass.process(1.0));
            for _ in 0..511 {
                response.push(allpass.process(0.0));
            }

            for step in 0..=16 {
                let omega = std::f64::consts::PI * f64::from(step) / 16.0;
                let mut real = 0.0_f64;
                let mut imag = 0.0_f64;
                for (n, &sample) in response.iter().enumerate() {
                    let angle = omega * n as f64;
                    real += f64::from(sample) * angle.cos();
                    imag -= f64::from(sample) * angle.sin();
                }
                let magnitude = real.hypot(imag);
                assert!(
                    (magnitude - 1.0).abs() < 1e-6,
                    "coefficient {coefficient}, omega {omega}: |H| = {magnitude}"
                );
            }
        }
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
