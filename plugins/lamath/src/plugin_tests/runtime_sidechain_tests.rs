#[test]
fn audio_plugin_projects_sidechain_input_to_preallocated_mono_scratch_without_allocating() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 4,
        mode: ProcessMode::Realtime,
    };
    let sidechain_left = [0.5, f32::NAN, -0.5];
    let sidechain_right = [0.0, 0.5, 0.25];
    let mut left = [1.0; 4];
    let mut right = [-1.0; 4];

    synth.reset(setup);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin sidechain projection",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[],
        AudioInputBuffer::stereo(&sidechain_left, &sidechain_right),
    );

    assert_eq!(
        synth.sidechain_input_for_test(),
        &[0.25, 0.0, -0.125, 0.0]
    );
    let sidechain = synth.telemetry().sidechain;
    assert!(!sidechain.required);
    assert!(sidechain.input_detected);
    assert!(sidechain.signal_active);
    assert!(!sidechain.note_detected);
    assert_all_finite(&left);
    assert_all_finite(&right);
    assert_eq!(left, [0.0; 4]);
    assert_eq!(right, [0.0; 4]);
}

#[test]
fn audio_plugin_telemetry_reports_missing_and_inactive_required_sidechain() {
    let block_size = 128;
    let setup = realtime_process_setup(block_size);
    let mut synth = ResonatorSynth::default();
    let mut patch = waveguide_tail_test_patch();
    patch.live_excitation.mode = LiveExcitationMode::Continuous;
    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];
    let silent_sidechain = vec![0.0; block_size];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    process_block(&mut synth, setup, &mut left, &mut right, &[]);
    let missing = synth.telemetry().sidechain;
    assert!(missing.required);
    assert!(!missing.input_detected);
    assert!(!missing.signal_active);

    synth.process(
        ProcessContext::new(setup, AudioBuffer { left: &mut left, right: &mut right }, &[])
            .with_input(AudioInputBuffer::mono(&silent_sidechain)),
    );
    let inactive = synth.telemetry().sidechain;
    assert!(inactive.required);
    assert!(inactive.input_detected);
    assert!(!inactive.signal_active);
}

#[test]
fn audio_plugin_audio_note_creation_does_not_allocate() {
    let block_size = 8_192;
    let setup = realtime_process_setup(block_size);
    let mut synth = ResonatorSynth::default();
    let mut patch = audio_note_detection_patch(waveguide_tail_test_patch(), AudioInputMode::AudioCreatesNotes);
    patch.polyphony = 4;
    patch.live_excitation.mode = LiveExcitationMode::Off;
    let sidechain = sidechain_sine_note(60.0, 0.8, block_size);
    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin audio note creation",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[],
        AudioInputBuffer::mono(&sidechain),
    );

    let telemetry = synth.telemetry();
    assert_eq!(telemetry.active_voices, 1);
    assert!(telemetry.sidechain.required);
    assert!(telemetry.sidechain.input_detected);
    assert!(telemetry.sidechain.signal_active);
    assert!(telemetry.sidechain.note_detected);
    assert!(telemetry.sidechain.pitch_confidence > 0.0);
}

#[test]
fn audio_plugin_continuous_live_excitation_does_not_allocate() {
    let block_size = 512;
    let setup = realtime_process_setup(block_size);
    let mut synth = ResonatorSynth::default();
    let mut patch = waveguide_tail_test_patch();
    patch.live_excitation.mode = LiveExcitationMode::Continuous;
    patch.live_excitation.gain_db = 6.0;
    let sidechain = sidechain_sine_note(67.0, 0.8, block_size);
    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    process_block(&mut synth, setup, &mut left, &mut right, &events);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin continuous live excitation",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[],
        AudioInputBuffer::mono(&sidechain),
    );

    let telemetry = synth.telemetry();
    assert_eq!(telemetry.active_voices, 1);
    assert!(telemetry.sidechain.required);
    assert!(telemetry.sidechain.input_detected);
    assert!(telemetry.sidechain.signal_active);
}

