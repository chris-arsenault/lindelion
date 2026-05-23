#[test]
fn mod_wheel_modulates_resonator_damping_for_held_voice() {
    let sample_rate = 48_000.0;
    let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        mod_wheel_resonator_damping_patch(),
    );
    let mut pushed_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        mod_wheel_resonator_damping_patch(),
    );
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 48,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    pushed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 8192];
    let mut neutral_right = vec![0.0; 8192];
    let mut pushed_left = vec![0.0; 8192];
    let mut pushed_right = vec![0.0; 8192];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 1,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    pushed_processor.process(
        &[MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 1,
            value: 1.0,
        })],
        &mut pushed_left,
        &mut pushed_right,
    );

    assert_eq!(neutral_processor.active_voice_count(), 1);
    assert_eq!(pushed_processor.active_voice_count(), 1);
    assert_all_finite(&neutral_left);
    assert_all_finite(&neutral_right);
    assert_all_finite(&pushed_left);
    assert_all_finite(&pushed_right);
    assert!(rms(&neutral_left) > 0.000_001);
    assert!(
        rms(&pushed_left) > rms(&neutral_left) * 1.25,
        "neutral_rms={}, pushed_rms={}, diff={}",
        rms(&neutral_left),
        rms(&pushed_left),
        mean_abs_difference(&neutral_left, &pushed_left)
    );
}

#[test]
fn brightness_modulates_resonator_damping_for_held_voice() {
    let sample_rate = 48_000.0;
    let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        brightness_resonator_damping_patch(),
    );
    let mut bright_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        brightness_resonator_damping_patch(),
    );
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 48,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    bright_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 8192];
    let mut neutral_right = vec![0.0; 8192];
    let mut bright_left = vec![0.0; 8192];
    let mut bright_right = vec![0.0; 8192];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 74,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    bright_processor.process(
        &[MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 74,
            value: 1.0,
        })],
        &mut bright_left,
        &mut bright_right,
    );

    assert_eq!(neutral_processor.active_voice_count(), 1);
    assert_eq!(bright_processor.active_voice_count(), 1);
    assert_all_finite(&neutral_left);
    assert_all_finite(&neutral_right);
    assert_all_finite(&bright_left);
    assert_all_finite(&bright_right);
    assert!(rms(&neutral_left) > 0.000_001);
    assert!(
        rms(&bright_left) > rms(&neutral_left) * 1.25,
        "neutral_rms={}, bright_rms={}, diff={}",
        rms(&neutral_left),
        rms(&bright_left),
        mean_abs_difference(&neutral_left, &bright_left)
    );
}

#[test]
fn pitch_bend_parameter_retunes_active_voice() {
    let sample_rate = 48_000.0;
    let mut center_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_waveguide_patch(),
    );
    let mut bent_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_waveguide_patch(),
    );
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 128];
    let mut warmup_right = vec![0.0; 128];

    center_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    bent_processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        &mut warmup_left,
        &mut warmup_right,
    );
    bent_processor.set_pitch_bend_normalized(1.0);

    let mut center_left = vec![0.0; 8192];
    let mut center_right = vec![0.0; 8192];
    let mut bent_left = vec![0.0; 8192];
    let mut bent_right = vec![0.0; 8192];
    center_processor.process(&[], &mut center_left, &mut center_right);
    bent_processor.process(&[], &mut bent_left, &mut bent_right);

    assert_all_finite(&center_left);
    assert_all_finite(&bent_left);
    assert!(peak_abs(&center_left) > 0.000_001);
    assert!(peak_abs(&bent_left) > 0.000_001);

    let center_frequency = midi_note_to_hz(60.0);
    let bent_frequency = midi_note_to_hz(62.0);
    assert!(
        dft_magnitude_at(&center_left, sample_rate, center_frequency)
            > dft_magnitude_at(&center_left, sample_rate, bent_frequency)
    );
    assert!(
        dft_magnitude_at(&bent_left, sample_rate, bent_frequency)
            > dft_magnitude_at(&bent_left, sample_rate, center_frequency)
    );
}

