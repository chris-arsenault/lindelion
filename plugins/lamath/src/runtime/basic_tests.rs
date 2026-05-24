#[test]
fn processor_handles_note_events_and_renders_audio() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        &mut left,
        &mut right,
    );

    assert_eq!(processor.active_voice_count(), 1);
    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_001);
}

#[test]
fn processor_audio_path_does_not_allocate() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    assert_runtime_process_does_not_allocate(
        "processor process note-on",
        &mut processor,
        &events,
        &mut left,
        &mut right,
    );
    assert_runtime_process_does_not_allocate(
        "processor process render-only",
        &mut processor,
        &[],
        &mut left,
        &mut right,
    );
    let controls = [
        MidiEvent::Control(ControlEvent::PitchBend {
            channel: 0,
            semitones: 1.5,
        }),
        MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.75,
        }),
        MidiEvent::Control(ControlEvent::PolyPressure {
            channel: 0,
            note: 60,
            value: 0.65,
        }),
        MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 1,
            value: 0.5,
        }),
        MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 74,
            value: 0.25,
        }),
    ];
    assert_runtime_process_does_not_allocate(
        "processor process controls",
        &mut processor,
        &controls,
        &mut left,
        &mut right,
    );

    assert_live_control_path_does_not_allocate(
        "processor process live pressure resonator damping",
        aftertouch_resonator_damping_patch(),
        ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
    assert_live_control_path_does_not_allocate(
        "processor process live mod wheel resonator damping",
        mod_wheel_resonator_damping_patch(),
        ControlEvent::ContinuousController {
            channel: 0,
            controller: 1,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
    assert_live_control_path_does_not_allocate(
        "processor process live brightness resonator damping",
        brightness_resonator_damping_patch(),
        ControlEvent::ContinuousController {
            channel: 0,
            controller: 74,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
    assert_live_control_path_does_not_allocate(
        "processor process live poly pressure resonator damping",
        poly_pressure_resonator_damping_patch(),
        ControlEvent::PolyPressure {
            channel: 0,
            note: 60,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
}

#[test]
fn audio_note_events_use_voice_manager_and_release_by_owned_slot() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::MidiPlusAudioCreatesNotes;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let note_on = AudioNoteEvent::note_on(0, midi_note_to_hz(60.0), 0.95, 0.8).unwrap();

    processor.handle_audio_note_event(note_on);

    let active = processor.audio_note_state.active.unwrap();
    assert_eq!(active.slot, 0);
    assert_eq!(active.note, 60);
    assert!((active.pitch_hz - note_on.pitch_hz).abs() < 0.001);
    assert!((active.velocity - 0.8).abs() < 0.001);
    assert!((active.confidence - 0.95).abs() < 0.001);
    assert_eq!(processor.engine.slot_state(active.slot), Some(VoiceSlotState::Active));
    assert_eq!(processor.engine.slot_note(active.slot), Some(60));
    assert_eq!(processor.engine.slot_channel(active.slot), Some(AUDIO_NOTE_CHANNEL));
    let expression = processor.engine.slot_expression(active.slot).unwrap();
    assert!(expression.stream.gate);
    assert!((expression.stream.velocity - 0.8).abs() < 0.001);

    processor.handle_audio_note_event(AudioNoteEvent::note_off(
        256,
        note_on.note,
        note_on.pitch_hz,
        note_on.confidence,
    ));

    assert_eq!(processor.audio_note_state.active, None);
    assert_eq!(
        processor.engine.slot_state(active.slot),
        Some(VoiceSlotState::Released)
    );
    assert!(!processor
        .engine
        .slot_expression(active.slot)
        .unwrap()
        .stream
        .gate);
}

#[test]
fn audio_creates_notes_mode_ignores_midi_note_allocation() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::AudioCreatesNotes;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        &mut left,
        &mut right,
    );

    assert_eq!(processor.active_voice_count(), 0);
}

#[test]
fn audio_creates_notes_mode_ignores_midi_note_allocation_with_external_expression_source() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::AudioCreatesNotes;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let mut source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_expression_source(
        &mut source,
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        &mut left,
        &mut right,
    );

    assert_eq!(processor.active_voice_count(), 0);
}

#[test]
fn midi_only_mode_ignores_sidechain_note_creation_and_preserves_midi_allocation() {
    let mut patch = test_patch();
    configure_audio_note_detection(&mut patch, AudioInputMode::Off);
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(60.0, 0.6, 8_192);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[MidiEvent::Note(NoteEvent::On {
            channel: 2,
            note: 64,
            velocity: 0.75,
        })])
        .with_sidechain(&sidechain),
        &mut left,
        &mut right,
    );

    assert_eq!(processor.audio_note_state.active, None);
    assert_eq!(processor.active_voice_count(), 1);
    assert_eq!(expression_for_slot(&processor, 2, 64).stream.velocity, 0.75);
}

