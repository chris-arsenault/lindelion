use lindelion_dsp_utils::math::finite_clamp;
use lindelion_midi::{QuantizeSettings, RootNote, Scale, SnapMode, TimingGrid};
use lindelion_onset_detect::{DetectionConfig, OnsetDetectionProfile};
use lindelion_phrase_analysis::{NoteSegmentationConfig, PhraseAnalysisConfig};
use lindelion_pitch_detect::{PitchDetectionConfig, SWIFTF0_MODEL_FMAX_HZ, SWIFTF0_MODEL_FMIN_HZ};
use serde::{Deserialize, Serialize};

use crate::audition::DEFAULT_AUDITION_VOLUME;

pub use lindelion_capture::{
    CaptureSettings, CaptureState, ScratchpadAudio, ScratchpadMetadata, SyncMode,
};

pub const MIN_ANALYSIS_CONFIDENCE_THRESHOLD: f32 = 0.0;
pub const MAX_ANALYSIS_CONFIDENCE_THRESHOLD: f32 = 1.0;
pub const DEFAULT_ANALYSIS_CONFIDENCE_THRESHOLD: f32 = 0.5;
pub const MIN_ONSET_SENSITIVITY: f32 = 0.0;
pub const MAX_ONSET_SENSITIVITY: f32 = 1.0;
pub const DEFAULT_ONSET_SENSITIVITY: f32 = 0.5;
pub const MIN_ANALYSIS_NOTE_MS: f32 = 30.0;
pub const MAX_ANALYSIS_NOTE_MS: f32 = 300.0;
pub const DEFAULT_ANALYSIS_NOTE_MS: f32 = 80.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlirdirPatch {
    pub name: String,
    pub capture: CaptureSettings,
    pub analysis: AnalysisSettings,
    pub quantize: QuantizeSettings,
    pub audition: AuditionSettings,
    pub scratchpad: Option<ScratchpadAudio>,
}

impl Default for GlirdirPatch {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            capture: CaptureSettings::default(),
            analysis: AnalysisSettings::default(),
            quantize: QuantizeSettings {
                root: RootNote::C,
                scale: Scale::Chromatic,
                snap_mode: SnapMode::Hard,
                grid: TimingGrid::Sixteenth,
                ..QuantizeSettings::default()
            },
            audition: AuditionSettings::default(),
            scratchpad: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSettings {
    pub confidence_threshold: f32,
    pub onset_sensitivity: f32,
    pub min_note_ms: f32,
    #[serde(default)]
    pub note_segmentation: NoteSegmentationConfig,
    #[serde(default)]
    pub onset_profile: OnsetDetectionProfile,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            confidence_threshold: DEFAULT_ANALYSIS_CONFIDENCE_THRESHOLD,
            onset_sensitivity: DEFAULT_ONSET_SENSITIVITY,
            min_note_ms: DEFAULT_ANALYSIS_NOTE_MS,
            note_segmentation: NoteSegmentationConfig::default(),
            onset_profile: OnsetDetectionProfile::default(),
        }
    }
}

impl AnalysisSettings {
    pub fn sanitized(self) -> Self {
        let confidence_threshold = finite_clamp(
            self.confidence_threshold,
            MIN_ANALYSIS_CONFIDENCE_THRESHOLD,
            MAX_ANALYSIS_CONFIDENCE_THRESHOLD,
            DEFAULT_ANALYSIS_CONFIDENCE_THRESHOLD,
        );
        let onset_sensitivity = finite_clamp(
            self.onset_sensitivity,
            MIN_ONSET_SENSITIVITY,
            MAX_ONSET_SENSITIVITY,
            DEFAULT_ONSET_SENSITIVITY,
        );
        let min_note_ms = finite_clamp(
            self.min_note_ms,
            MIN_ANALYSIS_NOTE_MS,
            MAX_ANALYSIS_NOTE_MS,
            DEFAULT_ANALYSIS_NOTE_MS,
        );
        let mut note_segmentation = self.note_segmentation.sanitized();
        note_segmentation.min_note_ms = min_note_ms;
        Self {
            confidence_threshold,
            onset_sensitivity,
            min_note_ms,
            note_segmentation,
            onset_profile: self.onset_profile.sanitized(),
        }
    }

