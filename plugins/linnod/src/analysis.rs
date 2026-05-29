use std::{error::Error, fmt};

use lindelion_onset_detect::{
    ConfiguredOnsetDetector, DetectionConfig, MarkerReconcileOutcome, MarkerReconcilePolicy,
    OnsetDetectionInput, OnsetDetector, SliceMarker, normalize_markers, reconcile_markers,
    select_strongest_markers,
};
use lindelion_pitch_detect::{
    PitchContour, PitchDetectionConfig, PitchDetectionError, PitchDetector, SwiftF0Detector,
};
use lindelion_pitch_shift::{
    PitchShiftAnalysisError, PitchShiftAnalyzer, PitchShiftEngine, PitchShiftRatios,
    PitchShiftRenderError, PitchShiftSliceRenderRequest, PitchShiftSliceSummary,
    PitchShiftSourceCache,
};
use lindelion_sample_library::{OwnedMonoAudioBuffer, RuntimeMonoAudioBuffer, SampleMetadata};

use crate::patch::SLICE_COUNT;

pub use lindelion_pitch_shift::PitchShiftSliceSummary as SlicePitchSummary;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceAnalysis {
    pub source: SampleMetadata,
    pub audio: RuntimeMonoAudioBuffer,
    pub pitch_contour: PitchContour,
    pub markers: Vec<SliceMarker>,
    pub pitch_shift_cache: PitchShiftSourceCache,
}

impl SourceAnalysis {
    pub fn slice_pitch_summaries(&self) -> &[PitchShiftSliceSummary] {
        &self.pitch_shift_cache.slice_summaries
    }

