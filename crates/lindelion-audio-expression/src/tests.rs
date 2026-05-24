use super::*;
use lindelion_midi::DetectedNote;
use lindelion_onset_detect::{MarkerKind, SliceMarker, StreamingOnsetDetector};
use lindelion_phrase_analysis::{PhraseAnalysisResult, SegmentedNote};
use lindelion_pitch_detect::{
    PitchContour, PitchDetectionError, PitchFrame, StreamingPitchTracker,
};
use lindelion_plugin_shell::ExpressionSource;

const SAMPLE_RATE: u32 = 48_000;

#[test]
fn source_maps_shared_phrase_result_to_voice_relative_pitch_bend() {
    let pitch_hz = 293.664_76;
    let audio = sine_wave(pitch_hz, 0.3, 2_048);
    let analysis = phrase_result(pitch_hz, 0.2, audio.len());
    let mut source = AudioAnalysisExpressionSource::<4>::new(
        &audio,
        SAMPLE_RATE,
        &analysis,
        AudioExpressionMapping {
            pitch_bend_range_semitones: 12.0,
            pressure_floor_rms: 0.0,
            pressure_ceiling_rms: 0.4,
            brightness_floor_hz: 100.0,
            brightness_ceiling_hz: 8_000.0,
        },
    );

    source.voice_started(0, 0, 60, 0.8);
    source.set_block(0, 1_024);
    let stream = source.next_block(0);

    assert!(stream.gate);
    assert!((stream.pitch_bend - 2.0).abs() < 0.05);
    assert_eq!(stream.velocity, 0.8);
    assert!(stream.pressure > 0.4);
    assert!(stream.brightness > 0.0);
}

#[test]
fn source_gate_follows_detected_note_windows() {
    let audio = sine_wave(440.0, 0.3, 4_096);
    let analysis = phrase_result(440.0, 0.2, 2_048);
    let mut source = AudioAnalysisExpressionSource::<1>::new(
        &audio,
        SAMPLE_RATE,
        &analysis,
        AudioExpressionMapping::default(),
    );
    source.voice_started(0, 0, 69, 1.0);

    source.set_block(0, 512);
    assert!(source.next_block(0).gate);

    source.set_block(3_000, 512);
    assert!(!source.next_block(0).gate);
}

#[test]
fn spectral_centroid_maps_to_brightness_parameter() {
    let analysis = phrase_result(440.0, 0.2, 2_048);
    let low_audio = sine_wave(500.0, 0.3, 2_048);
    let high_audio = sine_wave(4_000.0, 0.3, 2_048);
    let mapping = AudioExpressionMapping {
        brightness_floor_hz: 100.0,
        brightness_ceiling_hz: 8_000.0,
        ..AudioExpressionMapping::default()
    };
    let mut low_source =
        AudioAnalysisExpressionSource::<1>::new(&low_audio, SAMPLE_RATE, &analysis, mapping);
    let mut high_source =
        AudioAnalysisExpressionSource::<1>::new(&high_audio, SAMPLE_RATE, &analysis, mapping);

    low_source.set_block(0, 1_024);
    high_source.set_block(0, 1_024);

    assert!(high_source.current_frame().brightness > low_source.current_frame().brightness);
}

#[test]
fn generic_source_accepts_non_phrase_frame_provider() {
    #[derive(Debug, Clone, Copy)]
    struct FixedFrameProvider {
        frame: AudioExpressionFrame,
    }

    impl AudioExpressionFrameSource for FixedFrameProvider {
        fn current_frame(&self) -> AudioExpressionFrame {
            self.frame
        }

        fn set_block(
            &mut self,
            start_sample: usize,
            len_samples: usize,
            _mapping: AudioExpressionMapping,
        ) -> AudioExpressionFrame {
            self.frame.start_sample = start_sample;
            self.frame.end_sample = start_sample + len_samples;
            self.frame
        }
    }

    let mut source = AudioExpressionSource::<_, 1>::from_frame_source(
        FixedFrameProvider {
            frame: AudioExpressionFrame {
                pitch_hz: Some(493.883_3),
                pressure: 0.7,
                brightness: 0.4,
                gate: true,
                ..AudioExpressionFrame::default()
            },
        },
        AudioExpressionMapping {
            pitch_bend_range_semitones: 12.0,
            ..AudioExpressionMapping::default()
        },
    );

    source.voice_started(0, 0, 69, 0.5);
    source.set_block(128, 256);
    let stream = source.next_block(0);

    assert_eq!(source.current_frame().start_sample, 128);
    assert!((stream.pitch_bend - 2.0).abs() < 0.05);
    assert_eq!(stream.pressure, 0.7);
    assert_eq!(stream.brightness, 0.4);
    assert_eq!(stream.velocity, 0.5);
    assert!(stream.gate);
}

