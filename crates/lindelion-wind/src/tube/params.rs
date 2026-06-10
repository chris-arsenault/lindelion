//! Reed-tube parameter surface, model switches, and the render-tap snapshot.

use lindelion_dsp_utils::math;

use super::super::{
    BOUNDARY_REFLECTION_DEFAULT, LOOP_FILTER_CUTOFF_DEFAULT_HZ, LOOP_FILTER_RESONANCE_DEFAULT,
    LOOP_GAIN_DEFAULT, PICKUP_POSITION_DEFAULT,
};
use super::{
    BODY_ODD_MODE_PROJECTION_DEFAULT, REGISTER_MODE_RATIO_MAX, REGISTER_MODE_RATIO_MIN,
    REGISTER_VENT_ADMITTANCE_MAX, REGISTER_VENT_POSITION_DEFAULT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReedTubeSwitches {
    pub reed_enabled: bool,
    pub bell_enabled: bool,
    pub bore_steepening_enabled: bool,
    pub body_enabled: bool,
    pub clarinet_contour_enabled: bool,
}

impl Default for ReedTubeSwitches {
    fn default() -> Self {
        Self {
            reed_enabled: true,
            bell_enabled: true,
            bore_steepening_enabled: true,
            body_enabled: true,
            clarinet_contour_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReedTubeParams {
    pub frequency_hz: f32,
    pub loop_filter_cutoff_hz: f32,
    pub loop_filter_resonance: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub boundary_reflection: f32,
    pub pickup_position: f32,
    pub bell_radiation: f32,
    pub bell_radiation_shape: f32,
    pub body_formant: f32,
    pub body_formant_shift: f32,
    /// Ratio between sounding frequency and bore fundamental. A register-keyed clarinet note
    /// speaks on the third bore mode, so `3.0` keeps the long low-register bore while the sounding
    /// frequency stays high.
    pub register_mode_ratio: f32,
    /// Effective side-hole admittance for the register vent. `0.0` is closed; larger values leak
    /// pressure at `register_vent_position`, suppressing the fundamental and encouraging the third
    /// mode.
    pub register_vent_admittance: f32,
    /// Normalized bore position of the register vent, measured from the mouthpiece.
    pub register_vent_position: f32,
    /// Mix amount for the closed-open body projection. `1.0` is the physical odd-mode
    /// projection `0.5 * (x[n] - x[n - T/2])`, which rejects even harmonics at the radiating body
    /// input; lower values leak direct pickup pressure into the body path.
    pub body_odd_mode_projection: f32,
    /// Strength of the low-register upper odd body/radiation mode bank. These tracked h9/h11/h13
    /// modes fill the clarinet tail above the primary h3/h5/h7 body resonances.
    pub body_upper_odd_modes: f32,
    /// Phase delay (samples) the inertial reed aperture adds to the feedback loop at the
    /// playing frequency, supplied by the driving [`crate::ReedDriver`]. Folded into the
    /// bore-length tuning so the reed's loop phase is compensated like the mouth-loss and
    /// damping filters; `0.0` (instant aperture) leaves tuning unchanged.
    pub reed_phase_delay_samples: f32,
    pub switches: ReedTubeSwitches,
}

impl Default for ReedTubeParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            loop_filter_cutoff_hz: LOOP_FILTER_CUTOFF_DEFAULT_HZ,
            loop_filter_resonance: LOOP_FILTER_RESONANCE_DEFAULT,
            loop_gain: LOOP_GAIN_DEFAULT,
            loop_nonlinearity: 0.0,
            boundary_reflection: BOUNDARY_REFLECTION_DEFAULT,
            pickup_position: PICKUP_POSITION_DEFAULT,
            bell_radiation: 1.0,
            bell_radiation_shape: 0.0,
            body_formant: 0.0,
            body_formant_shift: 0.0,
            register_mode_ratio: 1.0,
            register_vent_admittance: 0.0,
            register_vent_position: REGISTER_VENT_POSITION_DEFAULT,
            body_odd_mode_projection: BODY_ODD_MODE_PROJECTION_DEFAULT,
            body_upper_odd_modes: 1.0,
            reed_phase_delay_samples: 0.0,
            switches: ReedTubeSwitches::default(),
        }
    }
}

impl ReedTubeParams {
    pub fn sanitized(self) -> Self {
        let fallback = Self::default();
        Self {
            frequency_hz: math::finite_clamp(
                self.frequency_hz,
                1.0,
                22_000.0,
                fallback.frequency_hz,
            ),
            loop_filter_cutoff_hz: math::finite_clamp(
                self.loop_filter_cutoff_hz,
                20.0,
                22_000.0,
                fallback.loop_filter_cutoff_hz,
            ),
            loop_filter_resonance: unit(self.loop_filter_resonance, fallback.loop_filter_resonance),
            loop_gain: math::finite_clamp(self.loop_gain, 0.0, 0.999, fallback.loop_gain),
            loop_nonlinearity: unit(self.loop_nonlinearity, fallback.loop_nonlinearity),
            boundary_reflection: math::finite_clamp(
                self.boundary_reflection,
                -1.0,
                1.0,
                fallback.boundary_reflection,
            ),
            pickup_position: math::finite_clamp(
                self.pickup_position,
                0.001,
                0.999,
                fallback.pickup_position,
            ),
            bell_radiation: unit(self.bell_radiation, fallback.bell_radiation),
            bell_radiation_shape: unit(self.bell_radiation_shape, fallback.bell_radiation_shape),
            body_formant: unit(self.body_formant, fallback.body_formant),
            body_formant_shift: math::finite_clamp(
                self.body_formant_shift,
                -2.0,
                2.0,
                fallback.body_formant_shift,
            ),
            register_mode_ratio: math::finite_clamp(
                self.register_mode_ratio,
                REGISTER_MODE_RATIO_MIN,
                REGISTER_MODE_RATIO_MAX,
                fallback.register_mode_ratio,
            ),
            register_vent_admittance: math::finite_clamp(
                self.register_vent_admittance,
                0.0,
                REGISTER_VENT_ADMITTANCE_MAX,
                fallback.register_vent_admittance,
            ),
            register_vent_position: math::finite_clamp(
                self.register_vent_position,
                0.05,
                0.95,
                fallback.register_vent_position,
            ),
            body_odd_mode_projection: unit(
                self.body_odd_mode_projection,
                fallback.body_odd_mode_projection,
            ),
            body_upper_odd_modes: unit(self.body_upper_odd_modes, fallback.body_upper_odd_modes),
            reed_phase_delay_samples: math::finite_clamp(
                self.reed_phase_delay_samples,
                0.0,
                24.0,
                fallback.reed_phase_delay_samples,
            ),
            switches: ReedTubeSwitches {
                reed_enabled: true,
                ..self.switches
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReedTubeTaps {
    pub mouth_wave: f32,
    pub mouth_incident: f32,
    pub mouth_filtered: f32,
    pub mouth_reflection: f32,
    pub bell_incident: f32,
    pub bell_reflection: f32,
    pub bell_pressure: f32,
    pub pickup_left: f32,
    pub pickup_right: f32,
    pub pickup_pressure: f32,
    pub pickup_flow: f32,
    pub pickup_sample: f32,
    pub body_input: f32,
    pub body_output: f32,
    pub body_reaction_flow: f32,
    pub bell_radiated: f32,
    pub register_vent_flow: f32,
    pub register_vent_output: f32,
    pub body_bell_sum: f32,
    pub main_output: f32,
    pub final_output: f32,
}

fn unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}
