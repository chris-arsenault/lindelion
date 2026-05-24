use lindelion_dsp_utils::{
    analysis::{rms, spectral_centroid_hz},
    math::{finite_clamp, hz_to_midi_note},
};
use lindelion_onset_detect::StreamingOnsetDetector;
use lindelion_phrase_analysis::PhraseAnalysisResult;
use lindelion_pitch_detect::{
    PitchDetectionError, PitchFrame, StreamingPitchTracker, median_voiced_pitch,
};
use lindelion_plugin_shell::{ExpressionSource, ExpressionStream};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PITCH_BEND_RANGE_SEMITONES: f32 = 2.0;
pub const DEFAULT_PRESSURE_FLOOR_RMS: f32 = 0.02;
pub const DEFAULT_PRESSURE_CEILING_RMS: f32 = 0.35;
pub const DEFAULT_BRIGHTNESS_FLOOR_HZ: f32 = 500.0;
pub const DEFAULT_BRIGHTNESS_CEILING_HZ: f32 = 6_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioExpressionMapping {
    pub pitch_bend_range_semitones: f32,
    pub pressure_floor_rms: f32,
    pub pressure_ceiling_rms: f32,
    pub brightness_floor_hz: f32,
    pub brightness_ceiling_hz: f32,
}

impl Default for AudioExpressionMapping {
    fn default() -> Self {
        Self {
            pitch_bend_range_semitones: DEFAULT_PITCH_BEND_RANGE_SEMITONES,
            pressure_floor_rms: DEFAULT_PRESSURE_FLOOR_RMS,
            pressure_ceiling_rms: DEFAULT_PRESSURE_CEILING_RMS,
            brightness_floor_hz: DEFAULT_BRIGHTNESS_FLOOR_HZ,
            brightness_ceiling_hz: DEFAULT_BRIGHTNESS_CEILING_HZ,
        }
    }
}

impl AudioExpressionMapping {
    pub fn sanitized(self) -> Self {
        Self {
            pitch_bend_range_semitones: finite_clamp(
                self.pitch_bend_range_semitones,
                0.0,
                48.0,
                DEFAULT_PITCH_BEND_RANGE_SEMITONES,
            ),
            pressure_floor_rms: finite_clamp(
                self.pressure_floor_rms,
                0.0,
                1.0,
                DEFAULT_PRESSURE_FLOOR_RMS,
            ),
            pressure_ceiling_rms: finite_clamp(
                self.pressure_ceiling_rms,
                0.0,
                1.0,
                DEFAULT_PRESSURE_CEILING_RMS,
            ),
            brightness_floor_hz: finite_clamp(
                self.brightness_floor_hz,
                0.0,
                48_000.0,
                DEFAULT_BRIGHTNESS_FLOOR_HZ,
            ),
            brightness_ceiling_hz: finite_clamp(
                self.brightness_ceiling_hz,
                0.0,
                48_000.0,
                DEFAULT_BRIGHTNESS_CEILING_HZ,
            ),
        }
    }

    pub fn frame_from_features(self, features: AudioExpressionFeatures) -> AudioExpressionFrame {
        let mapping = self.sanitized();
        AudioExpressionFrame {
            start_sample: features.start_sample,
            end_sample: features.end_sample,
            pitch_hz: features.pitch_hz,
            pressure: if features.gate {
                map_linear(
                    features.loudness_rms,
                    mapping.pressure_floor_rms,
                    mapping.pressure_ceiling_rms,
                )
            } else {
                0.0
            },
            brightness: if features.gate {
                map_linear(
                    features.spectral_centroid_hz,
                    mapping.brightness_floor_hz,
                    mapping.brightness_ceiling_hz,
                )
            } else {
                0.0
            },
            gate: features.gate,
        }
    }