#[test]
fn midi_plus_audio_mode_keeps_midi_and_audio_voice_ownership_separate() {
    let patch = audio_note_detection_patch(test_patch(), AudioInputMode::MidiPlusAudioCreatesNotes);
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(60.0, 0.6, 8_192);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[MidiEvent::Note(NoteEvent::On {
            channel: 3,
            note: 64,
            velocity: 0.7,
        })])
        .with_sidechain(&sidechain),
        &mut left,
        &mut right,
    );

    let audio_voice = processor
        .audio_note_state
        .active
        .expect("sidechain should create an audio-owned voice");
    assert_ne!(audio_voice.slot, 0);
    assert_eq!(processor.engine.slot_channel(audio_voice.slot), Some(AUDIO_NOTE_CHANNEL));
    assert_eq!(processor.engine.slot_note(audio_voice.slot), Some(60));
    assert_eq!(expression_for_slot(&processor, 3, 64).stream.velocity, 0.7);
    assert_eq!(processor.active_voice_count(), 2);
}

#[test]
fn midi_note_off_does_not_release_audio_owned_voice() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::MidiPlusAudioCreatesNotes;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let note_on = AudioNoteEvent::note_on(0, midi_note_to_hz(60.0), 0.95, 0.8).unwrap();
    processor.handle_audio_note_event(note_on);
    let active = processor.audio_note_state.active.unwrap();

    processor.note_off(AUDIO_NOTE_CHANNEL, note_on.note);

    assert_eq!(
        processor.audio_note_state.active.map(|voice| voice.slot),
        Some(active.slot)
    );
    assert_eq!(processor.engine.slot_state(active.slot), Some(VoiceSlotState::Active));
    assert!(processor
        .engine
        .slot_expression(active.slot)
        .unwrap()
        .stream
        .gate);
}

#[test]
fn audio_note_off_does_not_release_midi_owned_voice_with_same_note() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::MidiPlusAudioCreatesNotes;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 4,
            note: 60,
            velocity: 0.7,
        })],
        &mut left,
        &mut right,
    );
    let midi_expression = expression_for_slot(&processor, 4, 60);
    let note_on = AudioNoteEvent::note_on(0, midi_note_to_hz(60.0), 0.95, 0.8).unwrap();
    processor.handle_audio_note_event(note_on);
    let audio_voice = processor.audio_note_state.active.unwrap();

    processor.handle_audio_note_event(AudioNoteEvent::note_off(
        128,
        note_on.note,
        note_on.pitch_hz,
        note_on.confidence,
    ));

    assert_eq!(
        expression_for_slot(&processor, 4, 60).stream.gate,
        midi_expression.stream.gate
    );
    assert_eq!(processor.engine.slot_state(audio_voice.slot), Some(VoiceSlotState::Released));
    assert_eq!(processor.audio_note_state.active, None);
}

