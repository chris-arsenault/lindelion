//! Shared reed-driven tube physical model.
//!
//! Product crates own MIDI, articulations, patch storage, and UI. This module owns the
//! validated bore/reed algorithm and exposes a sparse, host-neutral DSP boundary.

mod body;
mod core;
mod reed;
mod traveling;
mod tube;

pub use reed::{ReedDriver, ReedParams, ReedProcessTaps};
pub use tube::{ReedTube, ReedTubeParams, ReedTubeSwitches, ReedTubeTaps};

pub(crate) const DSP_FALLBACK_SAMPLE_RATE: f32 = 48_000.0;
pub(crate) const DEFAULT_BIQUAD_Q: f32 = 0.707;
pub(crate) const LOWEST_TUBE_FREQUENCY_HZ: f32 = 20.0;

pub(crate) const LOOP_FILTER_CUTOFF_DEFAULT_HZ: f32 = 4_200.0;
pub(crate) const LOOP_GAIN_DEFAULT: f32 = 0.97;
pub(crate) const LOOP_FILTER_RESONANCE_DEFAULT: f32 = 0.0;
pub(crate) const PICKUP_POSITION_DEFAULT: f32 = 0.82;
pub(crate) const BOUNDARY_REFLECTION_DEFAULT: f32 = -0.75;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatRange {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl FloatRange {
    pub const fn new(min: f32, max: f32, default: f32) -> Self {
        Self { min, max, default }
    }

    pub fn clamp(self, value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.default
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResonanceQ {
    base: f32,
    scale: f32,
}

impl ResonanceQ {
    pub const fn new(base: f32, scale: f32) -> Self {
        Self { base, scale }
    }

    pub fn q_for_resonance(self, resonance: f32) -> f32 {
        self.base + FILTER_RESONANCE.clamp(resonance) * self.scale
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TubeBoundaryModel {
    reflection: FloatRange,
    output_base_gain: f32,
    output_reflection_gain: f32,
}

impl TubeBoundaryModel {
    pub const fn new(
        reflection: FloatRange,
        output_base_gain: f32,
        output_reflection_gain: f32,
    ) -> Self {
        Self {
            reflection,
            output_base_gain,
            output_reflection_gain,
        }
    }

    pub fn reflection(self, value: f32) -> f32 {
        self.reflection.clamp(value)
    }

    pub fn output_gain(self, value: f32) -> f32 {
        let reflection = self.reflection(value).abs();
        self.output_base_gain + reflection * self.output_reflection_gain
    }
}

pub(crate) const FILTER_RESONANCE: FloatRange = FloatRange::new(0.0, 0.999, 0.0);
pub(crate) const LOOP_GAIN: FloatRange = FloatRange::new(0.0, 0.999, LOOP_GAIN_DEFAULT);
pub(crate) const LOOP_FILTER_Q: ResonanceQ = ResonanceQ::new(0.55, 4.0);
pub(crate) const PICKUP_POSITION: FloatRange = FloatRange::new(0.001, 0.999, 0.82);
pub(crate) const TUBE_BOUNDARY: TubeBoundaryModel =
    TubeBoundaryModel::new(FloatRange::new(-1.0, 1.0, -0.75), 0.8, 0.2);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_model_numerics_are_pinned() {
        assert_eq!(TUBE_BOUNDARY.reflection(f32::NAN), -0.75);
        assert_eq!(TUBE_BOUNDARY.reflection(2.0), 1.0);
        assert!((TUBE_BOUNDARY.output_gain(0.75) - 0.95).abs() < 0.000_001);
    }
}
