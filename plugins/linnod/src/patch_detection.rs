use lindelion_dsp_utils::math::finite_clamp;
use lindelion_onset_detect::{
    AlgorithmParams, DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT, DEFAULT_MANUAL_GRID_DIVISIONS,
    DEFAULT_MANUAL_GRID_OFFSET_MS, DEFAULT_PITCH_STABILITY_DURATION_MS,
    DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS, DetectionAlgorithm, DetectionConfig,
};

use crate::patch::SLICE_COUNT;

const MIN_DETECTION_MIN_SLICE_MS: f32 = 0.0;
const MAX_DETECTION_MIN_SLICE_MS: f32 = 2_000.0;
const MIN_DETECTION_MANUAL_GRID_DIVISIONS: usize = 1;
const MAX_DETECTION_MANUAL_GRID_DIVISIONS: usize = SLICE_COUNT;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectionEdit {
    Algorithm(DetectionAlgorithm),
    MinSliceMs(f32),
    LookbackFrames(u32),
    MaxFilterRadius(u32),
    GroupDelayWeight(f32),
    SpectralWindowSize(usize),
    PitchStabilityThresholdCents(f32),
    PitchStabilityDurationMs(f32),
    EnergyFrameSize(usize),
    ManualGridDivisions(usize),
    ManualGridOffsetMs(f32),
}

pub(crate) fn detection_config_after_edit(
    config: DetectionConfig,
    edit: DetectionEdit,
) -> DetectionConfig {
    let config = config.sanitized();
    match edit {
        DetectionEdit::Algorithm(algorithm) => config.with_algorithm(algorithm),
        DetectionEdit::MinSliceMs(min_slice_ms) => detection_with_min_slice(config, min_slice_ms),
        DetectionEdit::LookbackFrames(lookback_frames) => {
            detection_with_lookback_frames(config, lookback_frames)
        }
        DetectionEdit::MaxFilterRadius(max_filter_radius) => {
            detection_with_max_filter_radius(config, max_filter_radius)
        }
        DetectionEdit::GroupDelayWeight(group_delay_weight) => {
            detection_with_group_delay_weight(config, group_delay_weight)
        }
        DetectionEdit::SpectralWindowSize(window_size) => {
            detection_with_spectral_window_size(config, window_size)
        }
        DetectionEdit::PitchStabilityThresholdCents(threshold_cents) => {
            detection_with_pitch_threshold(config, threshold_cents)
        }
        DetectionEdit::PitchStabilityDurationMs(min_stable_duration_ms) => {
            detection_with_pitch_duration(config, min_stable_duration_ms)
        }
        DetectionEdit::EnergyFrameSize(frame_size) => {
            detection_with_energy_frame_size(config, frame_size)
        }
        DetectionEdit::ManualGridDivisions(divisions) => {
            detection_with_manual_grid_divisions(config, divisions)
        }
        DetectionEdit::ManualGridOffsetMs(offset_ms) => {
            detection_with_manual_grid_offset(config, offset_ms)
        }
    }
    .sanitized()
}

fn detection_with_min_slice(mut config: DetectionConfig, min_slice_ms: f32) -> DetectionConfig {
    config.min_slice_ms = finite_clamp(
        min_slice_ms,
        MIN_DETECTION_MIN_SLICE_MS,
        MAX_DETECTION_MIN_SLICE_MS,
        lindelion_onset_detect::DEFAULT_MIN_SLICE_MS,
    );
    config
}

fn detection_with_lookback_frames(
    mut config: DetectionConfig,
    lookback_frames: u32,
) -> DetectionConfig {
    let lookback_frames = lookback_frames.clamp(1, 32);
    match config.algorithm {
        DetectionAlgorithm::SuperFlux => {
            let (_, max_filter_radius) = superflux_param_values(config);
            config.params = AlgorithmParams::SuperFlux {
                lookback_frames,
                max_filter_radius,
            };
        }
        DetectionAlgorithm::ComplexFlux => {
            let (_, group_delay_weight) = complex_flux_param_values(config);
            config.params = AlgorithmParams::ComplexFlux {
                lookback_frames,
                group_delay_weight,
            };
        }
        _ => {}
    }
    config.profile.lookback_frames = lookback_frames;
    config
}

fn detection_with_max_filter_radius(
    mut config: DetectionConfig,
    max_filter_radius: u32,
) -> DetectionConfig {
    let (lookback_frames, _) = superflux_param_values(config);
    let max_filter_radius = max_filter_radius.min(32);
    config.profile.max_filter_radius = max_filter_radius;
    config.params = AlgorithmParams::SuperFlux {
        lookback_frames,
        max_filter_radius,
    };
    config.algorithm = DetectionAlgorithm::SuperFlux;
    config
}

fn detection_with_group_delay_weight(
    mut config: DetectionConfig,
    group_delay_weight: f32,
) -> DetectionConfig {
    let (lookback_frames, _) = complex_flux_param_values(config);
    config.params = AlgorithmParams::ComplexFlux {
        lookback_frames,
        group_delay_weight: finite_clamp(
            group_delay_weight,
            0.0,
            8.0,
            DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT,
        ),
    };
    config.algorithm = DetectionAlgorithm::ComplexFlux;
    config
}