    pub fn stream_for_midi_note(
        self,
        frame: AudioExpressionFrame,
        midi_note: u8,
        velocity: f32,
    ) -> ExpressionStream {
        if !frame.gate {
            return ExpressionStream::default();
        }

        let range = self.sanitized().pitch_bend_range_semitones;
        let pitch_bend = frame
            .pitch_hz
            .and_then(hz_to_midi_note)
            .map(|note| (note - midi_note as f32).clamp(-range, range))
            .unwrap_or(0.0);

        ExpressionStream {
            pitch_bend,
            pressure: frame.pressure,
            brightness: frame.brightness,
            velocity,
            gate: true,
        }
        .sanitized()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioExpressionFeatures {
    pub start_sample: usize,
    pub end_sample: usize,
    pub pitch_hz: Option<f32>,
    pub loudness_rms: f32,
    pub spectral_centroid_hz: f32,
    pub gate: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioExpressionFrame {
    pub start_sample: usize,
    pub end_sample: usize,
    pub pitch_hz: Option<f32>,
    pub pressure: f32,
    pub brightness: f32,
    pub gate: bool,
}

pub trait AudioExpressionFrameSource {
    fn current_frame(&self) -> AudioExpressionFrame;

    fn set_block(
        &mut self,
        start_sample: usize,
        len_samples: usize,
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame;
}

pub trait StreamingAudioExpressionFrameSource: AudioExpressionFrameSource {
    fn set_audio_block(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StreamingLoudnessFrame {
    pub start_sample: usize,
    pub end_sample: usize,
    pub rms: f32,
    pub spectral_centroid_hz: f32,
}

pub trait StreamingLoudnessTracker {
    fn next_block(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        sample_rate: u32,
    ) -> StreamingLoudnessFrame;

    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RmsCentroidLoudnessTracker {
    frame: StreamingLoudnessFrame,
}

impl RmsCentroidLoudnessTracker {
    pub const fn current_frame(&self) -> StreamingLoudnessFrame {
        self.frame
    }
}

impl StreamingLoudnessTracker for RmsCentroidLoudnessTracker {
    fn next_block(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        sample_rate: u32,
    ) -> StreamingLoudnessFrame {
        self.frame = StreamingLoudnessFrame {
            start_sample,
            end_sample: start_sample.saturating_add(audio.len()),
            rms: rms(audio),
            spectral_centroid_hz: spectral_centroid_hz(audio, sample_rate.max(1) as f32)
                .unwrap_or(0.0),
        };
        self.frame
    }

    fn reset(&mut self) {
        self.frame = StreamingLoudnessFrame::default();
    }
}

#[derive(Debug, Clone)]
pub struct AudioExpressionSource<Frames, const VOICES: usize> {
    frames: Frames,
    mapping: AudioExpressionMapping,
    voices: [AudioExpressionVoice; VOICES],
}

impl<Frames, const VOICES: usize> AudioExpressionSource<Frames, VOICES>
where
    Frames: AudioExpressionFrameSource,
{
    pub fn from_frame_source(frames: Frames, mapping: AudioExpressionMapping) -> Self {
        Self {
            frames,
            mapping: mapping.sanitized(),
            voices: [AudioExpressionVoice::default(); VOICES],
        }
    }

    pub fn frame_source(&self) -> &Frames {
        &self.frames
    }

    pub fn frame_source_mut(&mut self) -> &mut Frames {
        &mut self.frames
    }

    pub fn set_mapping(&mut self, mapping: AudioExpressionMapping) {
        self.mapping = mapping.sanitized();
    }

    pub const fn mapping(&self) -> AudioExpressionMapping {
        self.mapping
    }

    pub fn current_frame(&self) -> AudioExpressionFrame {
        self.frames.current_frame()
    }

    pub fn set_block(&mut self, start_sample: usize, len_samples: usize) -> AudioExpressionFrame {
        self.frames
            .set_block(start_sample, len_samples, self.mapping)
    }

    pub fn stream_for_midi_note(&self, midi_note: u8, velocity: f32) -> ExpressionStream {
        self.mapping
            .stream_for_midi_note(self.current_frame(), midi_note, velocity)
    }
}

impl<Frames, const VOICES: usize> AudioExpressionSource<Frames, VOICES>
where
    Frames: StreamingAudioExpressionFrameSource,
{
    pub fn set_audio_block(&mut self, start_sample: usize, audio: &[f32]) -> AudioExpressionFrame {
        self.frames
            .set_audio_block(start_sample, audio, self.mapping)
    }
}

impl<Frames, const VOICES: usize> ExpressionSource for AudioExpressionSource<Frames, VOICES>
where
    Frames: AudioExpressionFrameSource,
{
    fn voice_started(&mut self, voice_id: u32, _channel: u8, note: u8, velocity: f32) {
        if let Some(voice) = self.voices.get_mut(voice_id as usize) {
            *voice = AudioExpressionVoice {
                note,
                velocity,
                active: true,
            };
        }
    }

    fn voice_released(&mut self, voice_id: u32) {
        if let Some(voice) = self.voices.get_mut(voice_id as usize) {
            voice.active = false;
        }
    }

    fn next_block(&mut self, voice_id: u32) -> ExpressionStream {
        let Some(voice) = self.voices.get(voice_id as usize).copied() else {
            return ExpressionStream::default();
        };
        if !voice.active {
            return ExpressionStream::default();
        }

        self.stream_for_midi_note(voice.note, voice.velocity)
    }
}

pub type AudioAnalysisExpressionSource<'a, const VOICES: usize> =
    AudioExpressionSource<PhraseAnalysisExpressionFrameSource<'a>, VOICES>;

impl<'a, const VOICES: usize> AudioAnalysisExpressionSource<'a, VOICES> {
    pub fn new(
        audio: &'a [f32],
        sample_rate: u32,
        analysis: &'a PhraseAnalysisResult,
        mapping: AudioExpressionMapping,
    ) -> Self {
        Self::from_frame_source(
            PhraseAnalysisExpressionFrameSource::new(audio, sample_rate, analysis),
            mapping,
        )
    }
}

pub type StreamingAudioAnalysisExpressionSource<P, O, L, const VOICES: usize> =
    AudioExpressionSource<StreamingAudioAnalysisFrameSource<P, O, L>, VOICES>;

impl<P, O, L, const VOICES: usize>
    AudioExpressionSource<StreamingAudioAnalysisFrameSource<P, O, L>, VOICES>
where
    P: StreamingPitchTracker,
    O: StreamingOnsetDetector,
    L: StreamingLoudnessTracker,
{
    pub fn from_streaming_analysis(
        pitch_tracker: P,
        onset_detector: O,
        loudness_tracker: L,
        sample_rate: u32,
        mapping: AudioExpressionMapping,
    ) -> Self {
        Self::from_frame_source(
            StreamingAudioAnalysisFrameSource::new(
                pitch_tracker,
                onset_detector,
                loudness_tracker,
                sample_rate,
            ),
            mapping,
        )
    }
}

#[derive(Debug, Clone)]
pub struct StreamingAudioAnalysisFrameSource<P, O, L> {
    pitch_tracker: P,
    onset_detector: O,
    loudness_tracker: L,
    sample_rate: u32,
    frame: AudioExpressionFrame,
    held_pitch_hz: Option<f32>,
    gate: bool,
    last_pitch_error: Option<PitchDetectionError>,
}

impl<P, O, L> StreamingAudioAnalysisFrameSource<P, O, L> {
    pub fn new(pitch_tracker: P, onset_detector: O, loudness_tracker: L, sample_rate: u32) -> Self {
        Self {
            pitch_tracker,
            onset_detector,
            loudness_tracker,
            sample_rate: sample_rate.max(1),
            frame: AudioExpressionFrame::default(),
            held_pitch_hz: None,
            gate: false,
            last_pitch_error: None,
        }
    }

    pub fn last_pitch_error(&self) -> Option<&PitchDetectionError> {
        self.last_pitch_error.as_ref()
    }
}

impl<P, O, L> StreamingAudioAnalysisFrameSource<P, O, L>
where
    P: StreamingPitchTracker,
    O: StreamingOnsetDetector,
    L: StreamingLoudnessTracker,
{
    pub fn reset(&mut self) {
        self.pitch_tracker.reset();
        self.onset_detector.reset();
        self.loudness_tracker.reset();
        self.frame = AudioExpressionFrame::default();
        self.held_pitch_hz = None;
        self.gate = false;
        self.last_pitch_error = None;
    }

    fn expression_frame_from_audio(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame {
        let pitch_hz = match self.pitch_tracker.next_block(audio) {
            Ok(frames) => latest_voiced_pitch(frames).or(self.held_pitch_hz),
            Err(error) => {
                self.last_pitch_error = Some(error);
                self.held_pitch_hz
            }
        };
        if let Some(pitch_hz) = pitch_hz {
            self.held_pitch_hz = Some(pitch_hz);
        }

        let markers = self.onset_detector.next_block(audio);
        let onset = !markers.is_empty();
        let loudness = self
            .loudness_tracker
            .next_block(start_sample, audio, self.sample_rate);
        let mapping = mapping.sanitized();
        let active_floor = mapping.pressure_floor_rms;
        let release_floor = active_floor * 0.25;
        self.gate =
            onset || loudness.rms >= active_floor || (self.gate && loudness.rms >= release_floor);

        mapping.frame_from_features(AudioExpressionFeatures {
            start_sample: loudness.start_sample,
            end_sample: loudness.end_sample,
            pitch_hz: self.gate.then_some(pitch_hz).flatten(),
            loudness_rms: loudness.rms,
            spectral_centroid_hz: loudness.spectral_centroid_hz,
            gate: self.gate,
        })
    }
}

impl<P, O, L> AudioExpressionFrameSource for StreamingAudioAnalysisFrameSource<P, O, L>
where
    P: StreamingPitchTracker,
    O: StreamingOnsetDetector,
    L: StreamingLoudnessTracker,
{
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
        self.frame.end_sample = start_sample.saturating_add(len_samples);
        self.frame
    }
}

impl<P, O, L> StreamingAudioExpressionFrameSource for StreamingAudioAnalysisFrameSource<P, O, L>
where
    P: StreamingPitchTracker,
    O: StreamingOnsetDetector,
    L: StreamingLoudnessTracker,
{
    fn set_audio_block(
        &mut self,
        start_sample: usize,
        audio: &[f32],
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame {
        self.frame = self.expression_frame_from_audio(start_sample, audio, mapping);
        self.frame
    }
}

#[derive(Debug, Clone)]
pub struct PhraseAnalysisExpressionFrameSource<'a> {
    audio: &'a [f32],
    sample_rate: u32,
    analysis: &'a PhraseAnalysisResult,
    frame: AudioExpressionFrame,
}

impl<'a> PhraseAnalysisExpressionFrameSource<'a> {
    pub fn new(audio: &'a [f32], sample_rate: u32, analysis: &'a PhraseAnalysisResult) -> Self {
        Self {
            audio,
            sample_rate: sample_rate.max(1),
            analysis,
            frame: AudioExpressionFrame::default(),
        }
    }

    fn expression_frame(
        &self,
        start_sample: usize,
        end_sample: usize,
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame {
        let note_index = self.strongest_note_overlap(start_sample, end_sample);
        let Some(note_index) = note_index else {
            return AudioExpressionFrame {
                start_sample,
                end_sample,
                ..AudioExpressionFrame::default()
            };
        };

        let note = self.analysis.detected_notes[note_index];
        let frames = self
            .analysis
            .pitch_contour
            .frames_in_range(start_sample, end_sample);
        let pitch_hz = median_voiced_pitch(frames).or(Some(note.pitch_hz));
        let loudness = if frames.is_empty() {
            rms(self.audio.get(start_sample..end_sample).unwrap_or_default()).max(note.mean_rms)
        } else {
            frames
                .iter()
                .map(|frame| frame.rms)
                .filter(|value| value.is_finite())
                .sum::<f32>()
                / frames.len() as f32
        };
        let audio = self.audio.get(start_sample..end_sample).unwrap_or_default();
        let centroid = spectral_centroid_hz(audio, self.sample_rate as f32).unwrap_or(0.0);

        mapping.frame_from_features(AudioExpressionFeatures {
            start_sample,
            end_sample,
            pitch_hz,
            loudness_rms: loudness,
            spectral_centroid_hz: centroid,
            gate: true,
        })
    }

    fn strongest_note_overlap(&self, start_sample: usize, end_sample: usize) -> Option<usize> {
        self.analysis
            .detected_notes
            .iter()
            .enumerate()
            .filter_map(|(index, note)| {
                let start = start_sample.max(note.start_sample);
                let end = end_sample.min(note.end_sample);
                if end > start {
                    Some((index, end - start))
                } else {
                    None
                }
            })
            .max_by_key(|(_, overlap)| *overlap)
            .map(|(index, _)| index)
    }
}

impl AudioExpressionFrameSource for PhraseAnalysisExpressionFrameSource<'_> {
    fn current_frame(&self) -> AudioExpressionFrame {
        self.frame
    }

    fn set_block(
        &mut self,
        start_sample: usize,
        len_samples: usize,
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame {
        let start = start_sample.min(self.audio.len());
        let end = start.saturating_add(len_samples).min(self.audio.len());
        self.frame = self.expression_frame(start, end, mapping);
        self.frame
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AudioExpressionVoice {
    note: u8,
    velocity: f32,
    active: bool,
}

fn map_linear(value: f32, floor: f32, ceiling: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    if ceiling <= floor {
        return if value >= ceiling { 1.0 } else { 0.0 };
    }

    ((value - floor) / (ceiling - floor)).clamp(0.0, 1.0)
}

fn latest_voiced_pitch(frames: &[PitchFrame]) -> Option<f32> {
    frames.iter().rev().find_map(|frame| {
        frame
            .f0_hz
            .filter(|pitch| pitch.is_finite() && *pitch > 0.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_midi::DetectedNote;
    use lindelion_onset_detect::{MarkerKind, SliceMarker, StreamingOnsetDetector};
    use lindelion_phrase_analysis::{PhraseAnalysisResult, SegmentedNote};
    use lindelion_pitch_detect::{
        PitchContour, PitchDetectionError, PitchFrame, StreamingPitchTracker,
    };

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
        frames: Vec<PitchFrame>,
    }

    impl FixedPitchTracker {
        fn new(pitch_hz: f32) -> Self {
            Self {
                pitch_hz,
                frames: Vec::new(),
            }
        }
    }

    impl StreamingPitchTracker for FixedPitchTracker {
        fn next_block(&mut self, audio: &[f32]) -> Result<&[PitchFrame], PitchDetectionError> {
            self.frames.clear();
            if !audio.is_empty() {
                self.frames.push(pitch_frame(0, 0, self.pitch_hz, 0.2));
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

    fn sine_wave(frequency_hz: f32, amplitude: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| {
                let phase =
                    std::f32::consts::TAU * frequency_hz * index as f32 / SAMPLE_RATE as f32;
                phase.sin() * amplitude
            })
            .collect()
    }
}
