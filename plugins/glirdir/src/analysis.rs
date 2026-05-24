use std::{error::Error, fmt};

use lindelion_midi::{DetectedNote, MidiClip, QuantizeSettings, clip_from_detected_notes};
use lindelion_onset_detect::{HybridOnsetDetector, SliceMarker};
#[cfg(test)]
use lindelion_onset_detect::{OnsetDetectionInput, OnsetDetector};
#[cfg(test)]
use lindelion_phrase_analysis::analyze_with_pitch_contour as analyze_phrase_with_pitch_contour;
use lindelion_phrase_analysis::{PhraseAnalysisError, PhraseAnalysisResult, PhraseAnalyzer};
use lindelion_pitch_detect::{PitchContour, PitchDetectionError, PitchDetector, SwiftF0Detector};
use serde::{Deserialize, Serialize};

use crate::patch::{AnalysisSettings, ScratchpadAudio, apply_scratchpad_midi_context};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub pitch_contour: PitchContour,
    pub markers: Vec<SliceMarker>,
    pub detected_notes: Vec<DetectedNote>,
    pub midi_clip: MidiClip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    EmptyScratchpad,
    Pitch(PitchDetectionError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyScratchpad => write!(formatter, "scratchpad audio is empty"),
            Self::Pitch(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for AnalysisError {}

impl From<PitchDetectionError> for AnalysisError {
    fn from(value: PitchDetectionError) -> Self {
        Self::Pitch(value)
    }
}

impl From<PhraseAnalysisError> for AnalysisError {
    fn from(value: PhraseAnalysisError) -> Self {
        match value {
            PhraseAnalysisError::EmptyAudio => Self::EmptyScratchpad,
            PhraseAnalysisError::Pitch(error) => Self::Pitch(error),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GlirdirAnalyzer<D = SwiftF0Detector> {
    phrase_analyzer: PhraseAnalyzer<D, HybridOnsetDetector>,
}

impl Default for GlirdirAnalyzer<SwiftF0Detector> {
    fn default() -> Self {
        Self {
            phrase_analyzer: PhraseAnalyzer::default(),
        }
    }
}

impl<D> GlirdirAnalyzer<D> {
    #[cfg(test)]
    pub fn new(pitch_detector: D) -> Self {
        Self {
            phrase_analyzer: PhraseAnalyzer::new(pitch_detector, HybridOnsetDetector),
        }
    }
}

impl<D> GlirdirAnalyzer<D>
where
    D: PitchDetector,
{
    pub fn analyze(
        &self,
        scratchpad: &ScratchpadAudio,
        analysis_settings: AnalysisSettings,
        quantize_settings: &QuantizeSettings,
    ) -> Result<AnalysisResult, AnalysisError> {
        if scratchpad.samples.is_empty() {
            return Err(AnalysisError::EmptyScratchpad);
        }

        let phrase_result = self.phrase_analyzer.analyze(
            &scratchpad.samples,
            scratchpad.sample_rate,
            analysis_settings.phrase_analysis_config(),
        )?;
        Ok(result_from_phrase_analysis(
            scratchpad,
            quantize_settings,
            phrase_result,
        ))
    }
}

#[cfg(test)]
pub(crate) fn analyze_with_pitch_contour(
    scratchpad: &ScratchpadAudio,
    analysis_settings: AnalysisSettings,
    quantize_settings: &QuantizeSettings,
    pitch_contour: PitchContour,
) -> AnalysisResult {
    let phrase_result = analyze_phrase_with_pitch_contour(
        &scratchpad.samples,
        scratchpad.sample_rate,
        analysis_settings.phrase_analysis_config(),
        pitch_contour,
        &HybridOnsetDetector,
    );
    result_from_phrase_analysis(scratchpad, quantize_settings, phrase_result)
}

fn result_from_phrase_analysis(
    scratchpad: &ScratchpadAudio,
    quantize_settings: &QuantizeSettings,
    phrase_result: PhraseAnalysisResult,
) -> AnalysisResult {
    let mut quantize_settings = quantize_settings.clone();
    apply_scratchpad_midi_context(scratchpad, &mut quantize_settings);
    let midi_clip = clip_from_detected_notes(&phrase_result.detected_notes, &quantize_settings);

    AnalysisResult {
        pitch_contour: phrase_result.pitch_contour,
        markers: phrase_result.markers,
        detected_notes: phrase_result.detected_notes,
        midi_clip,
    }
}

pub(crate) fn requantize_result(result: &mut AnalysisResult, quantize_settings: &QuantizeSettings) {
    result.midi_clip = clip_from_detected_notes(&result.detected_notes, quantize_settings);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_pitch_detect::{
        PitchDetectionConfig, PitchDetectionError, PitchFrame, SWIFTF0_MODEL_FMAX_HZ,
        SWIFTF0_MODEL_FMIN_HZ,
    };
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn segmentation_preserves_previous_pitch_through_low_confidence_region() {
        let audio = vec![0.2; 4_800];
        let contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0), 0.95),
                pitch_frame(1, 1_200, Some(440.0), 0.95),
                pitch_frame(2, 2_400, None, 0.1),
                pitch_frame(3, 3_600, None, 0.1),
            ],
        };
        let markers = vec![
            SliceMarker {
                position_samples: 0,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 2_400,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
        ];

        let phrase_result = lindelion_phrase_analysis::analyze_with_pitch_contour(
            &audio,
            48_000,
            AnalysisSettings {
                min_note_ms: 10.0,
                ..AnalysisSettings::default()
            }
            .phrase_analysis_config(),
            contour,
            &FixedOnsetDetector { markers },
        );

        assert_eq!(phrase_result.detected_notes.len(), 2);
        assert_eq!(phrase_result.detected_notes[0].pitch_hz, 440.0);
        assert_eq!(phrase_result.detected_notes[1].pitch_hz, 440.0);
    }

    #[test]
    fn analysis_reuses_shared_midi_quantizer() {
        let scratchpad = ScratchpadAudio::new(48_000, vec![0.2; 4_800]);
        let contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0), 0.95),
                pitch_frame(1, 1_200, Some(440.0), 0.95),
                pitch_frame(2, 2_400, Some(493.88), 0.95),
                pitch_frame(3, 3_600, Some(493.88), 0.95),
            ],
        };

        let result = analyze_with_pitch_contour(
            &scratchpad,
            AnalysisSettings {
                min_note_ms: 10.0,
                ..AnalysisSettings::default()
            },
            &QuantizeSettings::default(),
            contour,
        );

        assert!(!result.detected_notes.is_empty());
        assert_eq!(result.midi_clip.ppq, lindelion_midi::DEFAULT_PPQ);
    }

    #[test]
    fn analyzer_passes_analysis_settings_into_pitch_detection_config() {
        let configs = Rc::new(RefCell::new(Vec::new()));
        let analyzer = GlirdirAnalyzer::new(RecordingPitchDetector {
            configs: Rc::clone(&configs),
        });
        let scratchpad = ScratchpadAudio::new(48_000, vec![0.2; 4_800]);

        analyzer
            .analyze(
                &scratchpad,
                AnalysisSettings {
                    confidence_threshold: 0.82,
                    onset_sensitivity: 0.25,
                    min_note_ms: 40.0,
                    ..AnalysisSettings::default()
                },
                &QuantizeSettings::default(),
            )
            .unwrap();

        let configs = configs.borrow();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].confidence_threshold, 0.82);
        assert_eq!(configs[0].fmin_hz, SWIFTF0_MODEL_FMIN_HZ);
        assert_eq!(configs[0].fmax_hz, SWIFTF0_MODEL_FMAX_HZ);
    }

    #[derive(Debug, Clone)]
    struct RecordingPitchDetector {
        configs: Rc<RefCell<Vec<PitchDetectionConfig>>>,
    }

    struct FixedOnsetDetector {
        markers: Vec<SliceMarker>,
    }

    impl OnsetDetector for FixedOnsetDetector {
        fn detect(
            &self,
            input: OnsetDetectionInput<'_>,
            _config: lindelion_onset_detect::DetectionConfig,
        ) -> Vec<SliceMarker> {
            assert!(input.pitch_track.is_some());
            self.markers.clone()
        }
    }

    impl PitchDetector for RecordingPitchDetector {
        fn detect(
            &self,
            audio: &[f32],
            sample_rate: u32,
        ) -> Result<PitchContour, PitchDetectionError> {
            self.detect_with_config(audio, sample_rate, PitchDetectionConfig::default())
        }

        fn detect_with_config(
            &self,
            audio: &[f32],
            sample_rate: u32,
            config: PitchDetectionConfig,
        ) -> Result<PitchContour, PitchDetectionError> {
            self.configs.borrow_mut().push(config);
            Ok(PitchContour {
                source_sample_rate: sample_rate,
                analysis_sample_rate: sample_rate,
                hop_size: 256,
                frames: vec![
                    pitch_frame(0, 0, Some(440.0), 0.95),
                    pitch_frame(1, audio.len().saturating_div(2), Some(440.0), 0.95),
                ],
            })
        }
    }

    fn pitch_frame(
        frame_index: usize,
        source_sample_position: usize,
        f0_hz: Option<f32>,
        confidence: f32,
    ) -> PitchFrame {
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds: source_sample_position as f32 / 48_000.0,
            f0_hz,
            raw_f0_hz: f0_hz.unwrap_or(0.0),
            confidence,
            voiced: f0_hz.is_some(),
            rms: 0.2,
        }
    }
}