fn detection_with_spectral_window_size(
    mut config: DetectionConfig,
    window_size: usize,
) -> DetectionConfig {
    config.params = AlgorithmParams::SpectralSparsity {
        window_size: window_size.clamp(64, 8192),
    };
    config.algorithm = DetectionAlgorithm::SpectralSparsity;
    config
}

fn detection_with_pitch_threshold(
    mut config: DetectionConfig,
    threshold_cents: f32,
) -> DetectionConfig {
    let (_, min_stable_duration_ms) = pitch_stability_param_values(config);
    config.profile.pitch_stability_threshold_cents = finite_clamp(
        threshold_cents,
        1.0,
        2_400.0,
        DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
    );
    config.params = AlgorithmParams::PitchStability {
        threshold_cents: config.profile.pitch_stability_threshold_cents,
        min_stable_duration_ms,
    };
    config.algorithm = DetectionAlgorithm::PitchStability;
    config
}

fn detection_with_pitch_duration(
    mut config: DetectionConfig,
    min_stable_duration_ms: f32,
) -> DetectionConfig {
    let (threshold_cents, _) = pitch_stability_param_values(config);
    config.profile.pitch_stability_duration_ms = finite_clamp(
        min_stable_duration_ms,
        1.0,
        5_000.0,
        DEFAULT_PITCH_STABILITY_DURATION_MS,
    );
    config.params = AlgorithmParams::PitchStability {
        threshold_cents,
        min_stable_duration_ms: config.profile.pitch_stability_duration_ms,
    };
    config.algorithm = DetectionAlgorithm::PitchStability;
    config
}

fn detection_with_energy_frame_size(
    mut config: DetectionConfig,
    frame_size: usize,
) -> DetectionConfig {
    config.params = AlgorithmParams::EnergyTransient {
        frame_size: frame_size.clamp(32, 8192),
    };
    config.algorithm = DetectionAlgorithm::EnergyTransient;
    config
}

fn detection_with_manual_grid_divisions(
    mut config: DetectionConfig,
    divisions: usize,
) -> DetectionConfig {
    let (_, offset_ms) = manual_grid_param_values(config);
    config.params = AlgorithmParams::ManualGrid {
        divisions: divisions.clamp(
            MIN_DETECTION_MANUAL_GRID_DIVISIONS,
            MAX_DETECTION_MANUAL_GRID_DIVISIONS,
        ),
        offset_ms,
    };
    config.algorithm = DetectionAlgorithm::ManualGrid;
    config
}

fn detection_with_manual_grid_offset(
    mut config: DetectionConfig,
    offset_ms: f32,
) -> DetectionConfig {
    let (divisions, _) = manual_grid_param_values(config);
    config.params = AlgorithmParams::ManualGrid {
        divisions,
        offset_ms: finite_clamp(offset_ms, 0.0, 60_000.0, DEFAULT_MANUAL_GRID_OFFSET_MS),
    };
    config.algorithm = DetectionAlgorithm::ManualGrid;
    config
}

fn superflux_param_values(config: DetectionConfig) -> (u32, u32) {
    let profile = config.effective_profile();
    match config.params {
        AlgorithmParams::SuperFlux {
            lookback_frames,
            max_filter_radius,
        } => (lookback_frames.clamp(1, 32), max_filter_radius.min(32)),
        _ => (profile.lookback_frames, profile.max_filter_radius),
    }
}

fn complex_flux_param_values(config: DetectionConfig) -> (u32, f32) {
    let profile = config.effective_profile();
    match config.params {
        AlgorithmParams::ComplexFlux {
            lookback_frames,
            group_delay_weight,
        } => (
            lookback_frames.clamp(1, 32),
            finite_clamp(
                group_delay_weight,
                0.0,
                8.0,
                DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT,
            ),
        ),
        _ => (
            profile.lookback_frames,
            DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT,
        ),
    }
}

fn pitch_stability_param_values(config: DetectionConfig) -> (f32, f32) {
    let profile = config.effective_profile();
    match config.params {
        AlgorithmParams::PitchStability {
            threshold_cents,
            min_stable_duration_ms,
        } => (
            finite_clamp(
                threshold_cents,
                1.0,
                2_400.0,
                DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
            ),
            finite_clamp(
                min_stable_duration_ms,
                1.0,
                5_000.0,
                DEFAULT_PITCH_STABILITY_DURATION_MS,
            ),
        ),
        _ => (
            profile.pitch_stability_threshold_cents,
            profile.pitch_stability_duration_ms,
        ),
    }
}

fn manual_grid_param_values(config: DetectionConfig) -> (usize, f32) {
    match config.params {
        AlgorithmParams::ManualGrid {
            divisions,
            offset_ms,
        } => (
            divisions.clamp(
                MIN_DETECTION_MANUAL_GRID_DIVISIONS,
                MAX_DETECTION_MANUAL_GRID_DIVISIONS,
            ),
            finite_clamp(offset_ms, 0.0, 60_000.0, DEFAULT_MANUAL_GRID_OFFSET_MS),
        ),
        _ => (DEFAULT_MANUAL_GRID_DIVISIONS, DEFAULT_MANUAL_GRID_OFFSET_MS),
    }
}