#[test]
fn member_channel_pitch_bend_updates_only_owned_held_voice_expression() {
    let sample_rate = 48_000.0;
    let mut processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_polyphonic_waveguide_patch(),
    );
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process(&two_member_channel_notes(), &mut left, &mut right);
    processor.process(
        &[MidiEvent::Control(ControlEvent::PitchBend {
            channel: 1,
            semitones: 2.0,
        })],
        &mut left,
        &mut right,
    );

    let bent = expression_for_slot(&processor, 1, 48);
    let untouched = expression_for_slot(&processor, 2, 60);
    assert_eq!(bent.stream.pitch_bend, 2.0);
    assert_eq!(untouched.stream.pitch_bend, 0.0);
}

#[test]
fn channel_zero_pitch_bend_updates_all_held_voice_expressions() {
    let sample_rate = 48_000.0;
    let mut processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_polyphonic_waveguide_patch(),
    );
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process(&two_member_channel_notes(), &mut left, &mut right);
    processor.process(
        &[MidiEvent::Control(ControlEvent::PitchBend {
            channel: 0,
            semitones: 2.0,
        })],
        &mut left,
        &mut right,
    );

    assert_eq!(
        expression_for_slot(&processor, 1, 48).stream.pitch_bend,
        2.0
    );
    assert_eq!(
        expression_for_slot(&processor, 2, 60).stream.pitch_bend,
        2.0
    );
}

#[test]
fn member_channel_pitch_bend_retunes_owned_voice_render() {
    let sample_rate = 48_000.0;
    let mut center_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_waveguide_patch(),
    );
    let mut bent_processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_waveguide_patch(),
    );
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 1,
        note: 60,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 128];
    let mut warmup_right = vec![0.0; 128];

    center_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    bent_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    bent_processor.process(
        &[MidiEvent::Control(ControlEvent::PitchBend {
            channel: 1,
            semitones: 2.0,
        })],
        &mut warmup_left,
        &mut warmup_right,
    );

    let mut center_left = vec![0.0; 8192];
    let mut center_right = vec![0.0; 8192];
    let mut bent_left = vec![0.0; 8192];
    let mut bent_right = vec![0.0; 8192];
    center_processor.process(&[], &mut center_left, &mut center_right);
    bent_processor.process(&[], &mut bent_left, &mut bent_right);

    assert_all_finite(&center_left);
    assert_all_finite(&bent_left);
    assert_frequency_dominates(&center_left, sample_rate, 60.0, 62.0);
    assert_frequency_dominates(&bent_left, sample_rate, 62.0, 60.0);
}

#[test]
fn runtime_note_off_events_route_gate_through_owned_expression_streams() {
    let sample_rate = 48_000.0;
    let mut processor = ResonatorProcessor::with_builtin_excitation(
        sample_rate,
        pitch_tracking_polyphonic_waveguide_patch(),
    );
    let mut left = vec![0.0; 128];
    let mut right = vec![0.0; 128];

    processor.process(&two_member_channel_notes(), &mut left, &mut right);
    processor.process(
        &[MidiEvent::Note(NoteEvent::Off {
            channel: 2,
            note: 60,
            velocity: 0.0,
        })],
        &mut left,
        &mut right,
    );
    assert_slot_expression_gate(&processor, 1, 48, true);
    assert_slot_expression_gate(&processor, 2, 60, false);

    processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 1,
            note: 48,
            velocity: 0.0,
        })],
        &mut left,
        &mut right,
    );
    assert_slot_expression_gate(&processor, 1, 48, false);
    assert_slot_expression_gate(&processor, 2, 60, false);
}

#[test]
fn state_roundtrip_preserves_patch() {
    let mut synth = crate::ResonatorSynth::default();
    let mut patch = test_patch();
    patch.name = "Roundtrip".to_string();
    synth.set_patch_for_test(patch.clone());

    let state = lindelion_plugin_shell::AudioPlugin::state(&synth);
    let mut restored = crate::ResonatorSynth::default();
    lindelion_plugin_shell::AudioPlugin::load_state(&mut restored, state);

    assert_eq!(restored.patch().name, "Roundtrip");
    assert_eq!(restored.patch().output.filter_mode, FilterMode::BandPass);
}
