mod energy;
mod manual_grid;
mod pitch_stability;
mod spectral_flux;

pub use energy::{EnergyTransientDetector, StreamingEnergyTransientDetector};
pub use manual_grid::ManualGridDetector;
pub use pitch_stability::{PitchStabilityDetector, pitch_stability_markers_from_track};
pub use spectral_flux::{
    StreamingFluxFrame, StreamingSpectralFlux, StreamingSuperFluxDetector, SuperFluxDetector,
};

use lindelion_dsp_utils::math::{finite_clamp, ms_to_samples};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ONSET_SENSITIVITY: f32 = 0.5;
pub const DEFAULT_MIN_SLICE_MS: f32 = 50.0;
pub const DEFAULT_SUPERFLUX_LOOKBACK_FRAMES: u32 = 3;
pub const DEFAULT_SUPERFLUX_MAX_FILTER_RADIUS: u32 = 3;
pub const DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS: f32 = 120.0;
pub const DEFAULT_PITCH_STABILITY_DURATION_MS: f32 = 64.0;
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
            DetectionAlgorithm::SuperFlux
            | DetectionAlgorithm::ComplexFlux
            | DetectionAlgorithm::SpectralSparsity => SuperFluxDetector.detect(input, config),
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
    mut markers: Vec<SliceMarker>,
    min_gap_samples: usize,
    audio_len: usize,
) -> Vec<SliceMarker> {
    if audio_len == 0 {
        return Vec::new();
    }

    markers.sort_by_key(|marker| marker.position_samples);
    let mut deduped: Vec<SliceMarker> = Vec::new();
    for mut marker in markers {
        marker.position_samples = marker.position_samples.min(audio_len - 1);
        let far_enough = deduped
            .last()
            .map(|last| {
                marker
                    .position_samples
                    .saturating_sub(last.position_samples)
                    >= min_gap_samples
            })
            .unwrap_or(true);
        if far_enough {
            deduped.push(marker);
        }
    }
    if deduped.first().map(|marker| marker.position_samples) != Some(0) {
        deduped.insert(
            0,
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
        );
    }
    deduped
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
mod tests;
