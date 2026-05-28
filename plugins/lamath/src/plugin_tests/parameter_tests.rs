#[test]
fn parameter_state_roundtrip_preserves_exposed_audio_controls() {
    let mut synth = ResonatorSynth::default();
    set_parameter_plain(&mut synth, 1, -9.0);
    set_parameter_plain(&mut synth, 3, 1_200.0);
    set_parameter_plain(&mut synth, 4, 0.25);
    set_parameter_plain(&mut synth, 52, 0.42);
    set_parameter_plain(&mut synth, 55, 1.0);
    set_parameter_plain(&mut synth, 56, -0.5);
    set_parameter_plain(&mut synth, 100, 2.0);
    set_parameter_plain(&mut synth, 101, 1.0);
    set_parameter_plain(&mut synth, 102, 12.0);
    set_parameter_plain(&mut synth, 110, 0.75);
    set_parameter_plain(&mut synth, 120, 3.0);
    set_parameter_plain(&mut synth, 122, 180.0);

    let state = AudioPlugin::state(&synth);
    let mut restored = ResonatorSynth::default();
    AudioPlugin::load_state(&mut restored, state);

    assert!((restored.patch().output.master_gain_db + 9.0).abs() < 0.001);
    assert!((restored.patch().output.filter_cutoff - 1_200.0).abs() < 0.001);
    assert!((restored.patch().output.saturation_drive - 0.25).abs() < 0.001);
    assert_resonator_b_loop_gain(restored.patch(), 0.42);
    assert_resonator_b_waveguide_style(restored.patch(), WaveguideStyle::Tube);
    assert_resonator_b_boundary_reflection(restored.patch(), -0.5);
    assert_eq!(
        restored.patch().audio_input.mode,
        AudioInputMode::MidiPlusAudioCreatesNotes
    );
    assert!(restored.patch().audio_expression.enabled);
    assert!(
        (restored
            .patch()
            .audio_expression
            .mapping
            .pitch_bend_range_semitones
            - 12.0)
            .abs()
            < 0.001
    );
    assert!((restored.patch().note_detection.onset_sensitivity - 0.75).abs() < 0.001);
    assert_eq!(
        restored.patch().live_excitation.mode,
        LiveExcitationMode::ContinuousAndNoteLatched
    );
    assert!((restored.patch().live_excitation.latch_window_ms - 180.0).abs() < 0.001);
}

#[test]
fn exposes_complete_patch_parameter_surface() {
    let names = PARAMETERS
        .iter()
        .map(|parameter| parameter.name)
        .collect::<Vec<_>>();

    for expected in [
        "Master Gain",
        "Master Pan",
        "Filter Mode",
        "Filter Resonance",
        "Routing",
        "Resonator Mix",
        "Retrigger Resonators",
        "Resonator A Model",
        "Resonator A Modal Preset",
        "Resonator A Mode Count",
        "Resonator A Brightness",
        "Resonator A Loop Resonance",
        "Resonator A Loop Gain",
        "Resonator A Waveguide Style",
        "Resonator A Boundary Reflection",
        "Resonator B Model",
        "Resonator B Loop Filter",
        "Resonator B Loop Resonance",
        "Resonator B Loop Gain",
        "Resonator B Waveguide Style",
        "Resonator B Boundary Reflection",
        "Amp Attack",
        "Amp Release",
        "LFO Shape",
        "Audio Input Mode",
        "Audio Expression Enable",
        "Audio Expression Pitch Range",
        "Audio Expression Pressure Floor",
        "Audio Expression Pressure Ceiling",
        "Audio Expression Brightness Floor",
        "Audio Expression Brightness Ceiling",
        "Audio Note Onset Sensitivity",
        "Audio Note Release Floor",
        "Audio Note Minimum Length",
        "Audio Note Pitch Confidence",
        "Audio Note Velocity Amount",
        "Live Excitation Mode",
        "Live Excitation Gain",
        "Live Excitation Latch Window",
        "Live Excitation Latch Pre-roll",
        "Live Excitation Latch Fade",
        "Mod 1 Source",
        "Mod 4 Amount",
    ] {
        assert!(names.contains(&expected), "missing parameter {expected}");
    }

    assert!(
        !names.contains(&"Loop Gain"),
        "global Loop Gain should not be exposed"
    );
    assert!(
        PARAMETERS.len() >= 48,
        "parameter surface should cover the editable patch, got {}",
        PARAMETERS.len()
    );
}

