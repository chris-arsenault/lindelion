mod energy;
mod flux_threshold;
mod manual_grid;
mod markers;
mod pitch_stability;
mod spectral_flux;

pub use energy::{EnergyTransientDetector, StreamingEnergyTransientDetector};
pub use manual_grid::ManualGridDetector;
pub use markers::{
    MarkerReconcileOutcome, MarkerReconcilePolicy, SliceRegion, normalize_markers,
    reconcile_markers, select_strongest_markers, slice_region_at_sample,
    slice_regions_from_markers, snap_markers_to_zero_crossings,
    snap_position_to_nearest_zero_crossing,
};
pub use pitch_stability::{PitchStabilityDetector, pitch_stability_markers_from_track};
pub use spectral_flux::{
    ComplexFluxDetector, SpectralSparsityDetector, StreamingFluxFrame, StreamingSpectralFlux,
    StreamingSuperFluxDetector, SuperFluxDetector,
};

use lindelion_dsp_utils::math::{finite_clamp, ms_to_samples};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ONSET_SENSITIVITY: f32 = 0.5;
pub const DEFAULT_MIN_SLICE_MS: f32 = 50.0;
pub const DEFAULT_SUPERFLUX_LOOKBACK_FRAMES: u32 = 3;
pub const DEFAULT_SUPERFLUX_MAX_FILTER_RADIUS: u32 = 3;
pub const DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT: f32 = 1.0;
pub const DEFAULT_SPECTRAL_SPARSITY_WINDOW_SIZE: usize = 1024;
pub const DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS: f32 = 120.0;
pub const DEFAULT_PITCH_STABILITY_DURATION_MS: f32 = 64.0;
pub const DEFAULT_ENERGY_TRANSIENT_FRAME_SIZE: usize = 512;
pub const DEFAULT_MANUAL_GRID_DIVISIONS: usize = 16;
pub const DEFAULT_MANUAL_GRID_OFFSET_MS: f32 = 0.0;
pub const ENERGY_TRANSIENT_BASE_THRESHOLD: f32 = 0.02;
pub const ENERGY_TRANSIENT_SENSITIVITY_RANGE: f32 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionAlgorithm {
    SuperFlux,
    ComplexFlux,
    SpectralSparsity,
    PitchStability,
    EnergyTransient,
    ManualGrid,
}

