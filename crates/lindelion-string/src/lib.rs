//! Shared string physical model.
//!
//! Product crates own MIDI, articulations, patch storage, and UI. This crate owns the
//! validated plucked/bowed string waveguide, body coupling, and driver kernels.

mod body;
mod bow;
mod core;
mod dispersion;
mod driver;
mod model;
mod traveling;

pub use body::StringBodyMode;
pub use driver::{BowContactDrive, BowParams, PickParams, StringDriver, StringDriverMode};
pub use model::{StringModel, StringModelParams, StringModelProbe, StringModelSwitches};

pub(crate) const DSP_FALLBACK_SAMPLE_RATE: f32 = 48_000.0;
pub(crate) const LOWEST_STRING_FREQUENCY_HZ: f32 = 20.0;

pub(crate) const LOOP_FILTER_CUTOFF_DEFAULT_HZ: f32 = 8_000.0;
pub(crate) const LOOP_FILTER_RESONANCE_DEFAULT: f32 = 0.0;
pub(crate) const LOOP_GAIN_DEFAULT: f32 = 0.97;
pub(crate) const DISPERSION_DEFAULT: f32 = 1.0;
pub(crate) const STRIKE_POSITION_DEFAULT: f32 = 0.5;
pub(crate) const PICKUP_POSITION_DEFAULT: f32 = 0.82;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatRange {
    min: f32,
    max: f32,
    default: f32,
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

pub(crate) const FILTER_RESONANCE: FloatRange = FloatRange::new(0.0, 0.999, 0.0);
pub(crate) const LOOP_GAIN: FloatRange = FloatRange::new(0.0, 0.999, LOOP_GAIN_DEFAULT);
pub(crate) const LOOP_FILTER_Q: ResonanceQ = ResonanceQ::new(0.55, 4.0);
pub(crate) const STRIKE_POSITION: FloatRange = FloatRange::new(0.001, 0.999, 0.5);
pub(crate) const PICKUP_POSITION: FloatRange = FloatRange::new(0.001, 0.999, 0.82);
pub(crate) const DISPERSION: FloatRange = FloatRange::new(0.0, 1.0, 1.0);
