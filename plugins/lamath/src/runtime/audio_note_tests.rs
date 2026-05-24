#[test]
#[allow(clippy::cognitive_complexity)]
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
fn sidechain_note_on_latency_is_bounded_with_small_host_blocks() {
    const BLOCK_SIZE: usize = 128;
    const SILENCE_BLOCKS: usize = 24;
    const TONE_BLOCKS: usize = 24;
    const ONSET_SAMPLE: usize = BLOCK_SIZE * SILENCE_BLOCKS;
    const MAX_LATENCY_SAMPLES: usize = 512;

    let mut patch = test_patch();
    patch.audio_input.mode = AudioInputMode::AudioCreatesNotes;
    let mut processor = ResonatorProcessor::with_builtin_excitation_and_realtime_capacity(
        48_000.0,
        patch,
        BLOCK_SIZE,
    );
    let sidechain =
        sidechain_sine_note_after_silence(84.0, 0.8, ONSET_SAMPLE, BLOCK_SIZE * TONE_BLOCKS);
    let mut rendered = Vec::with_capacity(sidechain.len());
    let mut left = vec![0.0; BLOCK_SIZE];
    let mut right = vec![0.0; BLOCK_SIZE];

    for block in sidechain.chunks_exact(BLOCK_SIZE) {
        processor.process_with_runtime_input(
            ResonatorRuntimeInput::new(&[]).with_sidechain(block),
            &mut left,
            &mut right,
        );
        rendered.extend_from_slice(&left);
    }

    let latency_samples = first_sample_above(&rendered[ONSET_SAMPLE..], 0.000_001)
        .expect("audio-created note should become audible after sidechain onset");
    assert!(
        latency_samples <= MAX_LATENCY_SAMPLES,
        "sidechain note-on latency should stay within {MAX_LATENCY_SAMPLES} samples, got {latency_samples}"
    );
    assert!(processor.audio_note_state.active.is_some());
    assert_all_finite(&rendered);
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
