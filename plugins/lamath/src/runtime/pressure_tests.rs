#[test]
fn channel_pressure_modulates_resonator_damping_for_held_voice() {
    let sample_rate = 48_000.0;
    let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        aftertouch_resonator_damping_patch(),
    );
    let mut pressed_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        aftertouch_resonator_damping_patch(),
    );
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 48,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    pressed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 8192];
    let mut neutral_right = vec![0.0; 8192];
    let mut pressed_left = vec![0.0; 8192];
    let mut pressed_right = vec![0.0; 8192];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    pressed_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 1.0,
        })],
        &mut pressed_left,
        &mut pressed_right,
    );

    assert_eq!(neutral_processor.active_voice_count(), 1);
    assert_eq!(pressed_processor.active_voice_count(), 1);
    assert_all_finite(&neutral_left);
    assert_all_finite(&neutral_right);
    assert_all_finite(&pressed_left);
    assert_all_finite(&pressed_right);
    assert!(rms(&neutral_left) > 0.000_001);
    assert!(
        rms(&pressed_left) > rms(&neutral_left) * 1.25,
        "neutral_rms={}, pressed_rms={}, diff={}",
        rms(&neutral_left),
        rms(&pressed_left),
        mean_abs_difference(&neutral_left, &pressed_left)
    );
}

#[test]
fn poly_pressure_modulates_only_target_note_for_held_voices() {
    let sample_rate = 48_000.0;
    let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let mut pressed_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let notes = [
        MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 48,
            velocity: 1.0,
        }),
        MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        }),
    ];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process(&notes, &mut warmup_left, &mut warmup_right);
    pressed_processor.process(&notes, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 8192];
    let mut neutral_right = vec![0.0; 8192];
    let mut pressed_left = vec![0.0; 8192];
    let mut pressed_right = vec![0.0; 8192];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::PolyPressure {
            channel: 0,
            note: 48,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    pressed_processor.process(
        &[MidiEvent::Control(ControlEvent::PolyPressure {
            channel: 0,
            note: 48,
            value: 1.0,
        })],
        &mut pressed_left,
        &mut pressed_right,
    );

    assert_eq!(neutral_processor.active_voice_count(), 2);
    assert_eq!(pressed_processor.active_voice_count(), 2);
    assert_all_finite(&neutral_left);
    assert_all_finite(&neutral_right);
    assert_all_finite(&pressed_left);
    assert_all_finite(&pressed_right);
    assert!(rms(&neutral_left) > 0.000_001);
    assert!(
        mean_abs_difference(&neutral_left, &pressed_left) > rms(&neutral_left) * 0.05,
        "neutral_rms={}, pressed_rms={}, diff={}",
        rms(&neutral_left),
        rms(&pressed_left),
        mean_abs_difference(&neutral_left, &pressed_left)
    );
}

#[test]
fn member_channel_pressure_modulates_only_owned_voices() {
    let sample_rate = 48_000.0;
    let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let mut wrong_channel_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let mut matching_channel_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let notes = [
        MidiEvent::Note(NoteEvent::On {
            channel: 1,
            note: 48,
            velocity: 1.0,
        }),
        MidiEvent::Note(NoteEvent::On {
            channel: 2,
            note: 60,
            velocity: 1.0,
        }),
    ];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process(&notes, &mut warmup_left, &mut warmup_right);
    wrong_channel_processor.process(&notes, &mut warmup_left, &mut warmup_right);
    matching_channel_processor.process(&notes, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 8192];
    let mut neutral_right = vec![0.0; 8192];
    let mut wrong_left = vec![0.0; 8192];
    let mut wrong_right = vec![0.0; 8192];
    let mut matching_left = vec![0.0; 8192];
    let mut matching_right = vec![0.0; 8192];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 3,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    wrong_channel_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 3,
            value: 1.0,
        })],
        &mut wrong_left,
        &mut wrong_right,
    );
    matching_channel_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 1,
            value: 1.0,
        })],
        &mut matching_left,
        &mut matching_right,
    );

    assert_eq!(neutral_processor.active_voice_count(), 2);
    assert_eq!(wrong_channel_processor.active_voice_count(), 2);
    assert_eq!(matching_channel_processor.active_voice_count(), 2);
    assert_all_finite(&neutral_left);
    assert_all_finite(&wrong_left);
    assert_all_finite(&matching_left);
    assert!(rms(&neutral_left) > 0.000_001);
    assert!(
        mean_abs_difference(&neutral_left, &wrong_left) < 1.0e-7,
        "neutral/wrong diff={}",
        mean_abs_difference(&neutral_left, &wrong_left)
    );
    assert!(
        mean_abs_difference(&neutral_left, &matching_left) > rms(&neutral_left) * 0.05,
        "neutral_rms={}, matching_rms={}, diff={}",
        rms(&neutral_left),
        rms(&matching_left),
        mean_abs_difference(&neutral_left, &matching_left)
    );
}

#[test]
fn channel_zero_pressure_remains_global_for_ordinary_midi() {
    let sample_rate = 48_000.0;
    let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let mut global_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        poly_pressure_resonator_damping_patch(),
    );
    let notes = [
        MidiEvent::Note(NoteEvent::On {
            channel: 1,
            note: 48,
            velocity: 1.0,
        }),
        MidiEvent::Note(NoteEvent::On {
            channel: 2,
            note: 60,
            velocity: 1.0,
        }),
    ];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process(&notes, &mut warmup_left, &mut warmup_right);
    global_processor.process(&notes, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 8192];
    let mut neutral_right = vec![0.0; 8192];
    let mut global_left = vec![0.0; 8192];
    let mut global_right = vec![0.0; 8192];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    global_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 1.0,
        })],
        &mut global_left,
        &mut global_right,
    );

    assert_all_finite(&neutral_left);
    assert_all_finite(&global_left);
    assert!(
        rms(&global_left) > rms(&neutral_left) * 1.25,
        "neutral_rms={}, global_rms={}, diff={}",
        rms(&neutral_left),
        rms(&global_left),
        mean_abs_difference(&neutral_left, &global_left)
    );
}

#[test]
fn midi_controllers_keep_independent_member_channel_state() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 128];
    let mut right = vec![0.0; 128];

    processor.process(
        &[
            MidiEvent::Control(ControlEvent::PitchBend {
                channel: 2,
                semitones: 1.5,
            }),
            MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 2,
                value: 0.6,
            }),
            MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 2,
                controller: 1,
                value: 0.7,
            }),
            MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 2,
                controller: 74,
                value: 0.8,
            }),
        ],
        &mut left,
        &mut right,
    );

    let untouched = processor.expression_source.channel_expression(1);
    let updated = processor.expression_source.channel_expression(2);
    assert_eq!(untouched.stream.pitch_bend, 0.0);
    assert_eq!(untouched.stream.pressure, 0.0);
    assert_eq!(untouched.mod_wheel, 0.0);
    assert_eq!(untouched.stream.brightness, 0.0);
    assert_eq!(updated.stream.pitch_bend, 1.5);
    assert_eq!(updated.stream.pressure, 0.6);
    assert_eq!(updated.mod_wheel, 0.7);
    assert_eq!(updated.stream.brightness, 0.8);
}

