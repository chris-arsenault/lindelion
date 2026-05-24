use super::*;
use lindelion_onset_detect::{DetectionConfig, MarkerKind};
use std::{cell::RefCell, rc::Rc};

const SAMPLE_RATE: u32 = 48_000;

#[test]
fn phrase_analyzer_runs_pitch_onset_and_segmentation_pipeline() {
    let pitch_configs = Rc::new(RefCell::new(Vec::new()));
    let onset_configs = Rc::new(RefCell::new(Vec::new()));
    let analyzer = PhraseAnalyzer::new(
        RecordingPitchDetector {
            configs: Rc::clone(&pitch_configs),
        },
        FixedOnsetDetector {
            markers: vec![marker(0), marker(2_400)],
            configs: Rc::clone(&onset_configs),
        },
    );
    let config = PhraseAnalysisConfig {
        pitch_detection: PitchDetectionConfig {
            confidence_threshold: 0.82,
            ..PitchDetectionConfig::default()
        },
        onset_detection: DetectionConfig::superflux(
            0.67,
            10.0,
            lindelion_onset_detect::OnsetDetectionProfile::aggressive(),
        ),
        note_segmentation: NoteSegmentationConfig {
            min_note_ms: 10.0,
            ..NoteSegmentationConfig::default()
        },
    };

    let result = analyzer
        .analyze(&vec![0.2; 4_800], SAMPLE_RATE, config)
        .unwrap();

    assert_eq!(pitch_configs.borrow()[0].confidence_threshold, 0.82);
    assert_eq!(onset_configs.borrow()[0], config.onset_detection);
    assert_eq!(result.markers.len(), 2);
    assert_eq!(result.segmented_notes.len(), 2);
    assert_eq!(result.detected_notes[0].pitch_hz, 440.0);
    assert_eq!(result.detected_notes[1].pitch_hz, 493.88);
}

#[test]
fn phrase_analyzer_rejects_empty_audio_before_pitch_detection() {
    let pitch_configs = Rc::new(RefCell::new(Vec::new()));
    let analyzer = PhraseAnalyzer::new(
        RecordingPitchDetector {
            configs: Rc::clone(&pitch_configs),
        },
        FixedOnsetDetector {
            markers: Vec::new(),
            configs: Rc::new(RefCell::new(Vec::new())),
        },
    );

    let error = analyzer
        .analyze(&[], SAMPLE_RATE, PhraseAnalysisConfig::default())
        .unwrap_err();

    assert_eq!(error, PhraseAnalysisError::EmptyAudio);
    assert!(pitch_configs.borrow().is_empty());
}

#[test]
fn marked_phrase_analysis_builds_complete_result_without_batch_detectors() {
    let audio = vec![0.2; 4_800];
    let markers = vec![marker(0), marker(2_400)];

    let result = analyze_with_markers(
        &audio,
        SAMPLE_RATE,
        NoteSegmentationConfig {
            min_note_ms: 10.0,
            ..NoteSegmentationConfig::default()
        },
        PitchContour {
            source_sample_rate: SAMPLE_RATE,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0), 0.95, 0.2),
                pitch_frame(1, 1_200, Some(440.0), 0.95, 0.2),
                pitch_frame(2, 2_400, Some(493.88), 0.95, 0.2),
                pitch_frame(3, 3_600, Some(493.88), 0.95, 0.2),
            ],
        },
        markers.clone(),
    );

    assert_eq!(result.markers, markers);
    assert_eq!(result.segmented_notes.len(), 2);
    assert_eq!(result.detected_notes.len(), 2);
    assert_eq!(result.detected_notes[0].pitch_hz, 440.0);
    assert_eq!(result.detected_notes[1].pitch_hz, 493.88);
}

