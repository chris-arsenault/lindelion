#[test]
fn controller_patch_mirror_tracks_parameter_edits() {
    let controller = ResonatorVst3Controller::new();
    let normalized = normalized_parameter_value(1, -12.0);

    assert_eq!(controller.set_value(1, normalized), kResultOk);

    assert!((controller.patch.borrow().output.master_gain_db + 12.0).abs() < 1.0e-5);
    assert_eq!(controller.editor_summary.borrow().patch_name, "Default");
}

#[test]
fn controller_roundtrips_v2_patch_surface() {
    let controller = ResonatorVst3Controller::new();

    for (id, plain) in [
        (100, 2.0),
        (101, 1.0),
        (102, 12.0),
        (110, 0.75),
        (120, 3.0),
        (121, -6.0),
        (122, 180.0),
    ] {
        assert_eq!(
            controller.set_value(id, normalized_parameter_value(id, plain)),
            kResultOk,
            "parameter {id}"
        );
    }

    {
        let patch = controller.patch.borrow();
        assert_eq!(
            patch.audio_input.mode,
            crate::AudioInputMode::MidiPlusAudioCreatesNotes
        );
        assert!(patch.audio_expression.enabled);
        assert!((patch.audio_expression.mapping.pitch_bend_range_semitones - 12.0).abs() < 0.001);
        assert!((patch.note_detection.onset_sensitivity - 0.75).abs() < 0.001);
        assert_eq!(
            patch.live_excitation.mode,
            crate::LiveExcitationMode::ContinuousAndNoteLatched
        );
        assert!((patch.live_excitation.gain_db + 6.0).abs() < 0.001);
        assert!((patch.live_excitation.latch_window_ms - 180.0).abs() < 0.001);
    }

    let patch = controller.patch.borrow();
    let values = parameter_values_from_patch(&patch);
    assert_parameter_value(&values, 100, 2.0);
    assert_parameter_value(&values, 101, 1.0);
    assert_parameter_value(&values, 102, 12.0);
    assert_parameter_value(&values, 110, 0.75);
    assert_parameter_value(&values, 120, 3.0);
    assert_parameter_value(&values, 121, -6.0);
    assert_parameter_value(&values, 122, 180.0);
}

#[test]
fn controller_roundtrips_expression_slot_choices() {
    let controller = ResonatorVst3Controller::new();

    assert_eq!(
        controller.set_value(81, normalized_parameter_value(81, 4.0)),
        kResultOk
    );
    assert_eq!(
        controller.set_value(82, normalized_parameter_value(82, 5.0)),
        kResultOk
    );

    {
        let patch = controller.patch.borrow();
        assert_eq!(
            patch.modulation.slots[0].source,
            crate::ModulationSource::ModWheel
        );
        assert_eq!(
            patch.modulation.slots[0].destination,
            crate::ModulationDestination::ExcitationGain
        );
    }

    let patch = controller.patch.borrow();
    let values = parameter_values_from_patch(&patch);
    assert_parameter_value(&values, 81, 4.0);
    assert_parameter_value(&values, 82, 5.0);
}

#[test]
fn controller_slot_assignment_updates_patch_and_summary_before_processor_bridge() {
    let controller = ResonatorVst3Controller::new();
    let reference = SampleReference::new("sample-hash", "Samples/kick.wav");

    let result = controller.assign_sample_reference_to_slot(reference.clone(), 2);

    assert_eq!(result, kResultFalse);
    assert_eq!(
        controller.patch.borrow().excitation_slots[2].sample,
        Some(reference)
    );
    assert_eq!(
        controller.editor_summary.borrow().slots[2].detail,
        "kick.wav"
    );
}

#[test]
fn processor_notify_applies_patch_payload() {
    let processor = ResonatorVst3Processor::new();
    let patch = ResonatorSynthPatch {
        name: "Bridge Patch".to_string(),
        ..ResonatorSynthPatch::default()
    };
    let payload = patch_io::to_toml_string(&patch).unwrap().into_bytes();
    let message = ResonatorPluginMessage::patch_update(payload)
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert_eq!(processor.synth.borrow().patch().name, "Bridge Patch");
}