#[test]
fn streaming_source_maps_detector_blocks_to_expression_stream() {
    let pitch_hz = 493.883_3;
    let audio = sine_wave(pitch_hz, 0.3, 2_048);
    let mut source = AudioExpressionSource::<_, 1>::from_streaming_analysis(
        FixedPitchTracker::new(pitch_hz),
        FirstBlockOnsetDetector::default(),
        RmsCentroidLoudnessTracker::default(),
        SAMPLE_RATE,
        AudioExpressionMapping {
            pitch_bend_range_semitones: 12.0,
            pressure_floor_rms: 0.0,
            pressure_ceiling_rms: 0.4,
            brightness_floor_hz: 100.0,
            brightness_ceiling_hz: 8_000.0,
        },
    );

    source.voice_started(0, 0, 69, 0.5);
    source.set_audio_block(0, &audio[..1_024]);
    let stream = source.next_block(0);

    assert!(stream.gate);
    assert!((stream.pitch_bend - 2.0).abs() < 0.05);
    assert_eq!(stream.velocity, 0.5);
    assert!(stream.pressure > 0.4);
    assert!(stream.brightness > 0.0);
}

#[test]
fn shared_streaming_audio_analysis_expression_source_builds_default_trackers() {
    let audio = sine_wave(293.664_76, 0.5, 8_192);
    let mut source = streaming_audio_analysis_expression_source::<1>(
        SAMPLE_RATE,
        AudioExpressionMapping {
            pitch_bend_range_semitones: 12.0,
            pressure_floor_rms: 0.0,
            pressure_ceiling_rms: 0.5,
            brightness_floor_hz: 100.0,
            brightness_ceiling_hz: 8_000.0,
        },
    );

    source.voice_started(0, 0, 60, 0.7);
    source.set_audio_block(0, &audio);
    let stream = source.next_block(0);

    assert!(stream.gate);
    assert!((stream.pitch_bend - 2.0).abs() < 0.5);
    assert_eq!(stream.velocity, 0.7);
    assert!(stream.pressure > 0.4);
    assert!(stream.brightness > 0.0);
}

#[test]
fn audio_note_detection_config_sanitizes_and_projects_detector_configs() {
    let config = AudioNoteDetectionConfig {
        onset_sensitivity: f32::NAN,
        note_release_floor_rms: -1.0,
        minimum_note_length_ms: f32::INFINITY,
        pitch_confidence: 2.0,
        velocity_amount: -0.5,
    }
    .sanitized();

    assert_eq!(
        config.onset_sensitivity,
        DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY
    );
    assert_eq!(config.note_release_floor_rms, 0.0);
    assert_eq!(
        config.minimum_note_length_ms,
        DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS
    );
    assert_eq!(config.pitch_confidence, 1.0);
    assert_eq!(config.velocity_amount, 0.0);

    let onset = config.onset_detection_config();
    assert_eq!(onset.sensitivity, config.onset_sensitivity);
    assert_eq!(onset.min_slice_ms, config.minimum_note_length_ms);
    assert_eq!(
        config.pitch_detection_config().confidence_threshold,
        config.pitch_confidence
    );
}

#[test]
fn streaming_audio_note_detector_emits_note_on_from_shared_trackers() {
    let audio = sine_wave(440.0, 0.4, 1_024);
    let mut detector = StreamingAudioNoteDetector::new(
        FixedPitchTracker::new(440.0),
        FirstBlockOnsetDetector::default(),
        RmsCentroidLoudnessTracker::default(),
        SAMPLE_RATE,
    );

    let events = detector.next_block(&audio, AudioNoteDetectionConfig::default());

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].offset, 0);
    assert_eq!(events[0].note, 69);
    assert!(events[0].gate);
    assert!((events[0].pitch_hz - 440.0).abs() < 0.001);
    assert!(events[0].velocity > 0.5);
    assert!((events[0].confidence - 0.95).abs() < 0.001);
    assert_eq!(detector.active_note().unwrap().note, 69);
}

#[test]
fn streaming_audio_note_detector_emits_late_onset_from_framed_detector() {
    let audio = sine_wave(440.0, 0.4, 128);
    let mut detector = StreamingAudioNoteDetector::new(
        FixedPitchTracker::new(440.0),
        LateOnsetDetector::new(128, 4),
        RmsCentroidLoudnessTracker::default(),
        SAMPLE_RATE,
    );

    for _ in 0..4 {
        assert!(
            detector
                .next_block(&audio, AudioNoteDetectionConfig::default())
                .is_empty()
        );
    }
    let events = detector.next_block(&audio, AudioNoteDetectionConfig::default());

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].offset, 0);
    assert_eq!(events[0].note, 69);
    assert!(events[0].gate);
    assert_eq!(detector.active_note().unwrap().note, 69);
}

#[test]
fn streaming_audio_note_detector_emits_note_off_after_release_floor() {
    let audio = sine_wave(440.0, 0.4, 1_024);
    let silence = vec![0.0; 1_024];
    let config = AudioNoteDetectionConfig {
        note_release_floor_rms: 0.05,
        minimum_note_length_ms: 1.0,
        ..AudioNoteDetectionConfig::default()
    };
    let mut detector = StreamingAudioNoteDetector::new(
        FixedPitchTracker::new(440.0),
        FirstBlockOnsetDetector::default(),
        RmsCentroidLoudnessTracker::default(),
        SAMPLE_RATE,
    );

    assert!(detector.next_block(&audio, config)[0].gate);
    let events = detector.next_block(&silence, config);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].offset, 0);
    assert_eq!(events[0].note, 69);
    assert!(!events[0].gate);
    assert_eq!(detector.active_note(), None);
}