#[test]
fn removed_global_loop_gain_parameter_is_ignored() {
    let mut patch = ResonatorSynthPatch::default();
    assert!(PARAMETERS.iter().all(|parameter| parameter.id.0 != 2));
    assert_eq!(patch_parameter_plain_value(&patch, 2), None);
    assert_eq!(
        apply_parameter_plain(&mut patch, 2, 0.1),
        ParameterApplyKind::Ignored
    );

    let mut synth = ResonatorSynth::default();
    synth.set_parameter_normalized(ParameterId(2), 0.0);
    assert_resonator_b_loop_gain(synth.patch(), 0.92);
}

#[test]
fn model_and_retrigger_parameters_are_explicit_binary_choices() {
    for id in [13, 20, 35, 40, 55] {
        let parameter = PARAMETERS
            .iter()
            .find(|parameter| parameter.id.0 == id)
            .expect("binary choice parameter should exist");
        assert_eq!(
            parameter.step_count,
            Some(1),
            "parameter {}",
            parameter.name
        );
        assert_eq!(parameter.range.min, 0.0, "parameter {}", parameter.name);
        assert_eq!(parameter.range.max, 1.0, "parameter {}", parameter.name);
    }
}

#[test]
fn routing_parameter_exposes_parallel_series_and_body_color_modes() {
    let parameter = PARAMETERS
        .iter()
        .find(|parameter| parameter.id.0 == 10)
        .expect("routing parameter should exist");

    assert_eq!(parameter.step_count, Some(2));
    assert_eq!(parameter.range.min, 0.0);
    assert_eq!(parameter.range.max, 2.0);
}

#[test]
fn modulation_source_parameters_cover_brightness_cc74() {
    let source_parameters = [81, 85, 89, 93].map(modulation_source_parameter_shape);
    assert_eq!(source_parameters, [(Some(5), 5.0); 4]);

    let sources = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0].map(ModulationSource::from_plain);
    assert_eq!(
        sources,
        [
            ModulationSource::SecondaryEnvelope,
            ModulationSource::Lfo,
            ModulationSource::Velocity,
            ModulationSource::Aftertouch,
            ModulationSource::ModWheel,
            ModulationSource::Brightness,
        ]
    );
    assert_eq!(ModulationSource::Brightness.plain(), 5.0);
    assert_eq!(ModulationSource::Aftertouch.label(), "Pressure");
    assert_eq!(ModulationSource::label_from_plain(5.0), "Brightness");
}

#[test]
fn modulation_destination_parameters_format_as_labels() {
    let destinations = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(ModulationDestination::from_plain);
    assert_eq!(
        destinations,
        [
            ModulationDestination::FilterCutoff,
            ModulationDestination::ResonatorADamping,
            ModulationDestination::ResonatorBDamping,
            ModulationDestination::ResonatorAPosition,
            ModulationDestination::ResonatorBPosition,
            ModulationDestination::ExcitationGain,
            ModulationDestination::LfoRate,
        ]
    );
    assert_eq!(ModulationDestination::ResonatorBPosition.plain(), 4.0);
    assert_eq!(
        ModulationDestination::ResonatorBPosition.label(),
        "Res B Position"
    );
    assert_eq!(ModulationDestination::label_from_plain(6.0), "LFO Rate");
}

fn modulation_source_parameter_shape(id: u32) -> (Option<u32>, f32) {
    let parameter = PARAMETERS
        .iter()
        .find(|parameter| parameter.id.0 == id)
        .expect("modulation source parameter should exist");
    (parameter.step_count, parameter.range.max)
}

#[test]
#[allow(clippy::cognitive_complexity)]
fn structural_parameters_have_explicit_apply_policies() {
    let mut patch = ResonatorSynthPatch::default();

    assert_eq!(
        apply_parameter_plain(&mut patch, 7, 1.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::LiveMuteRamp)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 10, 1.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::LiveMuteRamp)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 13, 1.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 20, 1.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 35, 1.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 100, 2.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 110, 0.75),
        ParameterApplyKind::Structural(StructuralChangePolicy::ResetState)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 112, 90.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::ResetState)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 120, 3.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::NoteBoundary)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 122, 180.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::ResetState)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 123, 30.0),
        ParameterApplyKind::Structural(StructuralChangePolicy::ResetState)
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 11, 0.25),
        ParameterApplyKind::Live
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, RESONATOR_MIX_PARAMETER_ID, 0.5),
        ParameterApplyKind::Live
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 101, 1.0),
        ParameterApplyKind::Live
    );
    assert_eq!(
        apply_parameter_plain(&mut patch, 121, -6.0),
        ParameterApplyKind::Live
    );
}