    pub fn render_shifted_slice_to(
        &self,
        slice_index: usize,
        ratios: PitchShiftRatios,
        output: &mut [f32],
    ) -> Result<(), PitchShiftRenderError> {
        PitchShiftEngine.render_slice_to(
            self.audio.samples(),
            &self.pitch_shift_cache,
            PitchShiftSliceRenderRequest::new(slice_index, ratios),
            output,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceAnalysisError {
    EmptySource,
    Pitch(PitchDetectionError),
    PitchShift(PitchShiftAnalysisError),
}

impl fmt::Display for SourceAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySource => write!(formatter, "source audio is empty"),
            Self::Pitch(error) => write!(formatter, "{error}"),
            Self::PitchShift(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for SourceAnalysisError {}

impl From<PitchDetectionError> for SourceAnalysisError {
    fn from(value: PitchDetectionError) -> Self {
        Self::Pitch(value)
    }
}

impl From<PitchShiftAnalysisError> for SourceAnalysisError {
    fn from(value: PitchShiftAnalysisError) -> Self {
        Self::PitchShift(value)
    }
}

#[derive(Debug, Clone)]
pub struct LinnodSourceAnalyzer<D = SwiftF0Detector, O = ConfiguredOnsetDetector> {
    pitch_detector: D,
    onset_detector: O,
}

impl Default for LinnodSourceAnalyzer<SwiftF0Detector, ConfiguredOnsetDetector> {
    fn default() -> Self {
        Self {
            pitch_detector: SwiftF0Detector::default(),
            onset_detector: ConfiguredOnsetDetector,
        }
    }
}

impl<D, O> LinnodSourceAnalyzer<D, O> {
    #[cfg(test)]
    pub fn new(pitch_detector: D, onset_detector: O) -> Self {
        Self {
            pitch_detector,
            onset_detector,
        }
    }
}

impl<D, O> LinnodSourceAnalyzer<D, O>
where
    D: PitchDetector,
    O: OnsetDetector,
{
    pub fn analyze(
        &self,
        source: SampleMetadata,
        audio: OwnedMonoAudioBuffer,
        detection: DetectionConfig,
        patch_markers: &[SliceMarker],
    ) -> Result<SourceAnalysis, SourceAnalysisError> {
        self.analyze_with_marker_policy(
            source,
            audio,
            detection,
            patch_markers,
            MarkerAnalysisPolicy::DetectAndMergeUserMarkers,
        )
    }

    pub fn analyze_with_saved_markers(
        &self,
        source: SampleMetadata,
        audio: OwnedMonoAudioBuffer,
        detection: DetectionConfig,
        patch_markers: &[SliceMarker],
    ) -> Result<SourceAnalysis, SourceAnalysisError> {
        self.analyze_with_marker_policy(
            source,
            audio,
            detection,
            patch_markers,
            MarkerAnalysisPolicy::UseSavedMarkers,
        )
    }

    fn analyze_with_marker_policy(
        &self,
        source: SampleMetadata,
        audio: OwnedMonoAudioBuffer,
        detection: DetectionConfig,
        patch_markers: &[SliceMarker],
        marker_policy: MarkerAnalysisPolicy,
    ) -> Result<SourceAnalysis, SourceAnalysisError> {
        if audio.samples.is_empty() {
            return Err(SourceAnalysisError::EmptySource);
        }

        let pitch_contour = self.pitch_detector.detect_with_config(
            &audio.samples,
            audio.sample_rate,
            PitchDetectionConfig::default(),
        )?;
        let min_gap_samples =
            lindelion_dsp_utils::math::ms_to_samples(detection.min_slice_ms, audio.sample_rate);
        let markers = match marker_policy {
            MarkerAnalysisPolicy::DetectAndMergeUserMarkers => {
                self.detect_and_merge_markers(MarkerDetectionInput {
                    audio: &audio.samples,
                    sample_rate: audio.sample_rate,
                    detection,
                    pitch_contour: &pitch_contour,
                    patch_markers,
                    min_gap_samples,
                })
            }
            MarkerAnalysisPolicy::UseSavedMarkers => {
                saved_markers_for_analysis(patch_markers, audio.samples.len(), min_gap_samples)
                    .unwrap_or_else(|| {
                        self.detect_and_merge_markers(MarkerDetectionInput {
                            audio: &audio.samples,
                            sample_rate: audio.sample_rate,
                            detection,
                            pitch_contour: &pitch_contour,
                            patch_markers,
                            min_gap_samples,
                        })
                    })
            }
        };
        let pitch_shift_cache = PitchShiftAnalyzer::default().analyze(
            &audio.samples,
            audio.sample_rate,
            &pitch_contour,
            &markers,
        )?;

        Ok(SourceAnalysis {
            source,
            audio: RuntimeMonoAudioBuffer::from_owned(audio),
            pitch_contour,
            markers,
            pitch_shift_cache,
        })
    }

    fn detect_and_merge_markers(&self, input: MarkerDetectionInput<'_>) -> Vec<SliceMarker> {
        let onset_input = OnsetDetectionInput::new(input.audio, input.sample_rate)
            .with_pitch_contour(input.pitch_contour);
        let auto_markers = select_strongest_markers(
            self.onset_detector.detect(onset_input, input.detection),
            input.audio,
            SLICE_COUNT,
            input.min_gap_samples,
        );
        let MarkerReconcileOutcome::Applied(markers) = reconcile_markers(
            auto_markers,
            input.patch_markers,
            MarkerReconcilePolicy::MergeUserMarkers,
            input.min_gap_samples,
            input.audio.len(),
        ) else {
            unreachable!("merge policy always applies")
        };
        select_strongest_markers(markers, input.audio, SLICE_COUNT, input.min_gap_samples)
    }
}

struct MarkerDetectionInput<'a> {
    audio: &'a [f32],
    sample_rate: u32,
    detection: DetectionConfig,
    pitch_contour: &'a PitchContour,
    patch_markers: &'a [SliceMarker],
    min_gap_samples: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerAnalysisPolicy {
    DetectAndMergeUserMarkers,
    UseSavedMarkers,
}

fn saved_markers_for_analysis(
    markers: &[SliceMarker],
    source_len: usize,
    min_gap_samples: usize,
) -> Option<Vec<SliceMarker>> {
    if markers.is_empty() {
        return None;
    }
    let mut markers = normalize_markers(markers.iter().copied(), min_gap_samples, source_len);
    markers.truncate(SLICE_COUNT);
    Some(markers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_onset_detect::{DetectionAlgorithm, MarkerKind, OnsetDetectionInput};
    use lindelion_pitch_detect::PitchFrame;

    #[test]
    fn source_analyzer_builds_markers_pitch_summary_and_runtime_audio() {
        let analyzer = LinnodSourceAnalyzer::new(
            FixedPitchDetector,
            FixedOnsetDetector {
                markers: vec![
                    SliceMarker {
                        position_samples: 0,
                        kind: MarkerKind::Auto,
                    },
                    SliceMarker {
                        position_samples: 2_400,
                        kind: MarkerKind::Auto,
                    },
                ],
            },
        );
        let audio = OwnedMonoAudioBuffer::new(vec![0.2; 4_800], 48_000);

        let result = analyzer
            .analyze(
                metadata(),
                audio,
                DetectionConfig {
                    algorithm: DetectionAlgorithm::EnergyTransient,
                    min_slice_ms: 10.0,
                    ..DetectionConfig::default()
                },
                &[],
            )
            .unwrap();

        assert_eq!(result.audio.samples().len(), 4_800);
        assert_eq!(result.markers.len(), 2);
        assert_eq!(
            result.slice_pitch_summaries()[0].detected_f0_hz,
            Some(220.0)
        );
        assert_eq!(
            result.slice_pitch_summaries()[1].detected_f0_hz,
            Some(440.0)
        );
        assert_eq!(result.pitch_shift_cache.source_len_samples, 4_800);
        let mut rendered = vec![0.0; 2_400];
        result
            .render_shifted_slice_to(
                0,
                PitchShiftRatios {
                    pitch_ratio: 1.5,
                    formant_ratio: None,
                },
                &mut rendered,
            )
            .unwrap();
        assert!(rendered.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn source_analyzer_merges_user_markers_during_redetection() {
        let analyzer = LinnodSourceAnalyzer::new(
            FixedPitchDetector,
            FixedOnsetDetector {
                markers: vec![
                    SliceMarker {
                        position_samples: 0,
                        kind: MarkerKind::Auto,
                    },
                    SliceMarker {
                        position_samples: 2_400,
                        kind: MarkerKind::Auto,
                    },
                ],
            },
        );
        let audio = OwnedMonoAudioBuffer::new(vec![0.2; 4_800], 48_000);
        let result = analyzer
            .analyze(
                metadata(),
                audio,
                DetectionConfig {
                    min_slice_ms: 10.0,
                    ..DetectionConfig::default()
                },
                &[SliceMarker {
                    position_samples: 2_450,
                    kind: MarkerKind::User,
                }],
            )
            .unwrap();

        assert_eq!(
            result.markers,
            vec![
                SliceMarker {
                    position_samples: 0,
                    kind: MarkerKind::Auto,
                },
                SliceMarker {
                    position_samples: 2_450,
                    kind: MarkerKind::User,
                },
            ]
        );
    }

    #[test]
    fn source_analyzer_reuses_saved_auto_markers_without_onset_detection() {
        let analyzer = LinnodSourceAnalyzer::new(FixedPitchDetector, PanicOnsetDetector);
        let audio = OwnedMonoAudioBuffer::new(vec![0.2; 4_800], 48_000);
        let saved_markers = vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 1_200,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 2_400,
                kind: MarkerKind::User,
            },
        ];

        let result = analyzer
            .analyze_with_saved_markers(
                metadata(),
                audio,
                DetectionConfig {
                    min_slice_ms: 10.0,
                    ..DetectionConfig::default()
                },
                &saved_markers,
            )
            .unwrap();

        assert_eq!(result.markers, saved_markers);
        assert_eq!(result.slice_pitch_summaries().len(), saved_markers.len());
    }

    #[test]
    fn source_analyzer_limits_markers_to_playable_slice_count_by_salience() {
        let mut markers = Vec::new();
        let mut audio = vec![0.0; 48_000];
        for index in 0..32 {
            let position = index * 1_200;
            markers.push(SliceMarker {
                position_samples: position,
                kind: MarkerKind::Auto,
            });
            audio[position] = if index >= 16 { 1.0 } else { 0.1 };
        }
        let analyzer =
            LinnodSourceAnalyzer::new(FixedPitchDetector, FixedOnsetDetector { markers });

        let result = analyzer
            .analyze(
                metadata(),
                OwnedMonoAudioBuffer::new(audio, 48_000),
                DetectionConfig {
                    min_slice_ms: 10.0,
                    ..DetectionConfig::default()
                },
                &[],
            )
            .unwrap();

        assert_eq!(result.markers.len(), crate::patch::SLICE_COUNT);
        assert_eq!(result.markers[0].position_samples, 0);
        assert!(
            result
                .markers
                .iter()
                .skip(1)
                .all(|marker| marker.position_samples >= 16 * 1_200)
        );
    }

    #[derive(Debug, Clone)]
    struct FixedPitchDetector;

    impl PitchDetector for FixedPitchDetector {
        fn detect(
            &self,
            audio: &[f32],
            sample_rate: u32,
        ) -> Result<PitchContour, PitchDetectionError> {
            Ok(PitchContour {
                source_sample_rate: sample_rate,
                analysis_sample_rate: sample_rate,
                hop_size: 1_200,
                frames: vec![
                    pitch_frame(0, 0, Some(220.0)),
                    pitch_frame(1, audio.len() / 4, Some(220.0)),
                    pitch_frame(2, audio.len() / 2, Some(440.0)),
                    pitch_frame(3, audio.len() * 3 / 4, Some(440.0)),
                ],
            })
        }
    }

    #[derive(Debug, Clone)]
    struct FixedOnsetDetector {
        markers: Vec<SliceMarker>,
    }

    impl OnsetDetector for FixedOnsetDetector {
        fn detect(
            &self,
            input: OnsetDetectionInput<'_>,
            _config: DetectionConfig,
        ) -> Vec<SliceMarker> {
            assert!(input.pitch_track.is_some());
            self.markers.clone()
        }
    }

    #[derive(Debug, Clone)]
    struct PanicOnsetDetector;

    impl OnsetDetector for PanicOnsetDetector {
        fn detect(
            &self,
            _input: OnsetDetectionInput<'_>,
            _config: DetectionConfig,
        ) -> Vec<SliceMarker> {
            panic!("saved marker restore must not run onset detection");
        }
    }

    fn metadata() -> SampleMetadata {
        SampleMetadata {
            reference: lindelion_sample_library::SampleReference::new("hash", "source.wav"),
            filename: "source.wav".to_string(),
            duration_ms: 100,
            sample_rate: 48_000,
            channels: 1,
            rms_db: None,
            peak_db: None,
            waveform_preview: lindelion_sample_library::SampleWaveformPreview {
                points: Vec::new(),
            },
        }
    }

    fn pitch_frame(
        frame_index: usize,
        source_sample_position: usize,
        f0_hz: Option<f32>,
    ) -> PitchFrame {
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds: source_sample_position as f32 / 48_000.0,
            f0_hz,
            raw_f0_hz: f0_hz.unwrap_or(0.0),
            confidence: 0.9,
            voiced: f0_hz.is_some(),
            rms: 0.2,
        }
    }
}
