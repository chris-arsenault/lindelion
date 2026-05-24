use std::sync::atomic::{AtomicU32, Ordering};

use lindelion_dsp_utils::smoothing::{SmoothedParam, SmoothedParamSpec};

use super::{ParameterId, ParameterInfo};

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