#[test]
fn waveguide_style_parameters_are_per_slot_controls() {
    let mut synth = ResonatorSynth::default();

    set_parameter_plain(&mut synth, 20, 1.0);
    set_parameter_plain(&mut synth, 35, 1.0);
    set_parameter_plain(&mut synth, 36, -0.65);
    set_parameter_plain(&mut synth, 55, 1.0);
    set_parameter_plain(&mut synth, 56, 0.4);

    assert_resonator_a_waveguide_style(synth.patch(), WaveguideStyle::Tube);
    assert_resonator_a_boundary_reflection(synth.patch(), -0.65);
    assert_resonator_b_waveguide_style(synth.patch(), WaveguideStyle::Tube);
    assert_resonator_b_boundary_reflection(synth.patch(), 0.4);
}

#[test]
fn only_model_selector_changes_selected_resonator_model() {
    let mut synth = ResonatorSynth::default();

    assert_resonator_model(synth.patch().resonator_a, 0);
    assert_resonator_model(synth.patch().resonator_b, 1);

    set_parameter_plain(&mut synth, 32, 0.25);
    set_parameter_plain(&mut synth, 46, 0.95);

    assert_resonator_model(synth.patch().resonator_a, 0);
    assert_resonator_model(synth.patch().resonator_b, 1);

    set_parameter_plain(&mut synth, 20, 1.0);
    set_parameter_plain(&mut synth, 40, 0.0);

    assert_resonator_model(synth.patch().resonator_a, 1);
    assert_resonator_model(synth.patch().resonator_b, 0);
}

#[test]
fn routing_switch_preserves_parallel_mix_values() {
    let mut synth = ResonatorSynth::default();

    set_parameter_plain(&mut synth, 11, 0.8);
    set_parameter_plain(&mut synth, 12, 0.2);
    assert_parallel_mix(synth.patch().routing, 0.8, 0.2);

    set_parameter_plain(&mut synth, 10, 1.0);
    assert_series_mix(synth.patch().routing, 0.8, 0.2);

    set_parameter_plain(&mut synth, 10, 2.0);
    assert_body_color_mix(synth.patch().routing, 0.8, 0.2);

    set_parameter_plain(&mut synth, 11, 0.25);
    assert_body_color_mix(synth.patch().routing, 0.25, 0.2);

    set_parameter_plain(&mut synth, 10, 0.0);
    assert_parallel_mix(synth.patch().routing, 0.25, 0.2);
}

#[test]
fn resonator_mix_balance_sets_parallel_mix_values() {
    let mut synth = ResonatorSynth::default();
    assert_parallel_mix(synth.patch().routing, 1.0, 0.0);

    set_parameter_plain(&mut synth, RESONATOR_MIX_PARAMETER_ID, 0.5);
    assert_parallel_mix(synth.patch().routing, 0.5, 0.5);

    set_parameter_plain(&mut synth, RESONATOR_MIX_PARAMETER_ID, 1.0);
    assert_parallel_mix(synth.patch().routing, 0.0, 1.0);
}

#[test]
fn modal_modal_series_selection_uses_body_color() {
    let mut synth = ResonatorSynth::default();

    set_parameter_plain(&mut synth, 11, 0.7);
    set_parameter_plain(&mut synth, 12, 0.3);
    set_parameter_plain(&mut synth, 40, 0.0);
    set_parameter_plain(&mut synth, 10, 1.0);

    assert_body_color_mix(synth.patch().routing, 0.7, 0.3);
    assert_eq!(
        patch_parameter_plain_value(synth.patch(), 10),
        Some(2.0),
        "modal-modal series should canonicalize to the body-color routing value",
    );
}

#[test]
fn modal_modal_existing_series_becomes_body_color_when_model_changes() {
    let mut synth = ResonatorSynth::default();

    set_parameter_plain(&mut synth, 11, 0.6);
    set_parameter_plain(&mut synth, 12, 0.4);
    set_parameter_plain(&mut synth, 10, 1.0);
    assert_series_mix(synth.patch().routing, 0.6, 0.4);

    set_parameter_plain(&mut synth, 40, 0.0);

    assert_body_color_mix(synth.patch().routing, 0.6, 0.4);
}

