#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearSmoother {
    current: f32,
    target: f32,
    step: f32,
    samples_remaining: usize,
}

impl LinearSmoother {
    pub fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            samples_remaining: 0,
        }
    }

    pub fn set_target(&mut self, target: f32, duration_samples: usize) {
        self.target = target;
        self.samples_remaining = duration_samples;
        self.step = if duration_samples == 0 {
            self.current = target;
            0.0
        } else {
            (target - self.current) / duration_samples as f32
        };
    }

    pub fn next_sample(&mut self) -> f32 {
        if self.samples_remaining > 0 {
            self.current += self.step;
            self.samples_remaining -= 1;
        } else {
            self.current = self.target;
        }

        self.current
    }

    pub const fn current(&self) -> f32 {
        self.current
    }

    pub const fn target(&self) -> f32 {
        self.target
    }

    pub const fn is_smoothing(&self) -> bool {
        self.samples_remaining > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_target_after_requested_samples() {
        let mut smoother = LinearSmoother::new(0.0);
        smoother.set_target(1.0, 4);

        assert_eq!(smoother.next_sample(), 0.25);
        assert_eq!(smoother.next_sample(), 0.5);
        assert_eq!(smoother.next_sample(), 0.75);
        assert_eq!(smoother.next_sample(), 1.0);
        assert_eq!(smoother.next_sample(), 1.0);
        assert!(!smoother.is_smoothing());
    }

    #[test]
    fn zero_duration_jumps_immediately() {
        let mut smoother = LinearSmoother::new(0.0);
        smoother.set_target(0.75, 0);

        assert_eq!(smoother.current(), 0.75);
        assert_eq!(smoother.next_sample(), 0.75);
    }
}
