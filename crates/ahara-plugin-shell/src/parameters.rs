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
            normalized_bits: AtomicU32::new(normalized.clamp(0.0, 1.0).to_bits()),
        }
    }

    pub const fn id(&self) -> ParameterId {
        self.id
    }

    pub fn load_normalized(&self) -> f32 {
        f32::from_bits(self.normalized_bits.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    pub fn store_normalized(&self, normalized: f32) {
        self.normalized_bits
            .store(normalized.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
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
}