#[test]
fn non_modal_modal_pairs_keep_raw_series_available() {
    let mut modal_waveguide = ResonatorSynth::default();
    set_parameter_plain(&mut modal_waveguide, 11, 0.9);
    set_parameter_plain(&mut modal_waveguide, 12, 0.1);
    set_parameter_plain(&mut modal_waveguide, 10, 1.0);
    assert_series_mix(modal_waveguide.patch().routing, 0.9, 0.1);

    let mut waveguide_modal = ResonatorSynth::default();
    set_parameter_plain(&mut waveguide_modal, 11, 0.5);
    set_parameter_plain(&mut waveguide_modal, 12, 0.5);
    set_parameter_plain(&mut waveguide_modal, 20, 1.0);
    set_parameter_plain(&mut waveguide_modal, 40, 0.0);
    set_parameter_plain(&mut waveguide_modal, 10, 1.0);
    assert_series_mix(waveguide_modal.patch().routing, 0.5, 0.5);

    let mut waveguide_waveguide = ResonatorSynth::default();
    set_parameter_plain(&mut waveguide_waveguide, 11, 0.5);
    set_parameter_plain(&mut waveguide_waveguide, 12, 0.5);
    set_parameter_plain(&mut waveguide_waveguide, 20, 1.0);
    set_parameter_plain(&mut waveguide_waveguide, 10, 1.0);
    assert_series_mix(waveguide_waveguide.patch().routing, 0.5, 0.5);
}

fn assert_resonator_model(config: ResonatorConfig, expected: u8) {
    assert_eq!(resonator_model_index(config), expected);
}

fn resonator_model_index(config: ResonatorConfig) -> u8 {
    match config {
        ResonatorConfig::Modal(_) => 0,
        ResonatorConfig::Waveguide(_) => 1,
    }
}

fn assert_parallel_mix(routing: ResonatorRouting, expected_a: f32, expected_b: f32) {
    let ResonatorRouting::Parallel { mix_a, mix_b } = routing else {
        panic!("expected parallel routing, got {routing:?}");
    };
    assert_mix_values(mix_a, mix_b, expected_a, expected_b);
}

fn assert_series_mix(routing: ResonatorRouting, expected_a: f32, expected_b: f32) {
    let ResonatorRouting::Series { mix_a, mix_b } = routing else {
        panic!("expected series routing, got {routing:?}");
    };
    assert_mix_values(mix_a, mix_b, expected_a, expected_b);
}

fn assert_body_color_mix(routing: ResonatorRouting, expected_a: f32, expected_b: f32) {
    let ResonatorRouting::BodyColor { mix_a, mix_b } = routing else {
        panic!("expected body-color routing, got {routing:?}");
    };
    assert_mix_values(mix_a, mix_b, expected_a, expected_b);
}

fn assert_mix_values(mix_a: f32, mix_b: f32, expected_a: f32, expected_b: f32) {
    assert!((mix_a - expected_a).abs() < 0.001, "mix_a={mix_a}");
    assert!((mix_b - expected_b).abs() < 0.001, "mix_b={mix_b}");
}

#[test]
fn expanded_parameter_updates_mutate_patch_and_roundtrip() {
    let patch = roundtrip_patch_after_parameter_updates();

    assert_expanded_output_and_routing(&patch);
    assert_expanded_resonator_parameters(&patch);
    assert_expanded_modulation_parameters(&patch);
    assert_expanded_v2_parameters(&patch);
}

fn roundtrip_patch_after_parameter_updates() -> ResonatorSynthPatch {
    let mut synth = ResonatorSynth::default();

    for (id, plain) in [
        (5, -0.5),
        (6, 0.35),
        (7, 2.0),
        (10, 1.0),
        (13, 1.0),
        (20, 1.0),
        (32, 0.975),
        (40, 0.0),
        (41, 4.0),
        (46, 0.8),
        (60, 12.0),
        (63, 480.0),
        (68, 7.5),
        (69, 3.0),
        (80, 1.0),
        (81, 5.0),
        (82, 3.0),
        (83, -0.33),
        (100, 2.0),
        (101, 1.0),
        (102, 12.0),
        (103, 0.04),
        (104, 0.5),
        (105, 900.0),
        (106, 9_000.0),
        (110, 0.75),
        (111, 0.03),
        (112, 90.0),
        (113, 0.8),
        (114, 0.6),
        (120, 3.0),
        (121, -6.0),
        (122, 180.0),
        (123, 30.0),
        (124, 10.0),
    ] {
        set_parameter_plain(&mut synth, id, plain);
    }

    let state = AudioPlugin::state(&synth);
    let mut restored = ResonatorSynth::default();
    AudioPlugin::load_state(&mut restored, state);

    restored.patch().clone()
}

