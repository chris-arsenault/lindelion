use ahara_dsp_utils::smoothing::{SmoothedParam, SmoothedParamSpec};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParameterId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterRange {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ParameterRange {
    pub const fn linear(min: f32, max: f32, default: f32) -> Self {
        Self { min, max, default }
    }

    pub fn normalize(self, value: f32) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }

        let value = if value.is_finite() {
            value
        } else {
            self.default
        };
        ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }

    pub fn denormalize(self, normalized: f32) -> f32 {
        let normalized = if normalized.is_finite() {
            normalized
        } else {
            self.normalize(self.default)
        };
        self.min + normalized.clamp(0.0, 1.0) * (self.max - self.min)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterFlags {
    pub automatable: bool,
    pub read_only: bool,
}

impl ParameterFlags {
    pub const AUTOMATABLE: Self = Self {
        automatable: true,
        read_only: false,
    };

    pub const READ_ONLY: Self = Self {
        automatable: false,
        read_only: true,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterInfo {
    pub id: ParameterId,
    pub name: &'static str,
    pub units: &'static str,
    pub range: ParameterRange,
    pub step_count: Option<u32>,
    pub flags: ParameterFlags,
}

impl ParameterInfo {
    pub const fn continuous(
        id: u32,
        name: &'static str,
        units: &'static str,
        range: ParameterRange,
    ) -> Self {
        Self {
            id: ParameterId(id),
            name,
            units,
            range,
            step_count: None,
            flags: ParameterFlags::AUTOMATABLE,
        }
    }

    pub const fn stepped(
        id: u32,
        name: &'static str,
        units: &'static str,
        range: ParameterRange,
        step_count: u32,
    ) -> Self {
        Self {
            id: ParameterId(id),
            name,
            units,
            range,
            step_count: Some(step_count),
            flags: ParameterFlags::AUTOMATABLE,
        }
    }
}

#[derive(Debug)]
pub struct AtomicParameter {
    id: ParameterId,
    normalized_bits: AtomicU32,
}

impl AtomicParameter {
    pub fn new(id: ParameterId, normalized: f32) -> Self {
        Self {
            id,
            normalized_bits: AtomicU32::new(sanitize_normalized(normalized).to_bits()),
        }
    }

    pub const fn id(&self) -> ParameterId {
        self.id
    }

    pub fn load_normalized(&self) -> f32 {
        sanitize_normalized(f32::from_bits(self.normalized_bits.load(Ordering::Relaxed)))
    }

    pub fn store_normalized(&self, normalized: f32) {
        self.normalized_bits
            .store(sanitize_normalized(normalized).to_bits(), Ordering::Relaxed);
    }
}

pub type PlainToSmoothedValue = fn(f32) -> f32;

#[derive(Debug, Clone, Copy)]
pub struct SmoothedAtomicParamSpec {
    pub info: ParameterInfo,
    pub smoothed: SmoothedParamSpec,
    plain_to_smoothed: PlainToSmoothedValue,
}

impl SmoothedAtomicParamSpec {
    pub const fn from_parameter(info: ParameterInfo, smoothing_ms: f32, epsilon: f32) -> Self {
        Self::mapped(
            info,
            SmoothedParamSpec::new(
                info.range.min,
                info.range.max,
                info.range.default,
                smoothing_ms,
                epsilon,
            ),
            identity_plain_value,
        )
    }

    pub const fn mapped(
        info: ParameterInfo,
        smoothed: SmoothedParamSpec,
        plain_to_smoothed: PlainToSmoothedValue,
    ) -> Self {
        Self {
            info,
            smoothed,
            plain_to_smoothed,
        }
    }

    pub fn smoothed_value(self, plain: f32) -> f32 {
        let normalized = self.info.range.normalize(plain);
        let plain = self.info.range.denormalize(normalized);
        self.smoothed.sanitize((self.plain_to_smoothed)(plain))
    }

    pub fn normalized_for_plain(self, plain: f32) -> f32 {
        self.info.range.normalize(plain)
    }
}

#[derive(Debug)]
pub struct SmoothedAtomicParam {
    spec: SmoothedAtomicParamSpec,
    atomic: AtomicParameter,
    smoothed: SmoothedParam,
    last_normalized_bits: u32,
}

impl SmoothedAtomicParam {
    pub fn new(spec: SmoothedAtomicParamSpec, sample_rate: f32) -> Self {
        Self::with_initial_plain(spec, sample_rate, spec.info.range.default)
    }

    pub fn with_initial_plain(spec: SmoothedAtomicParamSpec, sample_rate: f32, plain: f32) -> Self {
        let normalized = spec.normalized_for_plain(plain);
        let smoothed_value = spec.smoothed_value(plain);
        Self {
            spec,
            atomic: AtomicParameter::new(spec.info.id, normalized),
            smoothed: SmoothedParam::with_initial(spec.smoothed, sample_rate, smoothed_value),
            last_normalized_bits: normalized.to_bits(),
        }
    }

    pub const fn spec(&self) -> SmoothedAtomicParamSpec {
        self.spec
    }

    pub const fn atomic(&self) -> &AtomicParameter {
        &self.atomic
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.smoothed.set_sample_rate(sample_rate);
    }

    pub fn reset_plain(&mut self, plain: f32) {
        let normalized = self.spec.normalized_for_plain(plain);
        self.atomic.store_normalized(normalized);
        self.last_normalized_bits = normalized.to_bits();
        self.smoothed.reset(self.spec.smoothed_value(plain));
    }

    pub fn set_plain_target(&mut self, plain: f32) {
        self.atomic
            .store_normalized(self.spec.normalized_for_plain(plain));
        self.sync_from_atomic();
    }

    pub fn sync_from_atomic(&mut self) -> bool {
        let normalized = self.atomic.load_normalized();
        let normalized_bits = normalized.to_bits();
        if normalized_bits == self.last_normalized_bits {
            return false;
        }

        self.last_normalized_bits = normalized_bits;
        let plain = self.spec.info.range.denormalize(normalized);
        self.smoothed.set_target(self.spec.smoothed_value(plain));
        true
    }

    pub fn next_sample(&mut self) -> f32 {
        self.smoothed.next_sample()
    }

    pub const fn current(&self) -> f32 {
        self.smoothed.current()
    }

    pub const fn target(&self) -> f32 {
        self.smoothed.target()
    }

    pub const fn is_smoothing(&self) -> bool {
        self.smoothed.is_smoothing()
    }
}

fn identity_plain_value(value: f32) -> f32 {
    value
}

fn sanitize_normalized(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_denormalizes() {
        let range = ParameterRange::linear(-12.0, 12.0, 0.0);
        assert_eq!(range.normalize(0.0), 0.5);
        assert_eq!(range.denormalize(0.5), 0.0);
    }

    #[test]
    fn non_finite_values_fall_back_to_default() {
        let range = ParameterRange::linear(20.0, 20_000.0, 20_000.0);

        assert_eq!(range.normalize(f32::NAN), 1.0);
        assert_eq!(range.denormalize(f32::NAN), 20_000.0);
        assert_eq!(range.denormalize(f32::INFINITY), 20_000.0);
    }

    #[test]
    fn atomic_parameter_sanitizes_normalized_values() {
        let parameter = AtomicParameter::new(ParameterId(1), f32::NAN);

        assert_eq!(parameter.load_normalized(), 0.0);

        parameter.store_normalized(2.0);
        assert_eq!(parameter.load_normalized(), 1.0);
    }

    #[test]
    fn smoothed_atomic_param_round_trips_atomic_write_to_sample_accurate_ramp() {
        let info =
            ParameterInfo::continuous(1, "Target", "", ParameterRange::linear(0.0, 1.0, 0.0));
        let spec = SmoothedAtomicParamSpec::from_parameter(info, 4.0, 0.0);
        let mut parameter = SmoothedAtomicParam::new(spec, 1_000.0);

        parameter.atomic().store_normalized(1.0);

        assert!(parameter.sync_from_atomic());
        assert_eq!(parameter.target(), 1.0);
        assert_eq!(parameter.next_sample(), 0.25);
        assert_eq!(parameter.next_sample(), 0.5);
        assert_eq!(parameter.next_sample(), 0.75);
        assert_eq!(parameter.next_sample(), 1.0);
        assert_eq!(parameter.next_sample(), 1.0);
        assert!(!parameter.is_smoothing());
    }

    #[test]
    fn smoothed_atomic_param_maps_plain_values_before_smoothing() {
        fn square(value: f32) -> f32 {
            value * value
        }

        let info =
            ParameterInfo::continuous(2, "Mapped", "", ParameterRange::linear(0.0, 2.0, 1.0));
        let spec = SmoothedAtomicParamSpec::mapped(
            info,
            SmoothedParamSpec::new(0.0, 4.0, 1.0, 0.0, 0.0),
            square,
        );
        let mut parameter = SmoothedAtomicParam::new(spec, 1_000.0);

        parameter.set_plain_target(2.0);

        assert_eq!(parameter.target(), 4.0);
        assert_eq!(parameter.next_sample(), 4.0);
    }
}
