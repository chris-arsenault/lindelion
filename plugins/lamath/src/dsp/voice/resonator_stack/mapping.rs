//! Pure mappings from per-slot patch configs to the resonators' DSP parameter
//! structs, including the shared pitch (semitone/cent) tuning.

use lindelion_dsp_utils::math::{finite_or, semitones_to_ratio, snap_to_zero};

use crate::dsp::constants::{
    LOWEST_RESONATOR_FREQUENCY_HZ, WAVEGUIDE_DISPERSION, WAVEGUIDE_PICKUP_POSITION,
};
use crate::dsp::modal::ModalBankParams;
use crate::dsp::waveguide::{MeshVoiceParams, WaveguideParams};
use crate::{MeshConfig, ModalConfig, WaveguideConfig};

pub(super) fn modal_params_from_config(
    config: &ModalConfig,
    base_frequency: f32,
) -> ModalBankParams {
    ModalBankParams {
        fundamental_hz: tuned_frequency(base_frequency, config.semitone_offset, config.cent_offset),
        mode_count: config.mode_count as usize,
        preset: config.preset,
        inharmonicity: config.inharmonicity,
        brightness: config.brightness,
        decay_global: config.decay_global,
        decay_tilt: config.decay_tilt,
        position_of_strike: config.position_of_strike,
    }
}

pub(super) fn waveguide_params_from_config(
    config: &WaveguideConfig,
    base_frequency: f32,
) -> WaveguideParams {
    WaveguideParams {
        style: config.style,
        frequency_hz: tuned_frequency(base_frequency, config.semitone_offset, config.cent_offset),
        loop_filter_cutoff: config.loop_filter_cutoff,
        loop_filter_resonance: config.loop_filter_resonance,
        loop_gain: config.loop_gain,
        loop_nonlinearity: config.loop_nonlinearity,
        dispersion: WAVEGUIDE_DISPERSION.clamp(config.dispersion),
        position_of_strike: config.position_of_strike,
        pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
        boundary_reflection: config.boundary_reflection,
    }
}

pub(super) fn mesh_params_from_config(config: &MeshConfig, base_frequency: f32) -> MeshVoiceParams {
    MeshVoiceParams {
        frequency_hz: tuned_frequency(base_frequency, config.semitone_offset, config.cent_offset),
        material: config.material,
        size: config.size,
        damping: config.damping,
        tension: config.tension,
        strike_position: config.position_of_strike,
        pickup_spread: config.pickup_spread,
    }
}

fn tuned_frequency(base_frequency: f32, semitone_offset: i8, cent_offset: f32) -> f32 {
    let base_frequency = if base_frequency.is_finite() && base_frequency > 0.0 {
        base_frequency
    } else {
        LOWEST_RESONATOR_FREQUENCY_HZ
    };
    let cent_offset = finite_or(cent_offset, 0.0);
    snap_to_zero(base_frequency * semitones_to_ratio(semitone_offset as f32 + cent_offset / 100.0))
}
