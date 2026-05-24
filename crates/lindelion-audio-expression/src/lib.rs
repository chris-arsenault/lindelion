mod loudness;
mod note;
mod phrase;
mod source;
mod streaming;

pub use loudness::{RmsCentroidLoudnessTracker, StreamingLoudnessFrame, StreamingLoudnessTracker};
pub use note::{
    AudioNoteDetectionConfig, AudioNoteEvent, RealtimeStreamingAudioAnalysisNoteDetector,
    StreamingAudioAnalysisNoteDetector, StreamingAudioNoteDetector,
    realtime_audio_analysis_note_detector, streaming_audio_analysis_note_detector,
};
pub use phrase::{AudioAnalysisExpressionSource, PhraseAnalysisExpressionFrameSource};
pub use source::AudioExpressionSource;
pub use streaming::{
    DefaultStreamingAudioAnalysisExpressionSource, RealtimeStreamingAudioAnalysisExpressionSource,
    StreamingAudioAnalysisExpressionSource, StreamingAudioAnalysisFrameSource,
    realtime_audio_analysis_expression_source, streaming_audio_analysis_expression_source,
};

use lindelion_dsp_utils::math::{finite_clamp, hz_to_midi_note};
use lindelion_plugin_shell::ExpressionStream;
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

fn map_linear(value: f32, floor: f32, ceiling: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    if ceiling <= floor {
        return if value >= ceiling { 1.0 } else { 0.0 };
    }

    ((value - floor) / (ceiling - floor)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
