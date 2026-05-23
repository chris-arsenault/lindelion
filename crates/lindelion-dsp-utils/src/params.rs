#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatParamSpec {
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub epsilon: f32,
}

impl FloatParamSpec {
    pub const fn new(min: f32, max: f32, default: f32, epsilon: f32) -> Self {
        Self {
            min,
            max,
            default,
            epsilon,
        }
    }

    pub fn sanitize(self, value: f32) -> f32 {
        let min = self.min.min(self.max);
        let max = self.min.max(self.max);
        let default = if self.default.is_finite() {
            self.default.clamp(min, max)
        } else {
            min
        };

        if value.is_finite() {
            value.clamp(min, max)
        } else {
            default
        }
    }

    pub fn epsilon(self) -> f32 {
        if self.epsilon.is_finite() {
            self.epsilon.max(0.0)
        } else {
            0.0
        }
    }

    pub fn has_changed(self, current: f32, next: f32) -> bool {
        (current - next).abs() > self.epsilon()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImmediateParam {
    spec: FloatParamSpec,
    value: f32,
}

impl ImmediateParam {
    pub fn new(spec: FloatParamSpec) -> Self {
        Self::with_initial(spec, spec.default)
    }

    pub fn with_initial(spec: FloatParamSpec, value: f32) -> Self {
        Self {
            spec,
            value: spec.sanitize(value),
        }
    }

    pub fn set(&mut self, value: f32) -> bool {
        let value = self.spec.sanitize(value);
        if self.spec.has_changed(self.value, value) {
            self.value = value;
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self, value: f32) {
        self.value = self.spec.sanitize(value);
    }

    pub const fn current(&self) -> f32 {
        self.value
    }

    pub const fn spec(&self) -> FloatParamSpec {
        self.spec
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralChangePolicy {
    NoteBoundary,
    ResetState,
    LiveCrossfade,
    LiveMuteRamp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuralParam<T> {
    current: T,
    pending: Option<T>,
    policy: StructuralChangePolicy,
    ramp_samples: usize,
    transition: StructuralTransition,
}

pub type LatchedParam<T> = StructuralParam<T>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuralParamSample<T> {
    pub gain: f32,
    pub change: Option<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructuralTransition {
    Idle,
    ApplyImmediate,
    FadingOut { remaining: usize },
    FadingIn { elapsed: usize },
}

impl<T: Copy + PartialEq> StructuralParam<T> {
    pub const fn new(current: T, policy: StructuralChangePolicy) -> Self {
        Self {
            current,
            pending: None,
            policy,
            ramp_samples: 0,
            transition: StructuralTransition::Idle,
        }
    }

    pub const fn with_ramp_samples(
        current: T,
        policy: StructuralChangePolicy,
        ramp_samples: usize,
    ) -> Self {
        Self {
            current,
            pending: None,
            policy,
            ramp_samples,
            transition: StructuralTransition::Idle,
        }
    }

    pub fn set_pending(&mut self, value: T) -> bool {
        self.transition = StructuralTransition::Idle;
        if value == self.current {
            self.pending = None;
            false
        } else {
            self.pending = Some(value);
            true
        }
    }

    pub fn set_target(&mut self, value: T) -> bool {
        if value == self.current {
            self.pending = None;
            self.transition = StructuralTransition::Idle;
            return false;
        }
        if self.pending == Some(value) {
            return false;
        }

        self.pending = Some(value);
        self.transition = match self.policy {
            StructuralChangePolicy::NoteBoundary => StructuralTransition::Idle,
            StructuralChangePolicy::ResetState => StructuralTransition::ApplyImmediate,
            StructuralChangePolicy::LiveCrossfade | StructuralChangePolicy::LiveMuteRamp => {
                if self.ramp_samples == 0 {
                    StructuralTransition::ApplyImmediate
                } else {
                    StructuralTransition::FadingOut {
                        remaining: self.ramp_samples,
                    }
                }
            }
        };
        true
    }

    pub fn next_sample(&mut self) -> StructuralParamSample<T> {
        match self.transition {
            StructuralTransition::Idle => StructuralParamSample {
                gain: 1.0,
                change: None,
            },
            StructuralTransition::ApplyImmediate => {
                self.transition = StructuralTransition::Idle;
                StructuralParamSample {
                    gain: 1.0,
                    change: self.apply_pending(),
                }
            }
            StructuralTransition::FadingOut { remaining } => {
                if remaining <= 1 {
                    self.transition = StructuralTransition::FadingIn { elapsed: 0 };
                    StructuralParamSample {
                        gain: 0.0,
                        change: self.apply_pending(),
                    }
                } else {
                    let remaining = remaining - 1;
                    self.transition = StructuralTransition::FadingOut { remaining };
                    StructuralParamSample {
                        gain: remaining as f32 / self.ramp_samples as f32,
                        change: None,
                    }
                }
            }
            StructuralTransition::FadingIn { elapsed } => {
                let elapsed = elapsed + 1;
                let gain = if elapsed >= self.ramp_samples {
                    self.transition = StructuralTransition::Idle;
                    1.0
                } else {
                    self.transition = StructuralTransition::FadingIn { elapsed };
                    elapsed as f32 / self.ramp_samples as f32
                };
                StructuralParamSample { gain, change: None }
            }
        }
    }

    pub fn latch(&mut self) -> Option<T> {
        self.transition = StructuralTransition::Idle;
        self.apply_pending()
    }

    pub fn apply_immediate(&mut self, value: T) -> bool {
        self.pending = None;
        self.transition = StructuralTransition::Idle;
        if value == self.current {
            false
        } else {
            self.current = value;
            true
        }
    }

    pub fn reset(&mut self, value: T) {
        self.current = value;
        self.pending = None;
        self.transition = StructuralTransition::Idle;
    }

    pub const fn current(&self) -> T {
        self.current
    }

    pub const fn pending(&self) -> Option<T> {
        self.pending
    }

    pub const fn policy(&self) -> StructuralChangePolicy {
        self.policy
    }

    pub const fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn apply_pending(&mut self) -> Option<T> {
        let pending = self.pending.take()?;
        if pending == self.current {
            None
        } else {
            self.current = pending;
            Some(pending)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_param_sanitizes_and_sets_without_smoothing() {
        let spec = FloatParamSpec::new(0.0, 1.0, 0.25, 0.000_1);
        let mut parameter = ImmediateParam::with_initial(spec, f32::NAN);

        assert_eq!(parameter.current(), 0.25);
        assert!(parameter.set(2.0));
        assert_eq!(parameter.current(), 1.0);
        assert!(!parameter.set(1.0 + 0.000_01));
        assert_eq!(parameter.current(), 1.0);
    }

    #[test]
    fn latched_param_keeps_pending_value_until_latched() {
        let mut parameter = LatchedParam::new(0_u8, StructuralChangePolicy::NoteBoundary);

        assert!(parameter.set_pending(1));
        assert_eq!(parameter.current(), 0);
        assert_eq!(parameter.pending(), Some(1));
        assert_eq!(parameter.latch(), Some(1));
        assert_eq!(parameter.current(), 1);
        assert!(!parameter.has_pending());
    }

    #[test]
    fn structural_param_can_apply_immediate_reset_policy() {
        let mut parameter = StructuralParam::new("lowpass", StructuralChangePolicy::ResetState);

        assert_eq!(parameter.policy(), StructuralChangePolicy::ResetState);
        assert!(parameter.apply_immediate("highpass"));
        assert_eq!(parameter.current(), "highpass");
        assert!(!parameter.has_pending());
    }

    #[test]
    fn structural_param_live_mute_ramp_applies_change_at_silence() {
        let mut parameter =
            StructuralParam::with_ramp_samples(0_u8, StructuralChangePolicy::LiveMuteRamp, 4);

        assert!(parameter.set_target(1));
        assert_ramp_gains(&mut parameter, &[0.75, 0.5, 0.25]);
        assert_silence_applies_change(&mut parameter, 1);
        assert_ramp_gains(&mut parameter, &[0.25, 0.5, 0.75, 1.0]);
    }

    fn assert_silence_applies_change(parameter: &mut StructuralParam<u8>, expected: u8) {
        let silence = parameter.next_sample();
        assert_eq!(silence.gain, 0.0);
        assert_eq!(silence.change, Some(expected));
        assert_eq!(parameter.current(), expected);
    }

    fn assert_ramp_gains(parameter: &mut StructuralParam<u8>, expected: &[f32]) {
        for expected_gain in expected {
            let sample = parameter.next_sample();
            assert_eq!(sample.gain, *expected_gain);
            assert_eq!(sample.change, None);
        }
    }
}
