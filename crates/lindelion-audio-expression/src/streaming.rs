use lindelion_pitch_detect::{
    PitchDetectionConfig, PitchDetectionError, PitchFrame, StreamingPitchTracker,
    SwiftF0StreamingPitchTracker, ZeroCrossingStreamingPitchTracker,
};

use crate::{
    AudioExpressionFeatures, AudioExpressionFrame, AudioExpressionFrameSource,
    AudioExpressionMapping, AudioExpressionSource, AudioNoteDetectionConfig,
    RmsCentroidLoudnessTracker, StreamingAudioExpressionFrameSource, StreamingLoudnessTracker,
    note::realtime_onset_detection_config,
};
use lindelion_onset_detect::{
    StreamingEnergyTransientDetector, StreamingOnsetDetector, StreamingSuperFluxDetector,
};

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

fn latest_voiced_pitch(frames: &[PitchFrame]) -> Option<f32> {
    frames.iter().rev().find_map(|frame| {
        frame
            .f0_hz
            .filter(|pitch| pitch.is_finite() && *pitch > 0.0)
    })
}
