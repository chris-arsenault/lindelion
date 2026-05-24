use lindelion_plugin_shell::{ExpressionSource, ExpressionStream};

use crate::{
    AudioExpressionFrame, AudioExpressionFrameSource, AudioExpressionMapping,
    StreamingAudioExpressionFrameSource,
};

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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AudioExpressionVoice {
    note: u8,
    velocity: f32,
    active: bool,
}
