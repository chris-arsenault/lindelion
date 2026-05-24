use lindelion_dsp_utils::{
    analysis::{rms, spectral_centroid_hz},
    math::{finite_clamp, hz_to_midi_note},
};
use lindelion_onset_detect::{
    AlgorithmParams, DetectionAlgorithm, DetectionConfig, OnsetDetectionProfile,
    StreamingEnergyTransientDetector, StreamingOnsetDetector, StreamingSuperFluxDetector,
};
use lindelion_phrase_analysis::PhraseAnalysisResult;
use lindelion_pitch_detect::{
    PitchDetectionConfig, PitchDetectionError, PitchFrame, StreamingPitchTracker,
    SwiftF0StreamingPitchTracker, ZeroCrossingStreamingPitchTracker, median_voiced_pitch,
};
use lindelion_plugin_shell::{ExpressionSource, ExpressionStream};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PITCH_BEND_RANGE_SEMITONES: f32 = 2.0;
pub const DEFAULT_PRESSURE_FLOOR_RMS: f32 = 0.02;
pub const DEFAULT_PRESSURE_CEILING_RMS: f32 = 0.35;
pub const DEFAULT_BRIGHTNESS_FLOOR_HZ: f32 = 500.0;
pub const DEFAULT_BRIGHTNESS_CEILING_HZ: f32 = 6_000.0;
pub const DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY: f32 = 0.5;
pub const DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS: f32 = 0.01;
pub const DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS: f32 = 60.0;
pub const DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE: f32 = 0.65;
pub const DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT: f32 = 1.0;
const DEFAULT_AUDIO_NOTE_VELOCITY: f32 = 100.0 / 127.0;
const MAX_STREAMING_AUDIO_NOTE_EVENTS_PER_BLOCK: usize = 32;
const MAX_STREAMING_AUDIO_NOTE_ONSETS_PER_BLOCK: usize = 16;

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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioNoteDetectionConfig {
    pub onset_sensitivity: f32,
    pub note_release_floor_rms: f32,
    pub minimum_note_length_ms: f32,
    pub pitch_confidence: f32,
    pub velocity_amount: f32,
}

impl Default for AudioNoteDetectionConfig {
    fn default() -> Self {
        Self {
            onset_sensitivity: DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
            note_release_floor_rms: DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
            minimum_note_length_ms: DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS,
            pitch_confidence: DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE,
            velocity_amount: DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT,
        }
    }
}

impl AudioNoteDetectionConfig {
    pub fn sanitized(self) -> Self {
        Self {
            onset_sensitivity: finite_clamp(
                self.onset_sensitivity,
                0.0,
                1.0,
                DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
            ),
            note_release_floor_rms: finite_clamp(
                self.note_release_floor_rms,
                0.0,
                1.0,
                DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
            ),
            minimum_note_length_ms: finite_clamp(
                self.minimum_note_length_ms,
                1.0,
                2_000.0,
                DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS,
            ),
            pitch_confidence: finite_clamp(
                self.pitch_confidence,
                0.0,
                1.0,
                DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE,
            ),
            velocity_amount: finite_clamp(
                self.velocity_amount,
                0.0,
                1.0,
                DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT,
            ),
        }
    }

    pub fn onset_detection_config(self) -> DetectionConfig {
        let config = self.sanitized();
        DetectionConfig::superflux(
            config.onset_sensitivity,
            config.minimum_note_length_ms,
            OnsetDetectionProfile::default(),
        )
    }

    pub fn pitch_detection_config(self) -> PitchDetectionConfig {
        PitchDetectionConfig {
            confidence_threshold: self.sanitized().pitch_confidence,
            ..PitchDetectionConfig::default()
        }
    }

    pub fn minimum_note_length_samples(self, sample_rate: u32) -> usize {
        ((self.sanitized().minimum_note_length_ms * 0.001) * sample_rate.max(1) as f32)
            .round()
            .max(1.0) as usize
    }
}

