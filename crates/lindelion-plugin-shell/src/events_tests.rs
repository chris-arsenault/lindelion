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