#[test]
fn sidechain_audio_creates_note_and_empty_input_releases_it() {
    let mut patch = test_patch();
    configure_audio_note_detection(&mut patch, AudioInputMode::AudioCreatesNotes);
    patch.note_detection.note_release_floor_rms = 0.05;
    patch.note_detection.minimum_note_length_ms = 1.0;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(60.0, 0.6, 8_192);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&sidechain),
        &mut left,
        &mut right,
    );

    let active = processor
        .audio_note_state
        .active
        .expect("sidechain onset should create an audio-owned voice");
    assert_eq!(processor.engine.slot_state(active.slot), Some(VoiceSlotState::Active));
    assert_all_finite(&left);
    assert_all_finite(&right);

    processor.process_with_runtime_input(ResonatorRuntimeInput::new(&[]), &mut left, &mut right);

    assert_eq!(processor.audio_note_state.active, None);
    assert_eq!(
        processor.engine.slot_state(active.slot),
        Some(VoiceSlotState::Released)
    );
}

#[test]
fn audio_expression_pitch_drift_updates_audio_owned_voice_without_retriggering_note() {
    let mut patch = test_patch();
    configure_audio_note_detection(&mut patch, AudioInputMode::AudioCreatesNotes);
    patch.audio_expression.enabled = true;
    patch.audio_expression.mapping.pitch_bend_range_semitones = 12.0;
    patch.audio_expression.mapping.pressure_floor_rms = 0.0;
    patch.audio_expression.mapping.pressure_ceiling_rms = 0.5;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let initial = sidechain_sine_note(60.0, 0.6, 8_192);

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&initial),
        &mut left,
        &mut right,
    );

    let active = processor
        .audio_note_state
        .active
        .expect("initial sidechain block should create a note");
    assert_eq!(active.note, 60);
    assert_eq!(processor.engine.slot_note(active.slot), Some(60));

    let drift = sidechain_sine_note(62.0, 0.6, 8_192);
    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&drift),
        &mut left,
        &mut right,
    );

    let expression = processor.engine.slot_expression(active.slot).unwrap();
    assert_eq!(processor.audio_note_state.active.unwrap().slot, active.slot);
    assert_eq!(processor.audio_note_state.active.unwrap().note, 60);
    assert_eq!(processor.engine.slot_note(active.slot), Some(60));
    assert!(expression.stream.gate);
    assert!(
        expression.stream.pitch_bend > 1.0 && expression.stream.pitch_bend < 3.0,
        "pitch bend should express drift instead of retriggering: {}",
        expression.stream.pitch_bend
    );
    assert!(expression.stream.pressure > 0.4);
}

#[test]
fn audio_expression_does_not_replace_midi_owned_voice_expression() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::Off;
    patch.audio_expression.enabled = true;
    patch.audio_expression.mapping.pitch_bend_range_semitones = 12.0;
    patch.audio_expression.mapping.pressure_floor_rms = 0.0;
    patch.audio_expression.mapping.pressure_ceiling_rms = 0.5;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(62.0, 0.6, 8_192);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })])
        .with_sidechain(&sidechain),
        &mut left,
        &mut right,
    );

    let expression = expression_for_slot(&processor, 0, 60);
    assert_eq!(processor.audio_note_state.active, None);
    assert_eq!(expression.stream.pitch_bend, 0.0);
    assert_eq!(expression.stream.pressure, 0.0);
    assert_eq!(expression.stream.brightness, 0.0);
}

#[test]
fn live_excitation_off_preserves_midi_render_with_sidechain_input() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::Off;
    patch.live_excitation.mode = LiveExcitationMode::Off;
    let mut dry_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch.clone());
    let mut sidechain_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(67.0, 0.8, 512);
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut dry_left = vec![0.0; 512];
    let mut dry_right = vec![0.0; 512];
    let mut sidechain_left = vec![0.0; 512];
    let mut sidechain_right = vec![0.0; 512];

    dry_processor.process(&events, &mut dry_left, &mut dry_right);
    sidechain_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&sidechain),
        &mut sidechain_left,
        &mut sidechain_right,
    );

    assert_eq!(sidechain_left, dry_left);
    assert_eq!(sidechain_right, dry_right);
}