fn realtime_onset_detection_config(config: AudioNoteDetectionConfig) -> DetectionConfig {
    let config = config.sanitized();
    DetectionConfig {
        algorithm: DetectionAlgorithm::EnergyTransient,
        sensitivity: config.onset_sensitivity,
        min_slice_ms: config.minimum_note_length_ms,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::EnergyTransient { frame_size: 512 },
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioNoteEvent {
    pub offset: usize,
    pub note: u8,
    pub velocity: f32,
    pub pitch_hz: f32,
    pub confidence: f32,
    pub gate: bool,
}

impl AudioNoteEvent {
    pub fn note_on(offset: usize, pitch_hz: f32, confidence: f32, velocity: f32) -> Option<Self> {
        Some(Self {
            offset,
            note: note_from_pitch_hz(pitch_hz)?,
            velocity: finite_clamp(velocity, 0.0, 1.0, DEFAULT_AUDIO_NOTE_VELOCITY),
            pitch_hz: sanitize_pitch_hz(pitch_hz),
            confidence: finite_clamp(confidence, 0.0, 1.0, 0.0),
            gate: true,
        })
    }

    pub fn note_off(offset: usize, note: u8, pitch_hz: f32, confidence: f32) -> Self {
        Self {
            offset,
            note,
            velocity: 0.0,
            pitch_hz: sanitize_pitch_hz(pitch_hz),
            confidence: finite_clamp(confidence, 0.0, 1.0, 0.0),
            gate: false,
        }
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

pub type DefaultStreamingAudioAnalysisExpressionSource<const VOICES: usize> =
    StreamingAudioAnalysisExpressionSource<
        SwiftF0StreamingPitchTracker,
        StreamingSuperFluxDetector,
        RmsCentroidLoudnessTracker,
        VOICES,
    >;

pub type RealtimeStreamingAudioAnalysisExpressionSource<const VOICES: usize> =
    StreamingAudioAnalysisExpressionSource<
        ZeroCrossingStreamingPitchTracker,
        StreamingEnergyTransientDetector,
        RmsCentroidLoudnessTracker,
        VOICES,
    >;

pub type StreamingAudioAnalysisNoteDetector = StreamingAudioNoteDetector<
    SwiftF0StreamingPitchTracker,
    StreamingSuperFluxDetector,
    RmsCentroidLoudnessTracker,
>;

pub type RealtimeStreamingAudioAnalysisNoteDetector = StreamingAudioNoteDetector<
    ZeroCrossingStreamingPitchTracker,
    StreamingEnergyTransientDetector,
    RmsCentroidLoudnessTracker,
>;

/// Builds the higher-quality default streaming expression source. Realtime plugin
/// processing should use `realtime_audio_analysis_expression_source`.
pub fn streaming_audio_analysis_expression_source<const VOICES: usize>(
    sample_rate: u32,
    mapping: AudioExpressionMapping,
) -> DefaultStreamingAudioAnalysisExpressionSource<VOICES> {
    AudioExpressionSource::from_streaming_analysis(
        SwiftF0StreamingPitchTracker::new(sample_rate, PitchDetectionConfig::default()),
        StreamingSuperFluxDetector::new(
            sample_rate,
            AudioNoteDetectionConfig::default().onset_detection_config(),
        ),
        RmsCentroidLoudnessTracker::default(),
        sample_rate,
        mapping,
    )
}

/// Builds a streaming expression source whose detector buffers are prepared for
/// process blocks up to `max_block_size`.
pub fn realtime_audio_analysis_expression_source<const VOICES: usize>(
    sample_rate: u32,
    max_block_size: usize,
    mapping: AudioExpressionMapping,
) -> RealtimeStreamingAudioAnalysisExpressionSource<VOICES> {
    AudioExpressionSource::from_streaming_analysis(
        ZeroCrossingStreamingPitchTracker::new(sample_rate, PitchDetectionConfig::default()),
        StreamingEnergyTransientDetector::with_realtime_capacity(
            sample_rate,
            realtime_onset_detection_config(AudioNoteDetectionConfig::default()),
            max_block_size,
        ),
        RmsCentroidLoudnessTracker::default(),
        sample_rate,
        mapping,
    )
}

/// Builds the higher-quality default streaming note detector. Realtime plugin
/// processing should use `realtime_audio_analysis_note_detector`.
pub fn streaming_audio_analysis_note_detector(
    sample_rate: u32,
    config: AudioNoteDetectionConfig,
) -> StreamingAudioAnalysisNoteDetector {
    let pitch_config = PitchDetectionConfig {
        confidence_threshold: 0.0,
        ..config.pitch_detection_config()
    };
    StreamingAudioNoteDetector::new(
        SwiftF0StreamingPitchTracker::new(sample_rate, pitch_config),
        StreamingSuperFluxDetector::new(sample_rate, config.onset_detection_config()),
        RmsCentroidLoudnessTracker::default(),
        sample_rate,
    )
}

/// Builds a streaming note detector whose event and analysis buffers are bounded
/// and prepared for process blocks up to `max_block_size`.
pub fn realtime_audio_analysis_note_detector(
    sample_rate: u32,
    max_block_size: usize,
    config: AudioNoteDetectionConfig,
) -> RealtimeStreamingAudioAnalysisNoteDetector {
    let pitch_config = PitchDetectionConfig {
        confidence_threshold: 0.0,
        ..config.pitch_detection_config()
    };
    StreamingAudioNoteDetector::new(
        ZeroCrossingStreamingPitchTracker::new(sample_rate, pitch_config),
        StreamingEnergyTransientDetector::with_realtime_capacity(
            sample_rate,
            realtime_onset_detection_config(config),
            max_block_size,
        ),
        RmsCentroidLoudnessTracker::default(),
        sample_rate,
    )
}

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
pub struct StreamingAudioNoteDetector<P, O, L> {
    pitch_tracker: P,
    onset_detector: O,
    loudness_tracker: L,
    sample_rate: u32,
    samples_seen: usize,
    held_pitch: Option<AudioNotePitch>,
    active_note: Option<ActiveAudioNote>,
    events: [AudioNoteEvent; MAX_STREAMING_AUDIO_NOTE_EVENTS_PER_BLOCK],
    event_count: usize,
    onset_offsets: [usize; MAX_STREAMING_AUDIO_NOTE_ONSETS_PER_BLOCK],
    onset_offset_count: usize,
    last_pitch_error: Option<PitchDetectionError>,
}

impl<P, O, L> StreamingAudioNoteDetector<P, O, L> {
    pub fn new(pitch_tracker: P, onset_detector: O, loudness_tracker: L, sample_rate: u32) -> Self {
        Self {
            pitch_tracker,
            onset_detector,
            loudness_tracker,
            sample_rate: sample_rate.max(1),
            samples_seen: 0,
            held_pitch: None,
            active_note: None,
            events: [AudioNoteEvent::default(); MAX_STREAMING_AUDIO_NOTE_EVENTS_PER_BLOCK],
            event_count: 0,
            onset_offsets: [0; MAX_STREAMING_AUDIO_NOTE_ONSETS_PER_BLOCK],
            onset_offset_count: 0,
            last_pitch_error: None,
        }
    }

    pub fn last_pitch_error(&self) -> Option<&PitchDetectionError> {
        self.last_pitch_error.as_ref()
    }

    pub fn active_note(&self) -> Option<AudioNoteEvent> {
        let active = self.active_note?;
        Some(AudioNoteEvent {
            offset: 0,
            note: active.note,
            velocity: active.velocity,
            pitch_hz: active.pitch_hz,
            confidence: active.confidence,
            gate: true,
        })
    }
}

impl<P, O, L> StreamingAudioNoteDetector<P, O, L>
where
    P: StreamingPitchTracker,
    O: StreamingOnsetDetector,
    L: StreamingLoudnessTracker,
{
    pub fn next_block(
        &mut self,
        audio: &[f32],
        config: AudioNoteDetectionConfig,
    ) -> &[AudioNoteEvent] {
        self.event_count = 0;
        self.onset_offset_count = 0;
        if audio.is_empty() {
            return self.events();
        }

        let block_start = self.samples_seen;
        let config = config.sanitized();
        let pitch = self.detect_pitch(audio, config.pitch_confidence);
        if let Some(pitch) = pitch {
            self.held_pitch = Some(pitch);
        }

        {
            let onset_detector = &mut self.onset_detector;
            let onset_offsets = &mut self.onset_offsets;
            let mut onset_offset_count = 0;
            for marker in onset_detector.next_block(audio) {
                let Some(offset) = block_offset(marker.position_samples, block_start, audio.len())
                else {
                    continue;
                };
                if onset_offset_count >= onset_offsets.len() {
                    break;
                }
                onset_offsets[onset_offset_count] = offset;
                onset_offset_count += 1;
            }
            self.onset_offset_count = onset_offset_count;
        }

        let loudness = self
            .loudness_tracker
            .next_block(block_start, audio, self.sample_rate);
        let pitch = pitch.or(self.held_pitch);

        for index in 0..self.onset_offset_count {
            let offset = self.onset_offsets[index];
            self.start_note(offset, block_start + offset, pitch, loudness.rms, config);
        }

        if self.event_count == 0 {
            self.maybe_release_note(0, block_start, loudness.rms, config);
        }

        self.samples_seen = self.samples_seen.saturating_add(audio.len());
        self.events()
    }

    pub fn reset(&mut self) {
        self.pitch_tracker.reset();
        self.onset_detector.reset();
        self.loudness_tracker.reset();
        self.samples_seen = 0;
        self.held_pitch = None;
        self.active_note = None;
        self.event_count = 0;
        self.onset_offset_count = 0;
        self.last_pitch_error = None;
    }

    fn detect_pitch(&mut self, audio: &[f32], min_confidence: f32) -> Option<AudioNotePitch> {
        match self.pitch_tracker.next_block(audio) {
            Ok(frames) => latest_confident_pitch(frames, min_confidence),
            Err(error) => {
                self.last_pitch_error = Some(error);
                None
            }
        }
    }

    fn start_note(
        &mut self,
        offset: usize,
        sample_position: usize,
        pitch: Option<AudioNotePitch>,
        loudness_rms: f32,
        config: AudioNoteDetectionConfig,
    ) {
        let Some(pitch) = pitch else {
            return;
        };
        if let Some(active) = self.active_note {
            let min_samples = config.minimum_note_length_samples(self.sample_rate);
            if sample_position.saturating_sub(active.start_sample) < min_samples {
                return;
            }
            self.push_event(AudioNoteEvent::note_off(
                offset,
                active.note,
                active.pitch_hz,
                active.confidence,
            ));
        }

        let velocity = velocity_from_rms(loudness_rms, config.velocity_amount);
        let Some(event) =
            AudioNoteEvent::note_on(offset, pitch.pitch_hz, pitch.confidence, velocity)
        else {
            return;
        };
        self.active_note = Some(ActiveAudioNote {
            note: event.note,
            pitch_hz: event.pitch_hz,
            confidence: event.confidence,
            velocity: event.velocity,
            start_sample: sample_position,
        });
        self.push_event(event);
    }

    fn maybe_release_note(
        &mut self,
        offset: usize,
        sample_position: usize,
        loudness_rms: f32,
        config: AudioNoteDetectionConfig,
    ) {
        let Some(active) = self.active_note else {
            return;
        };
        let min_samples = config.minimum_note_length_samples(self.sample_rate);
        if sample_position.saturating_sub(active.start_sample) < min_samples {
            return;
        }
        if loudness_rms >= config.note_release_floor_rms {
            return;
        }

        self.push_event(AudioNoteEvent::note_off(
            offset,
            active.note,
            active.pitch_hz,
            active.confidence,
        ));
        self.active_note = None;
    }

    fn events(&self) -> &[AudioNoteEvent] {
        &self.events[..self.event_count]
    }

    fn push_event(&mut self, event: AudioNoteEvent) {
        if self.event_count >= self.events.len() {
            return;
        }
        self.events[self.event_count] = event;
        self.event_count += 1;
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AudioNotePitch {
    pitch_hz: f32,
    confidence: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct ActiveAudioNote {
    note: u8,
    pitch_hz: f32,
    confidence: f32,
    velocity: f32,
    start_sample: usize,
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

fn latest_confident_pitch(frames: &[PitchFrame], min_confidence: f32) -> Option<AudioNotePitch> {
    let min_confidence = finite_clamp(
        min_confidence,
        0.0,
        1.0,
        DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE,
    );
    frames.iter().rev().find_map(|frame| {
        let pitch_hz = frame
            .f0_hz
            .filter(|pitch| pitch.is_finite() && *pitch > 0.0)?;
        (frame.confidence >= min_confidence).then_some(AudioNotePitch {
            pitch_hz,
            confidence: finite_clamp(frame.confidence, 0.0, 1.0, 0.0),
        })
    })
}

fn block_offset(position_samples: usize, block_start: usize, block_len: usize) -> Option<usize> {
    if block_len == 0 {
        return None;
    }
    if position_samples < block_start {
        return Some(0);
    }
    let offset = position_samples - block_start;
    (offset < block_len).then_some(offset)
}

fn note_from_pitch_hz(pitch_hz: f32) -> Option<u8> {
    hz_to_midi_note(pitch_hz).map(|note| note.round().clamp(0.0, 127.0) as u8)
}

fn sanitize_pitch_hz(pitch_hz: f32) -> f32 {
    if pitch_hz.is_finite() && pitch_hz > 0.0 {
        pitch_hz
    } else {
        0.0
    }
}

fn velocity_from_rms(rms: f32, amount: f32) -> f32 {
    let amount = finite_clamp(amount, 0.0, 1.0, DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT);
    let dynamic = unit_velocity_from_rms(rms);
    DEFAULT_AUDIO_NOTE_VELOCITY + (dynamic - DEFAULT_AUDIO_NOTE_VELOCITY) * amount
}

fn unit_velocity_from_rms(rms: f32) -> f32 {
    let rms = finite_clamp(rms, 0.0, 1.0, 0.0).max(0.000_001);
    let db = 20.0 * rms.log10();
    ((db + 60.0) / 60.0).clamp(1.0 / 127.0, 1.0)
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
                let phase =
                    std::f32::consts::TAU * frequency_hz * index as f32 / SAMPLE_RATE as f32;
                phase.sin() * amplitude
            })
            .collect()
    }
}
