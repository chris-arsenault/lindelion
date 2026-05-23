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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiControllerRoute {
    pub source_controller: u32,
    pub target_controller: u8,
}

impl MidiControllerRoute {
    pub const fn new(source_controller: u32, target_controller: u8) -> Self {
        Self {
            source_controller,
            target_controller,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiExpressionControl {
    ModWheel,
    Brightness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiExpressionControlRoute {
    pub controller: u8,
    pub target: MidiExpressionControl,
}

impl MidiExpressionControlRoute {
    pub const fn new(controller: u8, target: MidiExpressionControl) -> Self {
        Self { controller, target }
    }
}

const STANDARD_MIDI_EXPRESSION_CONTROL_ROUTES: &[MidiExpressionControlRoute] = &[
    MidiExpressionControlRoute::new(1, MidiExpressionControl::ModWheel),
    MidiExpressionControlRoute::new(74, MidiExpressionControl::Brightness),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiExpressionMapping<'a> {
    controller_routes: &'a [MidiExpressionControlRoute],
}

impl<'a> MidiExpressionMapping<'a> {
    pub const fn new(controller_routes: &'a [MidiExpressionControlRoute]) -> Self {
        Self { controller_routes }
    }

    pub const fn standard() -> MidiExpressionMapping<'static> {
        MidiExpressionMapping::new(STANDARD_MIDI_EXPRESSION_CONTROL_ROUTES)
    }

    fn target_for_controller(self, controller: u8) -> Option<MidiExpressionControl> {
        self.controller_routes
            .iter()
            .find_map(|route| (route.controller == controller).then_some(route.target))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiEventNormalizer<'a> {
    controller_routes: &'a [MidiControllerRoute],
    pitch_bend_range_semitones: f32,
}

impl<'a> MidiEventNormalizer<'a> {
    pub const fn new(
        controller_routes: &'a [MidiControllerRoute],
        pitch_bend_range_semitones: f32,
    ) -> Self {
        Self {
            controller_routes,
            pitch_bend_range_semitones,
        }
    }

    pub fn normalize(self, event: HostMidiEvent) -> Option<MidiEvent> {
        match event {
            HostMidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => Some(MidiEvent::Note(NoteEvent::On {
                channel: sanitize_host_channel(channel),
                note: sanitize_host_midi7(note),
                velocity: sanitize_unit(velocity),
            })),
            HostMidiEvent::NoteOff {
                channel,
                note,
                velocity,
            } => Some(MidiEvent::Note(NoteEvent::Off {
                channel: sanitize_host_channel(channel),
                note: sanitize_host_midi7(note),
                velocity: sanitize_unit(velocity),
            })),
            HostMidiEvent::PolyPressure {
                channel,
                note,
                pressure,
            } => Some(MidiEvent::Control(ControlEvent::PolyPressure {
                channel: sanitize_host_channel(channel),
                note: sanitize_host_midi7(note),
                value: sanitize_unit(pressure),
            })),
            HostMidiEvent::ContinuousController {
                channel,
                controller,
                value,
            } => self.controller_routes.iter().find_map(|route| {
                (route.source_controller == controller).then(|| {
                    MidiEvent::Control(ControlEvent::ContinuousController {
                        channel: sanitize_host_channel(channel),
                        controller: route.target_controller,
                        value: normalize_midi7(value),
                    })
                })
            }),
            HostMidiEvent::ChannelPressure { channel, value } => {
                Some(MidiEvent::Control(ControlEvent::ChannelPressure {
                    channel: sanitize_host_channel(channel),
                    value: normalize_midi7(value),
                }))
            }
            HostMidiEvent::PitchBend { channel, lsb, msb } => {
                Some(MidiEvent::Control(ControlEvent::PitchBend {
                    channel: sanitize_host_channel(channel),
                    semitones: normalize_pitch_bend_semitones(
                        lsb,
                        msb,
                        self.pitch_bend_range_semitones,
                    ),
                }))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostMidiEvent {
    NoteOn {
        channel: i32,
        note: i32,
        velocity: f32,
    },
    NoteOff {
        channel: i32,
        note: i32,
        velocity: f32,
    },
    PolyPressure {
        channel: i32,
        note: i32,
        pressure: f32,
    },
    ContinuousController {
        channel: i32,
        controller: u32,
        value: i32,
    },
    ChannelPressure {
        channel: i32,
        value: i32,
    },
    PitchBend {
        channel: i32,
        lsb: i32,
        msb: i32,
    },
}

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
        self.apply_control_with_mapping(
            control,
            pitch_bend_range_semitones,
            MidiExpressionMapping::standard(),
        )
    }

    pub fn apply_control_with_mapping(
        &mut self,
        control: ControlEvent,
        pitch_bend_range_semitones: f32,
        mapping: MidiExpressionMapping<'_>,
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
                controller,
                value,
            } => match mapping.target_for_controller(controller) {
                Some(MidiExpressionControl::ModWheel) => Some(self.set_mod_wheel(channel, value)),
                Some(MidiExpressionControl::Brightness) => {
                    Some(self.set_brightness(channel, value))
                }
                None => None,
            },
            ControlEvent::PolyPressure { .. } => None,
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

fn sanitize_host_channel(channel: i32) -> u8 {
    channel.clamp(0, (MIDI_CHANNEL_COUNT - 1) as i32) as u8
}

fn sanitize_host_midi7(value: i32) -> u8 {
    value.clamp(0, 127) as u8
}

fn normalize_midi7(value: i32) -> f32 {
    f32::from(sanitize_host_midi7(value)) / 127.0
}

fn normalize_pitch_bend_semitones(lsb: i32, msb: i32, range_semitones: f32) -> f32 {
    let raw = i32::from(sanitize_host_midi7(lsb)) | (i32::from(sanitize_host_midi7(msb)) << 7);
    let range = sanitize_pitch_bend_range(range_semitones);
    ((raw as f32 - 8_192.0) / 8_192.0).clamp(-1.0, 1.0) * range
}

fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn sanitize_bipolar_semitones(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
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

    const RESONATOR_CONTROLLER_ROUTES: &[MidiControllerRoute] = &[
        MidiControllerRoute::new(1, 1),
        MidiControllerRoute::new(74, 74),
    ];

    #[test]
    fn midi_event_normalizer_maps_standard_host_fixture() {
        let normalizer = MidiEventNormalizer::new(RESONATOR_CONTROLLER_ROUTES, 2.0);

        assert_eq!(
            normalizer.normalize(HostMidiEvent::NoteOn {
                channel: 2,
                note: 64,
                velocity: 0.75,
            }),
            Some(MidiEvent::Note(NoteEvent::On {
                channel: 2,
                note: 64,
                velocity: 0.75,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::NoteOff {
                channel: 2,
                note: 64,
                velocity: 0.5,
            }),
            Some(MidiEvent::Note(NoteEvent::Off {
                channel: 2,
                note: 64,
                velocity: 0.5,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::ContinuousController {
                channel: 2,
                controller: 1,
                value: 64,
            }),
            Some(MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 2,
                controller: 1,
                value: 64.0 / 127.0,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::ContinuousController {
                channel: 2,
                controller: 74,
                value: 127,
            }),
            Some(MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 2,
                controller: 74,
                value: 1.0,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::ChannelPressure {
                channel: 2,
                value: 96,
            }),
            Some(MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 2,
                value: 96.0 / 127.0,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::PolyPressure {
                channel: 2,
                note: 64,
                pressure: 0.6,
            }),
            Some(MidiEvent::Control(ControlEvent::PolyPressure {
                channel: 2,
                note: 64,
                value: 0.6,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::PitchBend {
                channel: 2,
                lsb: 0,
                msb: 96,
            }),
            Some(MidiEvent::Control(ControlEvent::PitchBend {
                channel: 2,
                semitones: 1.0,
            }))
        );
    }

    #[test]
    fn midi_event_normalizer_uses_plugin_controller_routes_and_pitch_range() {
        let alternate_routes = &[MidiControllerRoute::new(11, 3)];
        let normalizer = MidiEventNormalizer::new(alternate_routes, 12.0);

        assert_eq!(
            normalizer.normalize(HostMidiEvent::ContinuousController {
                channel: 4,
                controller: 1,
                value: 127,
            }),
            None
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::ContinuousController {
                channel: 4,
                controller: 11,
                value: 127,
            }),
            Some(MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 4,
                controller: 3,
                value: 1.0,
            }))
        );
        assert_eq!(
            normalizer.normalize(HostMidiEvent::PitchBend {
                channel: 4,
                lsb: 0,
                msb: 96,
            }),
            Some(MidiEvent::Control(ControlEvent::PitchBend {
                channel: 4,
                semitones: 6.0,
            }))
        );
    }

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
    fn midi_expression_source_uses_plugin_expression_mapping() {
        let mut source = MidiExpressionSource::<4>::default();
        let routes = [
            MidiExpressionControlRoute::new(11, MidiExpressionControl::ModWheel),
            MidiExpressionControlRoute::new(12, MidiExpressionControl::Brightness),
        ];
        let mapping = MidiExpressionMapping::new(&routes);

        assert_eq!(
            source.apply_control_with_mapping(
                ControlEvent::ContinuousController {
                    channel: 1,
                    controller: 1,
                    value: 1.0,
                },
                2.0,
                mapping,
            ),
            None
        );

        source.apply_control_with_mapping(
            ControlEvent::ContinuousController {
                channel: 1,
                controller: 11,
                value: 0.4,
            },
            2.0,
            mapping,
        );
        source.apply_control_with_mapping(
            ControlEvent::ContinuousController {
                channel: 1,
                controller: 12,
                value: 0.7,
            },
            2.0,
            mapping,
        );

        let expression = source.channel_expression(1);
        assert_eq!(expression.mod_wheel, 0.4);
        assert_eq!(expression.stream.brightness, 0.7);
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
                pitch_bend: 144.0,
                pressure: 1.0,
                brightness: 0.6,
                velocity: 0.8,
                gate: true,
            }
        );
        assert_eq!(source.next_block(2), ExpressionStream::default());
    }
}