#[test]
fn continuous_live_excitation_changes_midi_created_voice_render() {
    let mut dry_patch = test_patch();
    dry_patch.audio_input.mode = AudioInputMode::Off;
    let mut live_patch = dry_patch.clone();
    live_patch.live_excitation.mode = LiveExcitationMode::Continuous;
    live_patch.live_excitation.gain_db = -6.0;
    let mut dry_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, dry_patch);
    let mut live_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, live_patch);
    let sidechain = sidechain_sine_note(67.0, 0.8, 512);
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut dry_left = vec![0.0; 512];
    let mut dry_right = vec![0.0; 512];
    let mut live_left = vec![0.0; 512];
    let mut live_right = vec![0.0; 512];

    dry_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&sidechain),
        &mut dry_left,
        &mut dry_right,
    );
    live_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&sidechain),
        &mut live_left,
        &mut live_right,
    );

    assert_eq!(dry_processor.active_voice_count(), 1);
    assert_eq!(live_processor.active_voice_count(), 1);
    assert_all_finite(&live_left);
    assert_all_finite(&live_right);
    assert!(
        mean_abs_difference(&dry_left, &live_left) > rms(&dry_left).max(0.000_001) * 0.05,
        "continuous live excitation should materially change MIDI-created voice output"
    );
}

#[test]
fn continuous_live_excitation_changes_audio_created_voice_render() {
    let dry_patch = audio_note_detection_patch(test_patch(), AudioInputMode::AudioCreatesNotes);
    let mut live_patch = dry_patch.clone();
    live_patch.live_excitation.mode = LiveExcitationMode::Continuous;
    live_patch.live_excitation.gain_db = -6.0;
    let mut dry_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, dry_patch);
    let mut live_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, live_patch);
    let sidechain = sidechain_sine_note(60.0, 0.8, 8_192);
    let mut dry_left = vec![0.0; 512];
    let mut dry_right = vec![0.0; 512];
    let mut live_left = vec![0.0; 512];
    let mut live_right = vec![0.0; 512];

    dry_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&sidechain),
        &mut dry_left,
        &mut dry_right,
    );
    live_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&sidechain),
        &mut live_left,
        &mut live_right,
    );

    assert!(dry_processor.audio_note_state.active.is_some());
    assert!(live_processor.audio_note_state.active.is_some());
    assert_all_finite(&live_left);
    assert_all_finite(&live_right);
    assert!(
        mean_abs_difference(&dry_left, &live_left) > rms(&dry_left).max(0.000_001) * 0.05,
        "continuous live excitation should materially change audio-created voice output"
    );
}

#[test]
fn continuous_live_excitation_sanitizes_sidechain_and_does_not_allocate() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::Off;
    patch.live_excitation.mode = LiveExcitationMode::Continuous;
    patch.live_excitation.gain_db = 24.0;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut sidechain = vec![0.0; 512];
    sidechain[0] = f32::NAN;
    sidechain[1] = f32::INFINITY;
    sidechain[2] = f32::NEG_INFINITY;
    sidechain[3] = 2.0;
    sidechain[4] = -2.0;
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process(&events, &mut left, &mut right);
    assert_runtime_input_process_does_not_allocate(
        "processor continuous live excitation",
        &mut processor,
        &[],
        &sidechain,
        &mut left,
        &mut right,
    );
}

