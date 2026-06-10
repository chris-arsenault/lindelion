//! Patch-to-model parameter mapping for the wind voice: bore/reed parameter
//! assembly (with humanize/phrasing modulation applied) and the register-key
//! fingering state.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) fn tube_params_with_mod(
    patch: &TubePatch,
    current_note: Option<u8>,
    frequency_hz: f32,
    body_formant_shift: f32,
) -> ReedTubeParams {
    let register = register_key_state(patch, current_note);
    ReedTubeParams {
        frequency_hz,
        loop_filter_cutoff_hz: brightness_hz(patch.brightness),
        loop_filter_resonance: 0.0,
        loop_gain: loop_gain_from_damping(patch.damping),
        loop_nonlinearity: 0.0,
        boundary_reflection: -0.75,
        pickup_position: 0.82,
        bell_radiation: bell_radiation_from_mix(patch.bell),
        bell_radiation_shape: patch.bell_radiation_shape,
        body_formant: patch.body_formant,
        body_formant_shift: math::finite_clamp(body_formant_shift, -2.0, 2.0, 0.0),
        register_mode_ratio: register.mode_ratio,
        register_vent_admittance: register.vent_admittance,
        register_vent_position: REGISTER_VENT_POSITION,
        body_odd_mode_projection: patch.body_odd_mode_projection,
        body_upper_odd_modes: patch.body_upper_odd_modes,
        reed_phase_delay_samples: 0.0,
        switches: ReedTubeSwitches {
            reed_enabled: true,
            bell_enabled: patch.switches.bell_enabled,
            bore_steepening_enabled: patch.switches.bore_steepening_enabled,
            body_enabled: patch.switches.body_enabled,
            clarinet_contour_enabled: patch.switches.clarinet_contour_enabled,
        },
    }
}

pub(super) fn reed_params(patch: &TubePatch) -> ReedParams {
    reed_params_with_mod(patch, 0.0, 0.0, 1.0, 0.0)
}

pub(super) fn reed_params_with_mod(
    patch: &TubePatch,
    pressure_mod: f32,
    embouchure_mod: f32,
    breath_noise: f32,
    tracking_frequency_hz: f32,
) -> ReedParams {
    ReedParams {
        pressure_depth: math::finite_clamp(
            patch.pressure * (1.0 + pressure_mod),
            0.0,
            1.0,
            patch.pressure,
        ),
        stiffness: patch.reed_stiffness,
        embouchure: math::finite_clamp(
            patch.embouchure + embouchure_mod,
            0.0,
            1.0,
            patch.embouchure,
        ),
        aperture_inertia: patch.reed_aperture_inertia,
        breath_noise,
        tracking_frequency_hz,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RegisterKeyState {
    pub(super) mode_ratio: f32,
    pub(super) vent_admittance: f32,
}

pub(super) fn register_key_state(patch: &TubePatch, current_note: Option<u8>) -> RegisterKeyState {
    let break_note = math::finite_clamp(patch.register_break_note, 48.0, 96.0, 69.0).round();
    let active = current_note
        .map(|note| note as f32 >= break_note)
        .unwrap_or(false);
    if active {
        RegisterKeyState {
            mode_ratio: REGISTER_MODE_RATIO,
            vent_admittance: REGISTER_VENT_ADMITTANCE,
        }
    } else {
        RegisterKeyState {
            mode_ratio: 1.0,
            vent_admittance: 0.0,
        }
    }
}

pub(super) fn brightness_hz(brightness: f32) -> f32 {
    let brightness = brightness.clamp(0.0, 1.0);
    MIN_BRIGHTNESS_HZ * (MAX_BRIGHTNESS_HZ / MIN_BRIGHTNESS_HZ).powf(brightness)
}

pub(super) fn loop_gain_from_damping(damping: f32) -> f32 {
    let damping = damping.clamp(0.0, 1.0);
    MAX_LOOP_GAIN + (MIN_LOOP_GAIN - MAX_LOOP_GAIN) * damping
}

pub(super) fn bell_radiation_from_mix(mix: f32) -> f32 {
    mix.clamp(0.0, 1.0).powf(BELL_MIX_EXPONENT)
}
