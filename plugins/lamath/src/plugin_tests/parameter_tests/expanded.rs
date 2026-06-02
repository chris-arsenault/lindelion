// Expanded full-surface parameter round-trip test and its assertion helpers.
// `include!`d into `plugin_tests/parameter_tests.rs` so it shares that test module's
// imports verbatim; split out only to keep `parameter_tests.rs` under the repository
// file-size limit.

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
        (156, 1.0),
        (157, 24.0),
        (158, 60.0),
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
    assert!(patch.shared_body.enabled);
    assert_eq!(patch.shared_body.damp_key_low, 24);
    assert_eq!(patch.shared_body.damp_key_high, 60);
}