impl DetectionAlgorithm {
    pub const ALL: [Self; 6] = [
        Self::SuperFlux,
        Self::ComplexFlux,
        Self::SpectralSparsity,
        Self::PitchStability,
        Self::EnergyTransient,
        Self::ManualGrid,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerKind {
    Auto,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceMarker {
    pub position_samples: usize,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AlgorithmParams {
    SuperFlux {
        lookback_frames: u32,
        max_filter_radius: u32,
    },
    ComplexFlux {
        lookback_frames: u32,
        group_delay_weight: f32,
    },
    SpectralSparsity {
        window_size: usize,
    },
    PitchStability {
        threshold_cents: f32,
        min_stable_duration_ms: f32,
    },
    EnergyTransient {
        frame_size: usize,
    },
    ManualGrid {
        divisions: usize,
        offset_ms: f32,
    },
}

impl AlgorithmParams {
    pub const fn algorithm(self) -> DetectionAlgorithm {
        match self {
            Self::SuperFlux { .. } => DetectionAlgorithm::SuperFlux,
            Self::ComplexFlux { .. } => DetectionAlgorithm::ComplexFlux,
            Self::SpectralSparsity { .. } => DetectionAlgorithm::SpectralSparsity,
            Self::PitchStability { .. } => DetectionAlgorithm::PitchStability,
            Self::EnergyTransient { .. } => DetectionAlgorithm::EnergyTransient,
            Self::ManualGrid { .. } => DetectionAlgorithm::ManualGrid,
        }
    }

    pub fn default_for_algorithm(
        algorithm: DetectionAlgorithm,
        profile: OnsetDetectionProfile,
    ) -> Self {
        let profile = profile.sanitized();
        match algorithm {
            DetectionAlgorithm::SuperFlux => profile.superflux_params(),
            DetectionAlgorithm::ComplexFlux => Self::ComplexFlux {
                lookback_frames: profile.lookback_frames,
                group_delay_weight: DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT,
            },
            DetectionAlgorithm::SpectralSparsity => Self::SpectralSparsity {
                window_size: DEFAULT_SPECTRAL_SPARSITY_WINDOW_SIZE,
            },
            DetectionAlgorithm::PitchStability => profile.pitch_stability_params(),
            DetectionAlgorithm::EnergyTransient => Self::EnergyTransient {
                frame_size: DEFAULT_ENERGY_TRANSIENT_FRAME_SIZE,
            },
            DetectionAlgorithm::ManualGrid => Self::ManualGrid {
                divisions: DEFAULT_MANUAL_GRID_DIVISIONS,
                offset_ms: DEFAULT_MANUAL_GRID_OFFSET_MS,
            },
        }
    }

    pub fn sanitized_for_algorithm(
        self,
        algorithm: DetectionAlgorithm,
        profile: OnsetDetectionProfile,
    ) -> Self {
        let profile = profile.sanitized();
        match (algorithm, self) {
            (
                DetectionAlgorithm::SuperFlux,
                Self::SuperFlux {
                    lookback_frames,
                    max_filter_radius,
                },
            ) => Self::SuperFlux {
                lookback_frames: lookback_frames.clamp(1, 32),
                max_filter_radius: max_filter_radius.min(32),
            },
            (
                DetectionAlgorithm::ComplexFlux,
                Self::ComplexFlux {
                    lookback_frames,
                    group_delay_weight,
                },
            ) => Self::ComplexFlux {
                lookback_frames: lookback_frames.clamp(1, 32),
                group_delay_weight: finite_clamp(
                    group_delay_weight,
                    0.0,
                    8.0,
                    DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT,
                ),
            },
            (DetectionAlgorithm::SpectralSparsity, Self::SpectralSparsity { window_size }) => {
                Self::SpectralSparsity {
                    window_size: window_size.clamp(64, 8192),
                }
            }
            (
                DetectionAlgorithm::PitchStability,
                Self::PitchStability {
                    threshold_cents,
                    min_stable_duration_ms,
                },
            ) => Self::PitchStability {
                threshold_cents: finite_clamp(
                    threshold_cents,
                    1.0,
                    2_400.0,
                    DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
                ),
                min_stable_duration_ms: finite_clamp(
                    min_stable_duration_ms,
                    1.0,
                    5_000.0,
                    DEFAULT_PITCH_STABILITY_DURATION_MS,
                ),
            },
            (DetectionAlgorithm::EnergyTransient, Self::EnergyTransient { frame_size }) => {
                Self::EnergyTransient {
                    frame_size: frame_size.clamp(32, 8192),
                }
            }
            (
                DetectionAlgorithm::ManualGrid,
                Self::ManualGrid {
                    divisions,
                    offset_ms,
                },
            ) => Self::ManualGrid {
                divisions: divisions.clamp(1, 1024),
                offset_ms: finite_clamp(offset_ms, 0.0, 60_000.0, DEFAULT_MANUAL_GRID_OFFSET_MS),
            },
            _ => Self::default_for_algorithm(algorithm, profile),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OnsetDetectionProfile {
    pub lookback_frames: u32,
    pub max_filter_radius: u32,
    pub pitch_stability_threshold_cents: f32,
    pub pitch_stability_duration_ms: f32,
}

impl Default for OnsetDetectionProfile {
    fn default() -> Self {
        Self {
            lookback_frames: DEFAULT_SUPERFLUX_LOOKBACK_FRAMES,
            max_filter_radius: DEFAULT_SUPERFLUX_MAX_FILTER_RADIUS,
            pitch_stability_threshold_cents: DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
            pitch_stability_duration_ms: DEFAULT_PITCH_STABILITY_DURATION_MS,
        }
    }
}

impl OnsetDetectionProfile {
    pub fn relaxed() -> Self {
        Self {
            lookback_frames: 5,
            max_filter_radius: 4,
            pitch_stability_threshold_cents: 160.0,
            pitch_stability_duration_ms: 80.0,
        }
    }

    pub fn aggressive() -> Self {
        Self {
            lookback_frames: 2,
            max_filter_radius: 2,
            pitch_stability_threshold_cents: 80.0,
            pitch_stability_duration_ms: 48.0,
        }
    }

    pub fn sanitized(self) -> Self {
        Self {
            lookback_frames: self.lookback_frames.clamp(1, 32),
            max_filter_radius: self.max_filter_radius.min(32),
            pitch_stability_threshold_cents: finite_clamp(
                self.pitch_stability_threshold_cents,
                1.0,
                2_400.0,
                DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS,
            ),
            pitch_stability_duration_ms: finite_clamp(
                self.pitch_stability_duration_ms,
                1.0,
                5_000.0,
                DEFAULT_PITCH_STABILITY_DURATION_MS,
            ),
        }
    }

    pub fn superflux_params(self) -> AlgorithmParams {
        let profile = self.sanitized();
        AlgorithmParams::SuperFlux {
            lookback_frames: profile.lookback_frames,
            max_filter_radius: profile.max_filter_radius,
        }
    }

    pub fn pitch_stability_params(self) -> AlgorithmParams {
        let profile = self.sanitized();
        AlgorithmParams::PitchStability {
            threshold_cents: profile.pitch_stability_threshold_cents,
            min_stable_duration_ms: profile.pitch_stability_duration_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub algorithm: DetectionAlgorithm,
    pub sensitivity: f32,
    pub min_slice_ms: f32,
    #[serde(default)]
    pub profile: OnsetDetectionProfile,
    pub params: AlgorithmParams,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        let profile = OnsetDetectionProfile::default();
        Self {
            algorithm: DetectionAlgorithm::SuperFlux,
            sensitivity: DEFAULT_ONSET_SENSITIVITY,
            min_slice_ms: DEFAULT_MIN_SLICE_MS,
            profile,
            params: profile.superflux_params(),
        }
    }
}

impl DetectionConfig {
    pub fn superflux(sensitivity: f32, min_slice_ms: f32, profile: OnsetDetectionProfile) -> Self {
        let profile = profile.sanitized();
        Self {
            algorithm: DetectionAlgorithm::SuperFlux,
            sensitivity,
            min_slice_ms,
            profile,
            params: profile.superflux_params(),
        }
        .sanitized()
    }

    pub fn with_algorithm(self, algorithm: DetectionAlgorithm) -> Self {
        let profile = self.effective_profile();
        Self {
            algorithm,
            profile,
            params: self.params.sanitized_for_algorithm(algorithm, profile),
            ..self
        }
        .sanitized()
    }

    pub fn effective_profile(self) -> OnsetDetectionProfile {
        onset_profile(self)
    }

    pub fn sanitized(self) -> Self {
        let profile = self.profile.sanitized();
        let algorithm = self.algorithm;
        Self {
            algorithm,
            sensitivity: finite_clamp(self.sensitivity, 0.0, 1.0, DEFAULT_ONSET_SENSITIVITY),
            min_slice_ms: finite_clamp(self.min_slice_ms, 0.0, 60_000.0, DEFAULT_MIN_SLICE_MS),
            profile,
            params: self.params.sanitized_for_algorithm(algorithm, profile),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchTrack<'a> {
    pub source_sample_rate: u32,
    pub frame_hop_samples: usize,
    pub frames: &'a [PitchFrame],
}

impl<'a> PitchTrack<'a> {
    pub fn from_contour(contour: &'a PitchContour) -> Self {
        Self {
            source_sample_rate: contour.source_sample_rate,
            frame_hop_samples: contour.source_frame_hop_samples(),
            frames: &contour.frames,
        }
    }
}

impl<'a> From<&'a PitchContour> for PitchTrack<'a> {
    fn from(contour: &'a PitchContour) -> Self {
        Self::from_contour(contour)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnsetDetectionInput<'a> {
    pub audio: &'a [f32],
    pub sample_rate: u32,
    pub pitch_track: Option<PitchTrack<'a>>,
}

impl<'a> OnsetDetectionInput<'a> {
    pub const fn new(audio: &'a [f32], sample_rate: u32) -> Self {
        Self {
            audio,
            sample_rate,
            pitch_track: None,
        }
    }

    pub fn with_pitch_track(mut self, pitch_track: PitchTrack<'a>) -> Self {
        self.pitch_track = Some(pitch_track);
        self
    }

    pub fn with_pitch_contour(self, pitch_contour: &'a PitchContour) -> Self {
        self.with_pitch_track(PitchTrack::from_contour(pitch_contour))
    }
}

pub trait OnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker>;
}

pub trait StreamingOnsetDetector {
    fn next_block(&mut self, audio: &[f32]) -> &[SliceMarker];
    fn reset(&mut self);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ConfiguredOnsetDetector;

impl OnsetDetector for ConfiguredOnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        match config.algorithm {
            DetectionAlgorithm::SuperFlux => SuperFluxDetector.detect(input, config),
            DetectionAlgorithm::ComplexFlux => ComplexFluxDetector.detect(input, config),
            DetectionAlgorithm::SpectralSparsity => SpectralSparsityDetector.detect(input, config),
            DetectionAlgorithm::EnergyTransient => EnergyTransientDetector.detect(input, config),
            DetectionAlgorithm::ManualGrid => ManualGridDetector.detect(input, config),
            DetectionAlgorithm::PitchStability => PitchStabilityDetector.detect(input, config),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HybridOnsetDetector;

impl OnsetDetector for HybridOnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        let profile = onset_profile(config);
        let mut markers = SuperFluxDetector.detect(
            input,
            DetectionConfig {
                algorithm: DetectionAlgorithm::SuperFlux,
                profile,
                params: profile.superflux_params(),
                ..config
            },
        );

        let Some(pitch_track) = input.pitch_track else {
            return markers;
        };

        markers.extend(PitchStabilityDetector.detect(input.with_pitch_track(pitch_track), config));
        dedupe_markers(
            markers,
            ms_to_samples(config.min_slice_ms, input.sample_rate),
            input.audio.len(),
        )
    }
}

pub(crate) fn dedupe_markers(
    markers: Vec<SliceMarker>,
    min_gap_samples: usize,
    audio_len: usize,
) -> Vec<SliceMarker> {
    normalize_markers(markers, min_gap_samples, audio_len)
}

pub(crate) fn onset_profile(config: DetectionConfig) -> OnsetDetectionProfile {
    let mut profile = config.profile.sanitized();
    if config.profile == OnsetDetectionProfile::default() {
        match config.params {
            AlgorithmParams::SuperFlux {
                lookback_frames,
                max_filter_radius,
            } => {
                profile.lookback_frames = lookback_frames;
                profile.max_filter_radius = max_filter_radius;
            }
            AlgorithmParams::PitchStability {
                threshold_cents,
                min_stable_duration_ms,
            } => {
                profile.pitch_stability_threshold_cents = threshold_cents;
                profile.pitch_stability_duration_ms = min_stable_duration_ms;
            }
            _ => {}
        }
    }
    profile.sanitized()
}

#[cfg(test)]
mod spectral_flux_tests;
#[cfg(test)]
mod tests;
