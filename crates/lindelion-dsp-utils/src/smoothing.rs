use crate::params::FloatParamSpec;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothedParamSpec {
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub smoothing_ms: f32,
    pub epsilon: f32,
}

impl SmoothedParamSpec {
    pub const fn new(min: f32, max: f32, default: f32, smoothing_ms: f32, epsilon: f32) -> Self {
        Self {
            min,
            max,
            default,
            smoothing_ms,
            epsilon,
        }
    }

    pub const fn value_spec(self) -> FloatParamSpec {
        FloatParamSpec::new(self.min, self.max, self.default, self.epsilon)
    }

    pub fn sanitize(self, value: f32) -> f32 {
        self.value_spec().sanitize(value)
    }

    pub fn duration_samples(self, sample_rate: f32) -> usize {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let smoothing_ms = if self.smoothing_ms.is_finite() {
            self.smoothing_ms.max(0.0)
        } else {
            0.0
        };
        (sample_rate * smoothing_ms * 0.001).round() as usize
    }

    pub fn epsilon(self) -> f32 {
        self.value_spec().epsilon()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothedParam {
    spec: SmoothedParamSpec,
    smoother: LinearSmoother,
    duration_samples: usize,
}

impl SmoothedParam {
    pub fn new(spec: SmoothedParamSpec, sample_rate: f32) -> Self {
        Self::with_initial(spec, sample_rate, spec.default)
    }

    pub fn with_initial(spec: SmoothedParamSpec, sample_rate: f32, value: f32) -> Self {
        let value = spec.sanitize(value);
        Self {
            spec,
            smoother: LinearSmoother::new(value),
            duration_samples: spec.duration_samples(sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.duration_samples = self.spec.duration_samples(sample_rate);
    }

    pub fn reset(&mut self, value: f32) {
        self.smoother = LinearSmoother::new(self.spec.sanitize(value));
    }

    pub fn set_target(&mut self, target: f32) {
        let target = self.spec.sanitize(target);
        if (self.smoother.target() - target).abs() > self.spec.epsilon() {
            self.smoother.set_target(target, self.duration_samples);
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        self.smoother.next_sample()
    }

    pub const fn current(&self) -> f32 {
        self.smoother.current()
    }

    pub const fn target(&self) -> f32 {
        self.smoother.target()
    }

    pub const fn is_smoothing(&self) -> bool {
        self.smoother.is_smoothing()
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

    #[test]
    fn smoothed_param_sanitizes_target_and_smooths() {
        let spec = SmoothedParamSpec::new(-1.0, 1.0, 0.0, 10.0, 0.000_1);
        let mut parameter = SmoothedParam::with_initial(spec, 1_000.0, 0.0);

        parameter.set_target(2.0);

        assert_eq!(parameter.target(), 1.0);
        assert!(parameter.is_smoothing());
        assert_eq!(parameter.next_sample(), 0.1);
    }

    #[test]
    fn smoothed_param_ignores_targets_inside_epsilon() {
        let spec = SmoothedParamSpec::new(0.0, 1.0, 0.5, 10.0, 0.01);
        let mut parameter = SmoothedParam::with_initial(spec, 1_000.0, 0.5);

        parameter.set_target(0.505);

        assert_eq!(parameter.target(), 0.5);
        assert!(!parameter.is_smoothing());
    }

    #[test]
    fn smoothed_param_uses_default_for_non_finite_values() {
        let spec = SmoothedParamSpec::new(0.0, 1.0, 0.25, 0.0, 0.0);
        let mut parameter = SmoothedParam::with_initial(spec, 48_000.0, f32::NAN);

        assert_eq!(parameter.current(), 0.25);
        parameter.set_target(f32::INFINITY);
        assert_eq!(parameter.target(), 0.25);
    }
}
