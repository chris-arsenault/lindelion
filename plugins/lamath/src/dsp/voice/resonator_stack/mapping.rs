//! Pure mappings from per-slot patch configs to the resonators' DSP parameter
//! structs, including the shared pitch (semitone/cent) tuning.

use lindelion_dsp_utils::math::{finite_clamp, finite_or, semitones_to_ratio, snap_to_zero};

use crate::dsp::constants::{
    LOWEST_RESONATOR_FREQUENCY_HZ, MODAL_DAMPING_MOD_OCTAVES, RESONATOR_POSITION_MOD_DEPTH,
    STRIKE_POSITION, WAVEGUIDE_DAMPING_MOD_DEPTH, WAVEGUIDE_DISPERSION, WAVEGUIDE_LOOP_GAIN,
    WAVEGUIDE_PICKUP_POSITION,
};
use crate::dsp::modal::ModalBankParams;
use crate::dsp::waveguide::{MeshVoiceParams, WaveguideParams};
use crate::{MeshConfig, ModalConfig, ResonatorConfig, WaveguideConfig};

/// The waveguide loop gain governing a voice's structural ramp length: whichever slot
/// holds a waveguide supplies it, else the default.
pub(super) fn loop_gain_from_configs(
    resonator_a: ResonatorConfig,
    resonator_b: ResonatorConfig,
) -> f32 {
    match (resonator_a, resonator_b) {
        (ResonatorConfig::Waveguide(config), _) => WAVEGUIDE_LOOP_GAIN.clamp(config.loop_gain),
        (_, ResonatorConfig::Waveguide(config)) => WAVEGUIDE_LOOP_GAIN.clamp(config.loop_gain),
        _ => WAVEGUIDE_LOOP_GAIN.default,
    }
}

/// Apply live damping/position modulation to a per-slot config, clamped per model.
pub(super) fn modulated_resonator_config(
    config: ResonatorConfig,
    damping_mod: f32,
    position_mod: f32,
) -> ResonatorConfig {
    match config {
        ResonatorConfig::Modal(mut config) => {
            config.decay_global = (config.decay_global
                * 2.0_f32.powf(damping_mod * MODAL_DAMPING_MOD_OCTAVES))
            .clamp(0.01, 10.0);
            config.position_of_strike = STRIKE_POSITION
                .clamp(config.position_of_strike + position_mod * RESONATOR_POSITION_MOD_DEPTH);
            ResonatorConfig::Modal(config)
        }
        ResonatorConfig::Waveguide(mut config) => {
            config.loop_gain = WAVEGUIDE_LOOP_GAIN
                .clamp(config.loop_gain + damping_mod * WAVEGUIDE_DAMPING_MOD_DEPTH);
            config.position_of_strike = STRIKE_POSITION
                .clamp(config.position_of_strike + position_mod * RESONATOR_POSITION_MOD_DEPTH);
            ResonatorConfig::Waveguide(config)
        }
        ResonatorConfig::Mesh(mut config) => {
            // Positive damping modulation lengthens the decay, so it lowers the
            // mesh's boundary loss.
            config.damping =
                (config.damping - damping_mod * WAVEGUIDE_DAMPING_MOD_DEPTH).clamp(0.0, 1.0);
            config.position_of_strike = STRIKE_POSITION
                .clamp(config.position_of_strike + position_mod * RESONATOR_POSITION_MOD_DEPTH);
            ResonatorConfig::Mesh(config)
        }
    }
}

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
        // The contact stage (M9) overrides this per (oversampled) sample from the
        // ContactConfig spread + playing effort; the config-time value is the
        // pre-M9 narrow default.
        excitation_spread: 0.0,
        source_body_balance: finite_clamp(config.source_body_balance, 0.0, 1.0, 0.0),
        bell_radiation: finite_clamp(config.bell_radiation, 0.0, 1.0, 1.0),
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