fn assert_expanded_output_and_routing(patch: &ResonatorSynthPatch) {
    assert_eq!(patch.output.filter_mode, FilterMode::HighPass);
    assert!((patch.output.master_pan + 0.5).abs() < 0.001);
    assert!((patch.output.filter_resonance - 0.35).abs() < 0.001);
    assert!(matches!(patch.routing, ResonatorRouting::Series { .. }));
    assert!(patch.retrigger_resonators);
}

fn assert_expanded_resonator_parameters(patch: &ResonatorSynthPatch) {
    assert!(matches!(
        patch.resonator_a,
        ResonatorConfig::Waveguide(WaveguideConfig { loop_gain, .. })
            if (loop_gain - 0.975).abs() < 0.001
    ));
    assert!(matches!(
        patch.resonator_b,
        ResonatorConfig::Modal(ModalConfig {
            preset: ModalPreset::MetalBar,
            brightness,
            ..
        }) if (brightness - 0.8).abs() < 0.001
    ));
}

fn assert_expanded_modulation_parameters(patch: &ResonatorSynthPatch) {
    assert!((patch.modulation.amp_envelope.attack_ms - 12.0).abs() < 0.001);
    assert!((patch.modulation.amp_envelope.release_ms - 480.0).abs() < 0.001);
    assert!((patch.modulation.lfo.rate_hz - 7.5).abs() < 0.001);
    assert_eq!(patch.modulation.lfo.shape, LfoShape::Square);
    assert!(patch.modulation.slots[0].enabled);
    assert_eq!(
        patch.modulation.slots[0].source,
        ModulationSource::Brightness
    );
    assert_eq!(
        patch.modulation.slots[0].destination,
        ModulationDestination::ResonatorAPosition
    );
    assert!((patch.modulation.slots[0].amount + 0.33).abs() < 0.001);
}

#[allow(clippy::cognitive_complexity)]
fn assert_expanded_v2_parameters(patch: &ResonatorSynthPatch) {
    assert_eq!(
        patch.audio_input.mode,
        AudioInputMode::MidiPlusAudioCreatesNotes
    );
    assert!(patch.audio_expression.enabled);
    assert!(
        (patch
            .audio_expression
            .mapping
            .pitch_bend_range_semitones
            - 12.0)
            .abs()
            < 0.001
    );
    assert!((patch.audio_expression.mapping.pressure_floor_rms - 0.04).abs() < 0.001);
    assert!((patch.audio_expression.mapping.pressure_ceiling_rms - 0.5).abs() < 0.001);
    assert!((patch.audio_expression.mapping.brightness_floor_hz - 900.0).abs() < 0.001);
    assert!((patch.audio_expression.mapping.brightness_ceiling_hz - 9_000.0).abs() < 0.001);
    assert!((patch.note_detection.onset_sensitivity - 0.75).abs() < 0.001);
    assert!((patch.note_detection.note_release_floor_rms - 0.03).abs() < 0.001);
    assert!((patch.note_detection.minimum_note_length_ms - 90.0).abs() < 0.001);
    assert!((patch.note_detection.pitch_confidence - 0.8).abs() < 0.001);
    assert!((patch.note_detection.velocity_amount - 0.6).abs() < 0.001);
    assert_eq!(
        patch.live_excitation.mode,
        LiveExcitationMode::ContinuousAndNoteLatched
    );
    assert!((patch.live_excitation.gain_db + 6.0).abs() < 0.001);
    assert!((patch.live_excitation.latch_window_ms - 180.0).abs() < 0.001);
    assert!((patch.live_excitation.latch_pre_roll_ms - 30.0).abs() < 0.001);
    assert!((patch.live_excitation.latch_fade_ms - 10.0).abs() < 0.001);
}
