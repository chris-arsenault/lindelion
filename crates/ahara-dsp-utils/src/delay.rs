use crate::interpolation;

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
        self.buffer[self.write_index] = sample;
        self.write_index = (self.write_index + 1) % self.buffer.len();
    }

    pub fn read(&self, delay_samples: f32) -> f32 {
        let max_delay = (self.buffer.len() - 2) as f32;
        let delay_samples = delay_samples.clamp(0.0, max_delay);
        let read_index = self.write_index as f32 - 1.0 - delay_samples;
        interpolation::linear_wrapped(&self.buffer, read_index)
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
    pub fn set_fractional_delay(&mut self, fractional_delay: f32) {
        let fractional_delay = fractional_delay.clamp(0.0, 0.999_999);
        self.coefficient = (1.0 - fractional_delay) / (1.0 + fractional_delay);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.coefficient * (input - self.z1);
        let output = output + self.z1;
        self.z1 = input - self.coefficient * output;
        output
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
}