#[test]
fn segmentation_preserves_previous_pitch_through_low_confidence_region() {
    let audio = vec![0.2; 4_800];
    let contour = PitchContour {
        source_sample_rate: SAMPLE_RATE,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames: vec![
            pitch_frame(0, 0, Some(440.0), 0.95, 0.2),
            pitch_frame(1, 1_200, Some(440.0), 0.95, 0.2),
            pitch_frame(2, 2_400, None, 0.1, 0.2),
            pitch_frame(3, 3_600, None, 0.1, 0.2),
        ],
    };
    let markers = vec![marker(0), marker(2_400)];

    let notes = segment_notes(
        &audio,
        SAMPLE_RATE,
        &contour,
        &markers,
        NoteSegmentationConfig {
            min_note_ms: 10.0,
            ..NoteSegmentationConfig::default()
        },
    );

    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].note.pitch_hz, 440.0);
    assert!(!notes[0].inherited_pitch);
    assert_eq!(notes[1].note.pitch_hz, 440.0);
    assert!(notes[1].inherited_pitch);
}

#[test]
fn segmentation_rejects_quiet_inherited_pitch_regions() {
    let mut audio = vec![0.2; 2_400];
    audio.extend(vec![0.01; 2_400]);
    let contour = PitchContour {
        source_sample_rate: SAMPLE_RATE,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames: vec![
            pitch_frame(0, 0, Some(440.0), 0.95, 0.2),
            pitch_frame(1, 1_200, Some(440.0), 0.95, 0.2),
            pitch_frame(2, 2_400, None, 0.1, 0.01),
            pitch_frame(3, 3_600, None, 0.1, 0.01),
        ],
    };

    let notes = segment_notes(
        &audio,
        SAMPLE_RATE,
        &contour,
        &[marker(0), marker(2_400)],
        NoteSegmentationConfig {
            min_note_ms: 10.0,
            ..NoteSegmentationConfig::default()
        },
    );

    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].note.start_sample, 0);
    assert_eq!(notes[0].note.end_sample, 2_400);
}

#[test]
fn unarticulated_same_pitch_splits_merge() {
    let audio = vec![0.2; 9_600];
    let contour = contour_with_regions(&[(0, Some(440.0)), (4_800, Some(441.0))], 0.2);

    let notes = segment_notes(
        &audio,
        SAMPLE_RATE,
        &contour,
        &[marker(0), marker(4_800)],
        NoteSegmentationConfig {
            min_note_ms: 10.0,
            ..NoteSegmentationConfig::default()
        },
    );

    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].note.start_sample, 0);
    assert_eq!(notes[0].note.end_sample, audio.len());
    assert!(!notes[0].inherited_pitch);
}

#[test]
fn articulation_gap_prevents_same_pitch_merge() {
    let mut audio = vec![0.2; 4_800];
    audio.extend(vec![0.0; 2_400]);
    audio.extend(vec![0.2; 4_800]);
    let restart = 7_200;
    let contour = PitchContour {
        source_sample_rate: SAMPLE_RATE,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames: vec![
            pitch_frame(0, 0, Some(440.0), 0.95, 0.2),
            pitch_frame(1, 2_400, Some(440.0), 0.95, 0.2),
            pitch_frame(2, restart, Some(440.0), 0.95, 0.2),
            pitch_frame(3, restart + 2_400, Some(440.0), 0.95, 0.2),
        ],
    };

    let notes = segment_notes(
        &audio,
        SAMPLE_RATE,
        &contour,
        &[marker(0), marker(restart)],
        NoteSegmentationConfig {
            min_note_ms: 10.0,
            ..NoteSegmentationConfig::default()
        },
    );

    assert_eq!(notes.len(), 2);
    assert_eq!(notes[1].note.start_sample, restart);
}

