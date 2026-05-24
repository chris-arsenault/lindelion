use lindelion_dsp_utils::math::{finite_clamp, hz_to_midi_note};
use lindelion_onset_detect::{
    AlgorithmParams, DetectionAlgorithm, DetectionConfig, OnsetDetectionProfile,
    StreamingEnergyTransientDetector, StreamingOnsetDetector, StreamingSuperFluxDetector,
};
use lindelion_pitch_detect::{
    PitchDetectionConfig, PitchDetectionError, PitchFrame, StreamingPitchTracker,
    SwiftF0StreamingPitchTracker, ZeroCrossingStreamingPitchTracker,
};

use crate::{
    DEFAULT_AUDIO_NOTE_MINIMUM_LENGTH_MS, DEFAULT_AUDIO_NOTE_ONSET_SENSITIVITY,
    DEFAULT_AUDIO_NOTE_PITCH_CONFIDENCE, DEFAULT_AUDIO_NOTE_RELEASE_FLOOR_RMS,
    DEFAULT_AUDIO_NOTE_VELOCITY_AMOUNT, RmsCentroidLoudnessTracker, StreamingLoudnessTracker,
};

const DEFAULT_AUDIO_NOTE_VELOCITY: f32 = 100.0 / 127.0;
const MAX_STREAMING_AUDIO_NOTE_EVENTS_PER_BLOCK: usize = 32;
const MAX_STREAMING_AUDIO_NOTE_ONSETS_PER_BLOCK: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
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

pub(crate) fn realtime_onset_detection_config(config: AudioNoteDetectionConfig) -> DetectionConfig {
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