#[test]
fn component_state_projection_covers_expanded_parameter_surface() {
    let mut patch = crate::ResonatorSynthPatch {
        output: crate::OutputConfig {
            filter_mode: crate::FilterMode::HighPass,
            filter_resonance: 0.4,
            master_pan: -0.25,
            ..crate::OutputConfig::default()
        },
        routing: crate::ResonatorRouting::Series {
            mix_a: 0.5,
            mix_b: 0.5,
        },
        resonator_a: crate::ResonatorConfig::Waveguide(crate::WaveguideConfig {
            style: crate::WaveguideStyle::Tube,
            loop_gain: 0.96,
            boundary_reflection: -0.4,
            ..crate::WaveguideConfig::default()
        }),
        resonator_b: crate::ResonatorConfig::Modal(crate::ModalConfig {
            preset: crate::ModalPreset::MetalBar,
            brightness: 0.75,
            ..crate::ModalConfig::default()
        }),
        ..crate::ResonatorSynthPatch::default()
    };
    patch.modulation.lfo.shape = crate::LfoShape::Square;
    patch.modulation.slots[0].source = crate::ModulationSource::Brightness;
    patch.modulation.slots[0].destination = crate::ModulationDestination::ResonatorBPosition;
    patch.audio_input.mode = crate::AudioInputMode::MidiPlusAudioCreatesNotes;
    patch.audio_expression.enabled = true;
    patch.audio_expression.mapping.pitch_bend_range_semitones = 12.0;
    patch.note_detection.onset_sensitivity = 0.75;
    patch.live_excitation.mode = crate::LiveExcitationMode::ContinuousAndNoteLatched;
    patch.live_excitation.latch_window_ms = 180.0;

    let values = parameter_values_from_patch(&patch);

    assert_parameter_value(&values, 5, -0.25);
    assert_parameter_value(&values, 7, 2.0);
    assert_parameter_value(&values, 10, 1.0);
    assert_parameter_value(&values, 20, 1.0);
    assert_parameter_value(&values, 32, 0.96);
    assert_parameter_value(&values, 35, 1.0);
    assert_parameter_value(&values, 36, -0.4);
    assert_parameter_value(&values, 41, 4.0);
    assert_parameter_value(&values, 46, 0.75);
    assert_parameter_value(&values, 69, 3.0);
    assert_parameter_value(&values, 81, 5.0);
    assert_parameter_value(&values, 82, 4.0);
    assert_parameter_value(&values, 100, 2.0);
    assert_parameter_value(&values, 101, 1.0);
    assert_parameter_value(&values, 102, 12.0);
    assert_parameter_value(&values, 110, 0.75);
    assert_parameter_value(&values, 120, 3.0);
    assert_parameter_value(&values, 122, 180.0);
}

#[test]
fn editor_patch_summary_reflects_excitation_samples() {
    let mut patch = crate::ResonatorSynthPatch {
        name: "Sample Patch".to_string(),
        ..crate::ResonatorSynthPatch::default()
    };
    patch.excitation_slots[0].sample = Some(lindelion_sample_library::SampleReference::new(
        "hash",
        "Samples/strikes/metal.wav",
    ));
    patch.excitation_slots[0].pitch_track = true;
    patch.excitation_slots[0].looping = true;

    let summary = EditorPatchSummary::from_patch(&patch);

    assert_eq!(summary.patch_name, "Sample Patch");
    assert_eq!(summary.slots[0].detail, "metal.wav");
    assert!(summary.slots[0].sample_backed);
    assert!(summary.slots[0].pitch_track);
    assert!(summary.slots[0].looping);
    assert_eq!(summary.slots[1].detail, "Empty layer");
}
