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