#[test]
fn streaming_audio_note_detector_requires_confident_pitch_for_note_on() {
    let audio = sine_wave(440.0, 0.4, 1_024);
    let config = AudioNoteDetectionConfig {
        pitch_confidence: 0.8,
        ..AudioNoteDetectionConfig::default()
    };
    let mut detector = StreamingAudioNoteDetector::new(
        FixedPitchTracker::with_confidence(440.0, 0.2),
        FirstBlockOnsetDetector::default(),
        RmsCentroidLoudnessTracker::default(),
        SAMPLE_RATE,
    );

    let events = detector.next_block(&audio, config);

    assert!(events.is_empty());
    assert_eq!(detector.active_note(), None);
}

fn phrase_result(pitch_hz: f32, rms: f32, len: usize) -> PhraseAnalysisResult {
    let note = DetectedNote {
        start_sample: 0,
        end_sample: len,
        pitch_hz,
        peak_rms: rms,
        mean_rms: rms,
    };
    PhraseAnalysisResult {
        pitch_contour: PitchContour {
            source_sample_rate: SAMPLE_RATE,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, pitch_hz, rms),
                pitch_frame(1, 768, pitch_hz, rms),
                pitch_frame(2, 1_536, pitch_hz, rms),
            ],
        },
        markers: Vec::new(),
        segmented_notes: vec![SegmentedNote {
            note,
            inherited_pitch: false,
        }],
        detected_notes: vec![note],
    }
}

fn pitch_frame(
    frame_index: usize,
    source_sample_position: usize,
    pitch_hz: f32,
    rms: f32,
) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / SAMPLE_RATE as f32,
        f0_hz: Some(pitch_hz),
        raw_f0_hz: pitch_hz,
        confidence: 0.95,
        voiced: true,
        rms,
    }
}

#[derive(Debug, Clone)]
struct FixedPitchTracker {
    pitch_hz: f32,
    confidence: f32,
    frames: Vec<PitchFrame>,
}

impl FixedPitchTracker {
    fn new(pitch_hz: f32) -> Self {
        Self::with_confidence(pitch_hz, 0.95)
    }

    fn with_confidence(pitch_hz: f32, confidence: f32) -> Self {
        Self {
            pitch_hz,
            confidence,
            frames: Vec::new(),
        }
    }
}

impl StreamingPitchTracker for FixedPitchTracker {
    fn next_block(&mut self, audio: &[f32]) -> Result<&[PitchFrame], PitchDetectionError> {
        self.frames.clear();
        if !audio.is_empty() {
            let mut frame = pitch_frame(0, 0, self.pitch_hz, 0.2);
            frame.confidence = self.confidence;
            frame.f0_hz = (self.confidence >= 0.5).then_some(self.pitch_hz);
            frame.voiced = frame.f0_hz.is_some();
            self.frames.push(frame);
        }
        Ok(&self.frames)
    }

    fn reset(&mut self) {
        self.frames.clear();
    }
}

#[derive(Debug, Clone, Default)]
struct FirstBlockOnsetDetector {
    emitted: bool,
    markers: Vec<SliceMarker>,
}

impl StreamingOnsetDetector for FirstBlockOnsetDetector {
    fn next_block(&mut self, audio: &[f32]) -> &[SliceMarker] {
        self.markers.clear();
        if !self.emitted && !audio.is_empty() {
            self.markers.push(SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            });
            self.emitted = true;
        }
        &self.markers
    }

    fn reset(&mut self) {
        self.emitted = false;
        self.markers.clear();
    }
}

#[derive(Debug, Clone)]
struct LateOnsetDetector {
    marker_position: usize,
    emit_on_call: usize,
    calls: usize,
    markers: Vec<SliceMarker>,
}

impl LateOnsetDetector {
    fn new(marker_position: usize, emit_on_call: usize) -> Self {
        Self {
            marker_position,
            emit_on_call,
            calls: 0,
            markers: Vec::new(),
        }
    }
}

impl StreamingOnsetDetector for LateOnsetDetector {
    fn next_block(&mut self, _audio: &[f32]) -> &[SliceMarker] {
        self.markers.clear();
        if self.calls == self.emit_on_call {
            self.markers.push(SliceMarker {
                position_samples: self.marker_position,
                kind: MarkerKind::Auto,
            });
        }
        self.calls += 1;
        &self.markers
    }

    fn reset(&mut self) {
        self.calls = 0;
        self.markers.clear();
    }
}

fn sine_wave(frequency_hz: f32, amplitude: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / SAMPLE_RATE as f32;
            phase.sin() * amplitude
        })
        .collect()
}