#[test]
fn merge_tolerance_is_configurable() {
    let audio = vec![0.2; 9_600];
    let contour = contour_with_regions(&[(0, Some(440.0)), (4_800, Some(452.0))], 0.2);
    let config = NoteSegmentationConfig {
        min_note_ms: 10.0,
        same_pitch_merge_cents: 50.0,
        ..NoteSegmentationConfig::default()
    };

    let merged = segment_notes(
        &audio,
        SAMPLE_RATE,
        &contour,
        &[marker(0), marker(4_800)],
        config,
    );
    let split = segment_notes(
        &audio,
        SAMPLE_RATE,
        &contour,
        &[marker(0), marker(4_800)],
        NoteSegmentationConfig {
            same_pitch_merge_cents: 20.0,
            ..config
        },
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(split.len(), 2);
}

#[test]
fn config_sanitization_preserves_safe_runtime_values() {
    let config = NoteSegmentationConfig {
        min_note_ms: f32::NAN,
        min_inherited_pitch_rms: f32::INFINITY,
        same_pitch_merge_cents: -10.0,
        articulation_search_ms: f32::NEG_INFINITY,
        articulation_gap_ratio: 10.0,
        rms_chunk_samples: 0,
    }
    .sanitized();

    assert_eq!(config.min_note_ms, DEFAULT_MIN_NOTE_MS);
    assert_eq!(
        config.min_inherited_pitch_rms,
        DEFAULT_MIN_INHERITED_PITCH_RMS
    );
    assert_eq!(config.same_pitch_merge_cents, 0.0);
    assert_eq!(
        config.articulation_search_ms,
        DEFAULT_ARTICULATION_SEARCH_MS
    );
    assert_eq!(config.articulation_gap_ratio, 1.0);
    assert_eq!(config.rms_chunk_samples, 1);
}

fn marker(position_samples: usize) -> SliceMarker {
    SliceMarker {
        position_samples,
        kind: MarkerKind::Auto,
    }
}

fn contour_with_regions(regions: &[(usize, Option<f32>)], rms: f32) -> PitchContour {
    let mut frames = Vec::new();
    for (index, (position, f0_hz)) in regions.iter().copied().enumerate() {
        frames.push(pitch_frame(index * 2, position, f0_hz, 0.95, rms));
        frames.push(pitch_frame(
            index * 2 + 1,
            position + 2_400,
            f0_hz,
            0.95,
            rms,
        ));
    }
    PitchContour {
        source_sample_rate: SAMPLE_RATE,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames,
    }
}

fn pitch_frame(
    frame_index: usize,
    source_sample_position: usize,
    f0_hz: Option<f32>,
    confidence: f32,
    rms: f32,
) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / SAMPLE_RATE as f32,
        f0_hz,
        raw_f0_hz: f0_hz.unwrap_or(0.0),
        confidence,
        voiced: f0_hz.is_some(),
        rms,
    }
}

#[derive(Debug, Clone)]
struct RecordingPitchDetector {
    configs: Rc<RefCell<Vec<PitchDetectionConfig>>>,
}

impl PitchDetector for RecordingPitchDetector {
    fn detect(&self, audio: &[f32], sample_rate: u32) -> Result<PitchContour, PitchDetectionError> {
        self.detect_with_config(audio, sample_rate, PitchDetectionConfig::default())
    }

    fn detect_with_config(
        &self,
        _audio: &[f32],
        sample_rate: u32,
        config: PitchDetectionConfig,
    ) -> Result<PitchContour, PitchDetectionError> {
        self.configs.borrow_mut().push(config);
        Ok(PitchContour {
            source_sample_rate: sample_rate,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0), 0.95, 0.2),
                pitch_frame(1, 1_200, Some(440.0), 0.95, 0.2),
                pitch_frame(2, 2_400, Some(493.88), 0.95, 0.2),
                pitch_frame(3, 3_600, Some(493.88), 0.95, 0.2),
            ],
        })
    }
}

#[derive(Debug, Clone)]
struct FixedOnsetDetector {
    markers: Vec<SliceMarker>,
    configs: Rc<RefCell<Vec<DetectionConfig>>>,
}

impl OnsetDetector for FixedOnsetDetector {
    fn detect(&self, input: OnsetDetectionInput<'_>, config: DetectionConfig) -> Vec<SliceMarker> {
        assert!(input.pitch_track.is_some());
        self.configs.borrow_mut().push(config);
        self.markers.clone()
    }
}
