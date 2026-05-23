#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    Note(NoteEvent),
    Control(ControlEvent),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEvent {
    On {
        channel: u8,
        note: u8,
        velocity: f32,
    },
    Off {
        channel: u8,
        note: u8,
        velocity: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlEvent {
    PitchBend {
        channel: u8,
        semitones: f32,
    },
    ChannelPressure {
        channel: u8,
        value: f32,
    },
    PolyPressure {
        channel: u8,
        note: u8,
        value: f32,
    },
    ContinuousController {
        channel: u8,
        controller: u8,
        value: f32,
    },
}

pub const MIDI_CHANNEL_COUNT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpressionStream {
    pub pitch_bend: f32,
    pub pressure: f32,
    pub brightness: f32,
    pub velocity: f32,
    pub gate: bool,
}

impl Default for ExpressionStream {
    fn default() -> Self {
        Self {
            pitch_bend: 0.0,
            pressure: 0.0,
            brightness: 0.0,
            velocity: 0.0,
            gate: false,
        }
    }
}

impl ExpressionStream {
    pub fn note_on(velocity: f32) -> Self {
        Self {
            velocity: sanitize_unit(velocity),
            gate: true,
            ..Self::default()
        }
    }

    pub fn sanitized(self) -> Self {
        Self {
            pitch_bend: sanitize_bipolar_semitones(self.pitch_bend),
            pressure: sanitize_unit(self.pressure),
            brightness: sanitize_unit(self.brightness),
            velocity: sanitize_unit(self.velocity),
            gate: self.gate,
        }
    }
}

pub trait ExpressionSource {
    fn next_block(&mut self, voice_id: u32) -> ExpressionStream;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ManualExpressionSource<const VOICES: usize> {
    streams: [ExpressionStream; VOICES],
}

impl<const VOICES: usize> Default for ManualExpressionSource<VOICES> {
    fn default() -> Self {
        Self {
            streams: [ExpressionStream::default(); VOICES],
        }
    }
}

impl<const VOICES: usize> ManualExpressionSource<VOICES> {
    pub fn set_voice_stream(&mut self, voice_id: u32, stream: ExpressionStream) -> bool {
        let Some(slot) = self.streams.get_mut(voice_id as usize) else {
            return false;
        };
        *slot = stream.sanitized();
        true
    }

    pub fn voice_stream(&self, voice_id: u32) -> Option<ExpressionStream> {
        self.streams
            .get(voice_id as usize)
            .map(|stream| stream.sanitized())
    }
}

impl<const VOICES: usize> ExpressionSource for ManualExpressionSource<VOICES> {
    fn next_block(&mut self, voice_id: u32) -> ExpressionStream {
        self.voice_stream(voice_id).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiVoiceExpression {
    pub stream: ExpressionStream,
    pub mod_wheel: f32,
}

impl Default for MidiVoiceExpression {
    fn default() -> Self {
        Self {
            stream: ExpressionStream::default(),
            mod_wheel: 0.0,
        }
    }
}

impl MidiVoiceExpression {
    pub fn sanitized(self) -> Self {
        Self {
            stream: self.stream.sanitized(),
            mod_wheel: sanitize_unit(self.mod_wheel),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiExpressionUpdate {
    pub channel: u8,
    pub expression: MidiVoiceExpression,
}

#[derive(Debug, Clone, Copy)]
pub struct MidiExpressionSource<const VOICES: usize> {
    channels: [MidiChannelExpression; MIDI_CHANNEL_COUNT],
    voices: [MidiExpressionVoice; VOICES],
}

impl<const VOICES: usize> Default for MidiExpressionSource<VOICES> {
    fn default() -> Self {
        Self {
            channels: [MidiChannelExpression::default(); MIDI_CHANNEL_COUNT],
            voices: [MidiExpressionVoice::default(); VOICES],
        }
    }
}

impl<const VOICES: usize> MidiExpressionSource<VOICES> {
    pub fn apply_control(
        &mut self,
        control: ControlEvent,
        pitch_bend_range_semitones: f32,
    ) -> Option<MidiExpressionUpdate> {
        match control {
            ControlEvent::PitchBend { channel, semitones } => {
                Some(self.set_pitch_bend(channel, semitones, pitch_bend_range_semitones))
            }
            ControlEvent::ChannelPressure { channel, value } => {
                Some(self.set_pressure(channel, value))
            }
            ControlEvent::ContinuousController {
                channel,
                controller: 1,
                value,
            } => Some(self.set_mod_wheel(channel, value)),
            ControlEvent::ContinuousController {
                channel,
                controller: 74,
                value,
            } => Some(self.set_brightness(channel, value)),
            ControlEvent::ContinuousController { .. } | ControlEvent::PolyPressure { .. } => None,
        }
    }

    pub fn set_pitch_bend(
        &mut self,
        channel: u8,
        semitones: f32,
        range_semitones: f32,
    ) -> MidiExpressionUpdate {
        let channel = sanitize_channel(channel);
        let range = sanitize_pitch_bend_range(range_semitones);
        self.channel_mut(channel).stream.pitch_bend =
            sanitize_bipolar_range(semitones, -range, range);
        self.apply_channel_to_voices(channel);
        self.update_for_channel(channel)
    }

    pub fn set_pressure(&mut self, channel: u8, value: f32) -> MidiExpressionUpdate {
        let channel = sanitize_channel(channel);
        self.channel_mut(channel).stream.pressure = sanitize_unit(value);
        self.apply_channel_to_voices(channel);
        self.update_for_channel(channel)
    }

    pub fn set_brightness(&mut self, channel: u8, value: f32) -> MidiExpressionUpdate {
        let channel = sanitize_channel(channel);
        self.channel_mut(channel).stream.brightness = sanitize_unit(value);
        self.apply_channel_to_voices(channel);
        self.update_for_channel(channel)
    }

    pub fn set_mod_wheel(&mut self, channel: u8, value: f32) -> MidiExpressionUpdate {
        let channel = sanitize_channel(channel);
        self.channel_mut(channel).mod_wheel = sanitize_unit(value);
        self.apply_channel_to_voices(channel);
        self.update_for_channel(channel)
    }

    pub fn note_expression(&self, channel: u8, velocity: f32) -> MidiVoiceExpression {
        self.channel(channel).with_note(velocity, true)
    }

    pub fn channel_expression(&self, channel: u8) -> MidiVoiceExpression {
        self.channel(channel).expression()
    }

    pub fn begin_voice(
        &mut self,
        voice_id: u32,
        channel: u8,
        velocity: f32,
    ) -> MidiVoiceExpression {
        let expression = self.note_expression(channel, velocity);
        if let Some(voice) = self.voice_mut(voice_id) {
            voice.active = true;
            voice.channel = sanitize_channel(channel);
            voice.stream = expression.stream;
        }
        expression
    }

    pub fn set_voice_gate(&mut self, voice_id: u32, gate: bool) -> Option<ExpressionStream> {
        let voice = self.voice_mut(voice_id)?;
        voice.stream.gate = gate;
        Some(voice.stream.sanitized())
    }

    fn apply_channel_to_voices(&mut self, channel: u8) {
        let expression = self.channel_expression(channel);
        for voice in &mut self.voices {
            if voice.active && (channel == 0 || voice.channel == channel) {
                voice.stream.pitch_bend = expression.stream.pitch_bend;
                voice.stream.pressure = expression.stream.pressure;
                voice.stream.brightness = expression.stream.brightness;
                voice.stream = voice.stream.sanitized();
            }
        }
    }

    fn update_for_channel(&self, channel: u8) -> MidiExpressionUpdate {
        MidiExpressionUpdate {
            channel,
            expression: self.channel_expression(channel),
        }
    }

    fn channel(&self, channel: u8) -> MidiChannelExpression {
        self.channels[usize::from(sanitize_channel(channel))]
    }

    fn channel_mut(&mut self, channel: u8) -> &mut MidiChannelExpression {
        &mut self.channels[usize::from(sanitize_channel(channel))]
    }

    fn voice_mut(&mut self, voice_id: u32) -> Option<&mut MidiExpressionVoice> {
        self.voices.get_mut(voice_id as usize)
    }
}

impl<const VOICES: usize> ExpressionSource for MidiExpressionSource<VOICES> {
    fn next_block(&mut self, voice_id: u32) -> ExpressionStream {
        self.voices
            .get(voice_id as usize)
            .map(|voice| voice.stream.sanitized())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MidiChannelExpression {
    stream: ExpressionStream,
    mod_wheel: f32,
}

impl Default for MidiChannelExpression {
    fn default() -> Self {
        Self {
            stream: ExpressionStream::default(),
            mod_wheel: 0.0,
        }
    }
}

impl MidiChannelExpression {
    fn expression(self) -> MidiVoiceExpression {
        MidiVoiceExpression {
            stream: self.stream,
            mod_wheel: self.mod_wheel,
        }
        .sanitized()
    }

    fn with_note(self, velocity: f32, gate: bool) -> MidiVoiceExpression {
        let mut expression = self.expression();
        expression.stream.velocity = sanitize_unit(velocity);
        expression.stream.gate = gate;
        expression
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct MidiExpressionVoice {
    active: bool,
    channel: u8,
    stream: ExpressionStream,
}

fn sanitize_channel(channel: u8) -> u8 {
    channel.min((MIDI_CHANNEL_COUNT - 1) as u8)
}

fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn sanitize_bipolar_semitones(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-96.0, 96.0)
    } else {
        0.0
    }
}

fn sanitize_pitch_bend_range(value: f32) -> f32 {
    if value.is_finite() { value.abs() } else { 0.0 }
}

fn sanitize_bipolar_range(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_expression_source_maps_controls_to_stream_and_trait_voice_state() {
        let mut source = MidiExpressionSource::<4>::default();

        source.apply_control(
            ControlEvent::PitchBend {
                channel: 2,
                semitones: 1.5,
            },
            2.0,
        );
        source.apply_control(
            ControlEvent::ChannelPressure {
                channel: 2,
                value: 0.75,
            },
            2.0,
        );
        source.apply_control(
            ControlEvent::ContinuousController {
                channel: 2,
                controller: 74,
                value: 0.25,
            },
            2.0,
        );
        source.apply_control(
            ControlEvent::ContinuousController {
                channel: 2,
                controller: 1,
                value: 0.5,
            },
            2.0,
        );

        let expression = source.begin_voice(1, 2, 0.8);

        assert_eq!(expression.stream.pitch_bend, 1.5);
        assert_eq!(expression.stream.pressure, 0.75);
        assert_eq!(expression.stream.brightness, 0.25);
        assert_eq!(expression.stream.velocity, 0.8);
        assert!(expression.stream.gate);
        assert_eq!(expression.mod_wheel, 0.5);
        assert_eq!(source.next_block(1), expression.stream);

        let released = source.set_voice_gate(1, false).unwrap();

        assert!(!released.gate);
        assert_eq!(source.next_block(1), released);
    }

    #[test]
    fn midi_expression_source_applies_member_and_global_channel_updates_to_active_voices() {
        let mut source = MidiExpressionSource::<4>::default();
        source.begin_voice(0, 1, 1.0);
        source.begin_voice(1, 2, 1.0);

        source.apply_control(
            ControlEvent::ChannelPressure {
                channel: 1,
                value: 0.6,
            },
            2.0,
        );

        assert_eq!(source.next_block(0).pressure, 0.6);
        assert_eq!(source.next_block(1).pressure, 0.0);

        source.apply_control(
            ControlEvent::PitchBend {
                channel: 0,
                semitones: 2.0,
            },
            2.0,
        );

        assert_eq!(source.next_block(0).pitch_bend, 2.0);
        assert_eq!(source.next_block(1).pitch_bend, 2.0);
    }

    #[test]
    fn manual_expression_source_returns_sanitized_per_voice_streams() {
        let mut source = ManualExpressionSource::<2>::default();
        let stream = ExpressionStream {
            pitch_bend: 144.0,
            pressure: 1.25,
            brightness: 0.6,
            velocity: 0.8,
            gate: true,
        };

        assert!(source.set_voice_stream(1, stream));
        assert!(!source.set_voice_stream(2, stream));

        assert_eq!(
            source.next_block(1),
            ExpressionStream {
                pitch_bend: 96.0,
                pressure: 1.0,
                brightness: 0.6,
                velocity: 0.8,
                gate: true,
            }
        );
        assert_eq!(source.next_block(2), ExpressionStream::default());
    }
}
