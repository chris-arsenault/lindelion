//! String model parameter surface, switches, and the diagnostic probe.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct String1dParams {
    pub(crate) frequency_hz: f32,
    pub(crate) loop_filter_cutoff: f32,
    pub(crate) loop_filter_resonance: f32,
    pub(crate) loop_gain: f32,
    pub(crate) loop_nonlinearity: f32,
    pub(crate) dispersion: f32,
    pub(crate) strike_position: f32,
    pub(crate) pickup_position: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StringMaterialParams {
    pub(super) loop_filter_cutoff: f32,
    pub(super) loop_filter_resonance: f32,
    pub(super) loop_gain: f32,
    pub(super) loop_nonlinearity: f32,
    pub(super) dispersion: f32,
    pub(super) strike_position: f32,
    pub(super) pickup_position: f32,
}

impl String1dParams {
    pub(super) fn material_params(self) -> StringMaterialParams {
        StringMaterialParams {
            loop_filter_cutoff: self.loop_filter_cutoff,
            loop_filter_resonance: self.loop_filter_resonance,
            loop_gain: self.loop_gain,
            loop_nonlinearity: self.loop_nonlinearity,
            dispersion: self.dispersion,
            strike_position: self.strike_position,
            pickup_position: self.pickup_position,
        }
    }

    pub(super) fn with_frequency(self, frequency_hz: f32) -> Self {
        Self {
            frequency_hz,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringModelSwitches {
    pub body_contact_enabled: bool,
    pub tension_modulation_enabled: bool,
    pub source_body_balance_enabled: bool,
}

impl Default for StringModelSwitches {
    fn default() -> Self {
        Self {
            body_contact_enabled: true,
            tension_modulation_enabled: true,
            source_body_balance_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StringModelParams {
    pub frequency_hz: f32,
    pub loop_filter_cutoff_hz: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub dispersion: f32,
    pub strike_position: f32,
    pub pickup_position: f32,
    pub excitation_spread: f32,
    pub source_body_balance: f32,
    pub body_mode: StringBodyMode,
    pub switches: StringModelSwitches,
}

impl Default for StringModelParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            loop_filter_cutoff_hz: LOOP_FILTER_CUTOFF_DEFAULT_HZ,
            loop_filter_resonance: LOOP_FILTER_RESONANCE_DEFAULT,
            loop_gain: LOOP_GAIN_DEFAULT,
            loop_nonlinearity: 0.0,
            dispersion: DISPERSION_DEFAULT,
            strike_position: STRIKE_POSITION_DEFAULT,
            pickup_position: PICKUP_POSITION_DEFAULT,
            excitation_spread: 0.0,
            source_body_balance: 0.0,
            body_mode: StringBodyMode::default(),
            switches: StringModelSwitches::default(),
        }
    }
}

impl StringModelParams {
    pub(super) fn string_params(self) -> String1dParams {
        String1dParams {
            frequency_hz: self.frequency_hz,
            loop_filter_cutoff: self.loop_filter_cutoff_hz,
            loop_filter_resonance: self.loop_filter_resonance,
            loop_gain: self.loop_gain,
            loop_nonlinearity: self.loop_nonlinearity,
            dispersion: self.dispersion,
            strike_position: self.strike_position,
            pickup_position: self.pickup_position,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StringModelProbe {
    pub pickup_tap: f32,
    pub body_radiated: f32,
    pub weighted_pickup: f32,
    pub weighted_body: f32,
    pub pickup_weight: f32,
    pub body_weight: f32,
    pub output: f32,
    pub bow_force: f32,
    pub bow_wave_correction: f32,
    pub current_frequency_hz: f32,
    pub one_way_delay_samples: f32,
}