#[test]
fn audio_plugin_note_latched_live_excitation_does_not_allocate() {
    let block_size = 512;
    let setup = realtime_process_setup(block_size);
    let mut synth = ResonatorSynth::default();
    let mut patch = waveguide_tail_test_patch();
    patch.live_excitation.mode = LiveExcitationMode::NoteLatched;
    patch.live_excitation.gain_db = 6.0;
    patch.live_excitation.latch_window_ms = 20.0;
    patch.live_excitation.latch_pre_roll_ms = 5.0;
    patch.live_excitation.latch_fade_ms = 1.0;
    let preroll = sidechain_sine_note(72.0, 0.8, block_size);
    let zeros = vec![0.0; block_size];
    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin note-latched pre-roll fill",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[],
        AudioInputBuffer::mono(&preroll),
    );
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin note-latched trigger",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::mono(&zeros),
    );

    assert_eq!(synth.telemetry().active_voices, 1);
}

#[test]
fn audio_plugin_mixed_midi_audio_mode_does_not_allocate() {
    let block_size = 8_192;
    let setup = realtime_process_setup(block_size);
    let mut synth = ResonatorSynth::default();
    let mut patch =
        audio_note_detection_patch(waveguide_tail_test_patch(), AudioInputMode::MidiPlusAudioCreatesNotes);
    patch.polyphony = 4;
    let sidechain = sidechain_sine_note(60.0, 0.8, block_size);
    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 2,
        note: 67,
        velocity: 0.75,
    })];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin mixed MIDI/audio note creation",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::mono(&sidechain),
    );

    assert_eq!(synth.telemetry().active_voices, 2);
}

#[test]
fn audio_plugin_all_enabled_v2_mode_does_not_allocate() {
    let block_size = 8_192;
    let setup = realtime_process_setup(block_size);
    let mut synth = ResonatorSynth::default();
    let mut patch =
        audio_note_detection_patch(waveguide_tail_test_patch(), AudioInputMode::MidiPlusAudioCreatesNotes);
    patch.polyphony = 4;
    patch.audio_expression.enabled = true;
    patch.audio_expression.mapping.pressure_floor_rms = 0.0;
    patch.audio_expression.mapping.pressure_ceiling_rms = 0.5;
    patch.live_excitation.mode = LiveExcitationMode::ContinuousAndNoteLatched;
    patch.live_excitation.gain_db = -6.0;
    patch.live_excitation.latch_window_ms = 20.0;
    patch.live_excitation.latch_pre_roll_ms = 5.0;
    patch.live_excitation.latch_fade_ms = 1.0;
    let sidechain = sidechain_sine_note(60.0, 0.8, block_size);
    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 2,
        note: 67,
        velocity: 0.75,
    })];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin all enabled v2 mode",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::mono(&sidechain),
    );

    let telemetry = synth.telemetry();
    assert_eq!(telemetry.active_voices, 2);
    assert!(telemetry.sidechain.required);
    assert!(telemetry.sidechain.signal_active);
    assert!(telemetry.sidechain.note_detected);
}

#[test]
fn empty_sidechain_input_preserves_midi_only_render() {
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 128,
        mode: ProcessMode::Realtime,
    };
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut dry_synth = ResonatorSynth::default();
    let mut empty_input_synth = ResonatorSynth::default();
    let mut dry_left = [0.0; 128];
    let mut dry_right = [0.0; 128];
    let mut empty_left = [0.0; 128];
    let mut empty_right = [0.0; 128];

    dry_synth.reset(setup);
    empty_input_synth.reset(setup);
    dry_synth.process(ProcessContext::new(
        setup,
        AudioBuffer {
            left: &mut dry_left,
            right: &mut dry_right,
        },
        &events,
    ));
    empty_input_synth.process(
        ProcessContext::new(
            setup,
            AudioBuffer {
                left: &mut empty_left,
                right: &mut empty_right,
            },
            &events,
        )
        .with_input(AudioInputBuffer::empty()),
    );

    assert_eq!(empty_input_synth.sidechain_input_for_test(), &[] as &[f32]);
    assert_eq!(empty_left, dry_left);
    assert_eq!(empty_right, dry_right);
}