#[test]
fn note_latched_live_excitation_changes_midi_created_voice_render() {
    let mut dry_patch = test_patch();
    dry_patch.audio_input.mode = AudioInputMode::Off;
    let mut latch_patch = dry_patch.clone();
    latch_patch.live_excitation.mode = LiveExcitationMode::NoteLatched;
    latch_patch.live_excitation.gain_db = 6.0;
    latch_patch.live_excitation.latch_window_ms = 20.0;
    latch_patch.live_excitation.latch_pre_roll_ms = 0.0;
    latch_patch.live_excitation.latch_fade_ms = 0.0;
    let mut dry_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, dry_patch);
    let mut latch_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, latch_patch);
    let sidechain = sidechain_sine_note(67.0, 0.8, 2_048);
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut dry_left = vec![0.0; 512];
    let mut dry_right = vec![0.0; 512];
    let mut latch_left = vec![0.0; 512];
    let mut latch_right = vec![0.0; 512];

    dry_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&sidechain),
        &mut dry_left,
        &mut dry_right,
    );
    latch_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&sidechain),
        &mut latch_left,
        &mut latch_right,
    );

    assert_eq!(dry_processor.active_voice_count(), 1);
    assert_eq!(latch_processor.active_voice_count(), 1);
    assert_all_finite(&latch_left);
    assert_all_finite(&latch_right);
    assert!(
        mean_abs_difference(&dry_left, &latch_left) > rms(&dry_left).max(0.000_001) * 0.05,
        "note-latched excitation should materially change MIDI-created voice output"
    );
}

#[test]
fn note_latched_live_excitation_changes_audio_created_voice_render() {
    let dry_patch = audio_note_detection_patch(test_patch(), AudioInputMode::AudioCreatesNotes);
    let mut latch_patch = dry_patch.clone();
    latch_patch.live_excitation.mode = LiveExcitationMode::NoteLatched;
    latch_patch.live_excitation.gain_db = 6.0;
    latch_patch.live_excitation.latch_window_ms = 20.0;
    latch_patch.live_excitation.latch_pre_roll_ms = 0.0;
    latch_patch.live_excitation.latch_fade_ms = 0.0;
    let mut dry_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, dry_patch);
    let mut latch_processor = ResonatorProcessor::with_builtin_excitation(48_000.0, latch_patch);
    let sidechain = sidechain_sine_note(60.0, 0.8, 8_192);
    let mut dry_left = vec![0.0; 512];
    let mut dry_right = vec![0.0; 512];
    let mut latch_left = vec![0.0; 512];
    let mut latch_right = vec![0.0; 512];

    dry_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&sidechain),
        &mut dry_left,
        &mut dry_right,
    );
    latch_processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&sidechain),
        &mut latch_left,
        &mut latch_right,
    );

    assert!(dry_processor.audio_note_state.active.is_some());
    assert!(latch_processor.audio_note_state.active.is_some());
    assert_all_finite(&latch_left);
    assert_all_finite(&latch_right);
    assert!(
        mean_abs_difference(&dry_left, &latch_left) > rms(&dry_left).max(0.000_001) * 0.05,
        "note-latched excitation should materially change audio-created voice output"
    );
}

#[test]
fn note_latched_live_excitation_uses_preroll_for_midi_note_on() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::Off;
    patch.live_excitation.mode = LiveExcitationMode::NoteLatched;
    patch.live_excitation.gain_db = 6.0;
    patch.live_excitation.latch_window_ms = 2.0;
    patch.live_excitation.latch_pre_roll_ms = 10.0;
    patch.live_excitation.latch_fade_ms = 0.0;
    let mut without_preroll = ResonatorProcessor::with_builtin_excitation(48_000.0, patch.clone());
    let mut with_preroll = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let preroll = sidechain_sine_note(72.0, 0.8, 512);
    let zeros = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut scratch_left = vec![0.0; 512];
    let mut scratch_right = vec![0.0; 512];
    let mut no_pre_left = vec![0.0; 512];
    let mut no_pre_right = vec![0.0; 512];
    let mut pre_left = vec![0.0; 512];
    let mut pre_right = vec![0.0; 512];

    with_preroll.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&preroll),
        &mut scratch_left,
        &mut scratch_right,
    );
    without_preroll.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&zeros),
        &mut no_pre_left,
        &mut no_pre_right,
    );
    with_preroll.process_with_runtime_input(
        ResonatorRuntimeInput::new(&events).with_sidechain(&zeros),
        &mut pre_left,
        &mut pre_right,
    );

    assert_all_finite(&pre_left);
    assert_all_finite(&pre_right);
    assert!(
        mean_abs_difference(&no_pre_left, &pre_left) > rms(&no_pre_left).max(0.000_001) * 0.05,
        "pre-roll should be captured into the MIDI note latch"
    );
}

