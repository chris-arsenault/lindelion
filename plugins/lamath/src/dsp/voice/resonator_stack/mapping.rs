use lindelion_dsp_utils::math::{finite_or, semitones_to_ratio, snap_to_zero};

use crate::ModalConfig;
use crate::dsp::{constants::LOWEST_RESONATOR_FREQUENCY_HZ, modal::ModalBankParams};

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

fn tuned_frequency(base_frequency: f32, semitone_offset: i8, cent_offset: f32) -> f32 {
    let base_frequency = if base_frequency.is_finite() && base_frequency > 0.0 {
        base_frequency
    } else {
        LOWEST_RESONATOR_FREQUENCY_HZ
    };
    let cent_offset = finite_or(cent_offset, 0.0);
    snap_to_zero(base_frequency * semitones_to_ratio(semitone_offset as f32 + cent_offset / 100.0))
}
