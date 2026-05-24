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