    pub fn pitch_detection_config(self) -> PitchDetectionConfig {
        PitchDetectionConfig {
            confidence_threshold: self.sanitized().confidence_threshold,
            fmin_hz: SWIFTF0_MODEL_FMIN_HZ,
            fmax_hz: SWIFTF0_MODEL_FMAX_HZ,
        }
        .sanitized()
    }

    pub fn note_segmentation_config(self) -> NoteSegmentationConfig {
        self.sanitized().note_segmentation
    }

    pub fn phrase_analysis_config(self) -> PhraseAnalysisConfig {
        let value = self.sanitized();
        PhraseAnalysisConfig {
            pitch_detection: value.pitch_detection_config(),
            onset_detection: DetectionConfig::from(value),
            note_segmentation: value.note_segmentation,
        }
        .sanitized()
    }
}

impl From<AnalysisSettings> for DetectionConfig {
    fn from(value: AnalysisSettings) -> Self {
        let value = value.sanitized();
        DetectionConfig::superflux(
            value.onset_sensitivity,
            value.min_note_ms,
            value.onset_profile,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AuditionSettings {
    pub volume: f32,
    pub loop_enabled: bool,
    pub live_edit: bool,
}

impl Default for AuditionSettings {
    fn default() -> Self {
        Self {
            volume: DEFAULT_AUDITION_VOLUME,
            loop_enabled: true,
            live_edit: true,
        }
    }
}

impl AuditionSettings {
    pub fn sanitized(self) -> Self {
        Self {
            volume: finite_clamp(self.volume, 0.0, 1.0, DEFAULT_AUDITION_VOLUME),
            loop_enabled: self.loop_enabled,
            live_edit: self.live_edit,
        }
    }
}

pub fn apply_scratchpad_midi_context(
    scratchpad: &ScratchpadAudio,
    settings: &mut QuantizeSettings,
) {
    settings.sample_rate = scratchpad.sample_rate;
    settings.bpm = f32::from(scratchpad.metadata.bpm);
    settings.time_signature_numerator = scratchpad.metadata.time_signature_numerator;
    settings.time_signature_denominator = scratchpad.metadata.time_signature_denominator;
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_onset_detect::AlgorithmParams;

    #[test]
    fn analysis_settings_project_explicit_profiles_to_reusable_configs() {
        let onset_profile = OnsetDetectionProfile {
            lookback_frames: 7,
            max_filter_radius: 5,
            pitch_stability_threshold_cents: 90.0,
            pitch_stability_duration_ms: 48.0,
        };
        let settings = AnalysisSettings {
            confidence_threshold: 0.7,
            onset_sensitivity: 0.6,
            min_note_ms: 72.0,
            note_segmentation: NoteSegmentationConfig {
                min_note_ms: 999.0,
                min_inherited_pitch_rms: 0.07,
                same_pitch_merge_cents: 22.0,
                articulation_search_ms: 33.0,
                articulation_gap_ratio: 0.4,
                rms_chunk_samples: 128,
            },
            onset_profile,
        };

        let segmentation = settings.note_segmentation_config();
        assert_eq!(segmentation.min_note_ms, 72.0);
        assert_eq!(segmentation.min_inherited_pitch_rms, 0.07);
        assert_eq!(segmentation.same_pitch_merge_cents, 22.0);

        let detection = DetectionConfig::from(settings);
        assert_eq!(detection.profile, onset_profile);
        assert_eq!(
            detection.params,
            AlgorithmParams::SuperFlux {
                lookback_frames: 7,
                max_filter_radius: 5
            }
        );
    }

    #[test]
    fn legacy_analysis_settings_deserialize_with_default_profiles() {
        let settings: AnalysisSettings = toml::from_str(
            r#"
            confidence_threshold = 0.61
            onset_sensitivity = 0.55
            min_note_ms = 70.0
            "#,
        )
        .unwrap();

        assert_eq!(
            settings.note_segmentation,
            NoteSegmentationConfig::default()
        );
        assert_eq!(settings.onset_profile, OnsetDetectionProfile::default());
        assert_eq!(settings.sanitized().note_segmentation.min_note_ms, 70.0);
    }
}