#[test]
fn note_latched_live_excitation_process_does_not_allocate() {
    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::Off;
    patch.live_excitation.mode = LiveExcitationMode::NoteLatched;
    patch.live_excitation.gain_db = 6.0;
    patch.live_excitation.latch_window_ms = 20.0;
    patch.live_excitation.latch_pre_roll_ms = 5.0;
    patch.live_excitation.latch_fade_ms = 1.0;
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(67.0, 0.8, 2_048);
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    assert_runtime_input_process_does_not_allocate(
        "processor note-latched live excitation",
        &mut processor,
        &events,
        &sidechain,
        &mut left,
        &mut right,
    );

    assert_all_finite(&left);
    assert_all_finite(&right);
}

#[test]
fn audio_note_creation_process_does_not_allocate() {
    let patch = audio_note_detection_patch(test_patch(), AudioInputMode::AudioCreatesNotes);
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(60.0, 0.8, 8_192);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    assert_runtime_input_process_does_not_allocate(
        "processor audio note creation",
        &mut processor,
        &[],
        &sidechain,
        &mut left,
        &mut right,
    );

    assert!(processor.audio_note_state.active.is_some());
}

#[test]
fn mixed_midi_audio_mode_process_does_not_allocate() {
    let patch = audio_note_detection_patch(test_patch(), AudioInputMode::MidiPlusAudioCreatesNotes);
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let sidechain = sidechain_sine_note(60.0, 0.8, 8_192);
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 2,
        note: 67,
        velocity: 0.75,
    })];
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    assert_runtime_input_process_does_not_allocate(
        "processor mixed MIDI/audio note creation",
        &mut processor,
        &events,
        &sidechain,
        &mut left,
        &mut right,
    );

    assert!(processor.audio_note_state.active.is_some());
    assert_eq!(processor.active_voice_count(), 2);
}

fn assert_live_control_path_does_not_allocate(
    label: &str,
    patch: ResonatorSynthPatch,
    control: ControlEvent,
    note_events: &[MidiEvent],
    left: &mut [f32],
    right: &mut [f32],
) {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    processor.process(note_events, left, right);
    assert_runtime_process_does_not_allocate(
        label,
        &mut processor,
        &[MidiEvent::Control(control)],
        left,
        right,
    );
}

#[test]
fn held_voice_consumes_expression_stream_updates_each_block() {
    let sample_rate = 48_000.0;
    let mut neutral_processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, expression_filter_patch());
    let mut pressed_processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, expression_filter_patch());
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 256];
    let mut warmup_right = vec![0.0; 256];
    neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    pressed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 4096];
    let mut neutral_right = vec![0.0; 4096];
    let mut pressed_left = vec![0.0; 4096];
    let mut pressed_right = vec![0.0; 4096];
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
        mean_abs_difference(&neutral_left, &pressed_left) > rms(&neutral_left) * 0.05,
        "neutral_rms={}, pressed_rms={}, diff={}",
        rms(&neutral_left),
        rms(&pressed_left),
        mean_abs_difference(&neutral_left, &pressed_left)
    );
}

