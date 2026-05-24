#[test]
fn repeated_clip_loop_note_offs_remain_bounded_and_decay_after_stop() {
    let sample_rate = 48_000.0;
    let block_size = 128;
    let setup = ProcessSetup {
        sample_rate,
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut start_tail = Vec::new();
    let mut end_tail = Vec::new();

    synth.reset(setup);
    for loop_index in 0..4 {
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 48 + loop_index,
                velocity: 100.0 / 127.0,
            })],
        );
        assert!(peak_abs(&block_left).max(peak_abs(&block_right)) < 8.0);

        for _ in 0..24 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
            assert_all_finite(&block_left);
            assert_all_finite(&block_right);
        }

        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &[MidiEvent::Note(NoteEvent::Off {
                channel: 0,
                note: 48 + loop_index,
                velocity: 0.0,
            })],
        );
    }

    for tail_block in 0..800 {
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        assert_all_finite(&block_left);
        assert_all_finite(&block_right);
        if tail_block < 64 {
            start_tail.extend_from_slice(&block_left);
        } else if tail_block >= 736 {
            end_tail.extend_from_slice(&block_left);
        }
    }

    assert!(
        rms(&end_tail) < rms(&start_tail) * 0.1,
        "output should decay after repeated note-offs"
    );
}

#[test]
fn audio_plugin_process_does_not_allocate() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 512,
        mode: ProcessMode::Realtime,
    };
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    synth.reset(setup);
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin process",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::empty(),
    );
}

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

#[test]
fn loaded_excitation_buffers_render_without_audio_thread_allocations() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 512,
        mode: ProcessMode::Realtime,
    };
    let mut patch = ResonatorSynthPatch {
        polyphony: 2,
        resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.97,
            loop_filter_cutoff: 14_000.0,
            ..WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            master_gain_db: -6.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    };
    patch.excitation_slots[0].gain_db = -3.0;
    patch.excitation_slots[0].pitch_track = true;
    let excitation = (0..96)
        .map(|index| {
            let phase = index as f32 / 96.0;
            (phase * std::f32::consts::TAU * 8.0).sin() * (1.0 - phase)
        })
        .collect::<Vec<_>>();
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    synth.reset(setup);
    synth.set_patch_with_loaded_excitations(
        patch,
        vec![LoadedExcitationBuffer::new(excitation, 48_000.0)],
    );
    assert_audio_plugin_process_block_does_not_allocate(
        "loaded excitation render",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::empty(),
    );

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(peak_abs(&left).max(peak_abs(&right)) > 0.000_001);
}

#[test]
fn patch_sample_references_load_from_sample_library_and_render() {
    let root = temp_root("sample-ref-load");
    let sample_path = root.join("strike.wav");
    write_test_wav(&sample_path, &[0.0, 0.7, -0.35, 0.18, 0.0]);
    let reference = SampleReference::new("sample-hash", sample_path.clone());
    let mut patch = waveguide_tail_test_patch();
    patch.excitation_slots[0].sample = Some(reference.clone());
    patch.excitation_slots[0].gain_db = -3.0;
    patch.excitation_slots[0].pitch_track = true;
    let library = StaticSampleLibrary {
        path: Some(sample_path),
    };
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 512,
        mode: ProcessMode::Realtime,
    };
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    synth.reset(setup);
    let report = synth
        .load_patch_from_sample_library(patch, &library)
        .unwrap();
    assert_eq!(report.loaded_slots, 1);
    assert!(report.missing_samples.is_empty());
    assert_audio_plugin_process_block_does_not_allocate(
        "resolved sample render",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        AudioInputBuffer::empty(),
    );

    assert_all_finite(&left);
    assert!(peak_abs(&left).max(peak_abs(&right)) > 0.000_001);
}

#[test]
fn patch_sample_reference_missing_reports_without_crashing() {
    let reference = SampleReference::new("missing-hash", "Samples/missing.wav");
    let mut patch = ResonatorSynthPatch::default();
    patch.excitation_slots[0].sample = Some(reference.clone());
    let mut synth = ResonatorSynth::default();
    let report = synth
        .load_patch_from_sample_library(patch, &StaticSampleLibrary { path: None })
        .unwrap();

    assert_eq!(report.loaded_slots, 0);
    assert_eq!(report.missing_samples, vec![reference]);
}

#[test]
fn state_load_resolves_absolute_sample_references_and_preserves_render() {
    let root = temp_root("sample-state-load");
    let sample_path = root.join("state-strike.wav");
    write_test_wav(&sample_path, &[0.0, 0.9, -0.45, 0.2, -0.1, 0.0]);
    let reference = SampleReference::new("state-hash", sample_path);
    let mut patch = waveguide_tail_test_patch();
    patch.excitation_slots[0].sample = Some(reference);
    patch.excitation_slots[0].gain_db = -3.0;
    patch.excitation_slots[0].pitch_track = true;
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 512,
        mode: ProcessMode::Realtime,
    };

    let mut source = ResonatorSynth::default();
    source.reset(setup);
    let report = source.load_patch_from_sample_paths(patch);
    assert_eq!(report.loaded_slots, 1);
    assert!(report.missing_samples.is_empty());
    let expected = render_one_block_after_state_load(&mut source, setup);
    let state = source.state();

    let mut restored = ResonatorSynth::default();
    restored.reset(setup);
    restored.load_state(state);
    let actual = render_one_block_after_state_load(&mut restored, setup);

    assert_all_finite(&actual);
    assert!(peak_abs(&actual) > 0.000_001);
    let max_diff = expected
        .iter()
        .copied()
        .zip(actual.iter().copied())
        .map(|(expected, actual)| (expected - actual).abs())
        .fold(0.0, f32::max);
    assert!(
        max_diff < 1.0e-6,
        "state load should restore sample-backed render, max_diff={max_diff}"
    );
}

fn render_one_block_after_state_load(synth: &mut ResonatorSynth, setup: ProcessSetup) -> Vec<f32> {
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    assert_audio_plugin_process_block_does_not_allocate(
        "state-loaded sample render",
        synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::empty(),
    );
    left
}

fn realtime_process_setup(block_size: usize) -> ProcessSetup {
    ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    }
}

#[allow(clippy::too_many_arguments)]
fn assert_audio_plugin_process_block_does_not_allocate(
    label: &str,
    synth: &mut ResonatorSynth,
    setup: ProcessSetup,
    left: &mut [f32],
    right: &mut [f32],
    events: &[MidiEvent],
    input: AudioInputBuffer<'_>,
) {
    assert_no_allocations(label, || {
        synth.process(
            ProcessContext::new(setup, AudioBuffer { left, right }, events).with_input(input),
        );
    });
    assert_all_finite(left);
    assert_all_finite(right);
}