#[test]
fn non_midi_expression_source_drives_pressure_and_brightness_without_midi_events() {
    let sample_rate = 48_000.0;
    let patch = external_expression_filter_patch();
    let mut neutral_processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, patch.clone());
    let mut driven_processor = ResonatorProcessor::with_builtin_excitation(sample_rate, patch);
    let mut neutral_source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
    let mut driven_source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
    let neutral_stream = ExpressionStream {
        velocity: 1.0,
        gate: true,
        ..ExpressionStream::default()
    };
    let driven_stream = ExpressionStream {
        pressure: 0.75,
        brightness: 0.85,
        ..neutral_stream
    };
    assert!(neutral_source.set_voice_stream(0, neutral_stream));
    assert!(driven_source.set_voice_stream(0, driven_stream));

    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process_with_expression_source(
        &mut neutral_source,
        &note_on,
        &mut warmup_left,
        &mut warmup_right,
    );
    driven_processor.process_with_expression_source(
        &mut driven_source,
        &note_on,
        &mut warmup_left,
        &mut warmup_right,
    );

    let mut neutral_left = vec![0.0; 4096];
    let mut neutral_right = vec![0.0; 4096];
    let mut driven_left = vec![0.0; 4096];
    let mut driven_right = vec![0.0; 4096];
    neutral_processor.process_with_expression_source(
        &mut neutral_source,
        &[],
        &mut neutral_left,
        &mut neutral_right,
    );
    driven_processor.process_with_expression_source(
        &mut driven_source,
        &[],
        &mut driven_left,
        &mut driven_right,
    );

    let neutral = expression_for_slot(&neutral_processor, 0, 60);
    let driven = expression_for_slot(&driven_processor, 0, 60);
    assert_eq!(neutral.stream.pressure, 0.0);
    assert_eq!(neutral.stream.brightness, 0.0);
    assert_eq!(driven.stream.pressure, 0.75);
    assert_eq!(driven.stream.brightness, 0.85);
    assert_all_finite(&neutral_left);
    assert_all_finite(&driven_left);
    assert!(
        mean_abs_difference(&neutral_left, &driven_left) > rms(&neutral_left) * 0.05,
        "neutral_rms={}, driven_rms={}, diff={}",
        rms(&neutral_left),
        rms(&driven_left),
        mean_abs_difference(&neutral_left, &driven_left)
    );
}

#[test]
fn audio_analysis_expression_source_drives_lamath_from_shared_phrase_result() {
    let sample_rate = 48_000.0;
    let pitch_hz = midi_note_to_hz(62.0);
    let audio = sidechain_sine_hz(pitch_hz, 0.3, 2_048);
    let analysis = audio_expression_phrase_result(pitch_hz, 0.2, audio.len());
    let mut source = crate::AudioAnalysisExpressionSource::<MIDI_EXPRESSION_VOICES>::new(
        &audio,
        sample_rate as u32,
        &analysis,
        crate::AudioExpressionMapping {
            pitch_bend_range_semitones: 12.0,
            pressure_floor_rms: 0.0,
            pressure_ceiling_rms: 0.4,
            brightness_floor_hz: 100.0,
            brightness_ceiling_hz: 8_000.0,
        },
    );
    source.set_block(0, 512);

    let mut processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, external_expression_filter_patch());
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 0.9,
    })];
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_expression_source(&mut source, &note_on, &mut left, &mut right);

    let expression = expression_for_slot(&processor, 0, 60);
    assert!(expression.stream.gate);
    assert!((expression.stream.pitch_bend - 2.0).abs() < 0.05);
    assert_eq!(expression.stream.velocity, 0.9);
    assert!(expression.stream.pressure > 0.4);
    assert!(expression.stream.brightness > 0.0);
    assert_all_finite(&left);
    assert_all_finite(&right);
}

fn audio_expression_phrase_result(pitch_hz: f32, rms: f32, len: usize) -> PhraseAnalysisResult {
    let note = DetectedNote {
        start_sample: 0,
        end_sample: len,
        pitch_hz,
        peak_rms: rms,
        mean_rms: rms,
    };
    PhraseAnalysisResult {
        pitch_contour: PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                audio_expression_pitch_frame(0, 0, pitch_hz, rms),
                audio_expression_pitch_frame(1, 768, pitch_hz, rms),
                audio_expression_pitch_frame(2, 1_536, pitch_hz, rms),
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

fn audio_expression_pitch_frame(
    frame_index: usize,
    source_sample_position: usize,
    pitch_hz: f32,
    rms: f32,
) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / 48_000.0,
        f0_hz: Some(pitch_hz),
        raw_f0_hz: pitch_hz,
        confidence: 0.95,
        voiced: true,
        rms,
    }
}
