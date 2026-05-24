use super::*;
use crate::assert_no_allocations;
use lindelion_plugin_shell::ParameterId;

#[test]
fn exposed_parameters_have_exactly_one_binding() {
    assert_eq!(PARAMETERS.len(), PARAMETER_BINDINGS.len());

    for (index, parameter) in PARAMETERS.iter().enumerate() {
        let binding = parameter_binding(parameter.id.0).expect("missing binding");
        assert_eq!(binding.info(), *parameter);
        assert_eq!(parameter_binding_index(parameter.id.0), Some(index));
    }

    for (left, left_binding) in PARAMETER_BINDINGS.iter().enumerate() {
        for right_binding in PARAMETER_BINDINGS.iter().skip(left + 1) {
            assert_ne!(
                left_binding.id(),
                right_binding.id(),
                "duplicate parameter id {}",
                left_binding.id().0
            );
        }
    }
}

#[test]
fn every_binding_round_trips_patch_get_set() {
    for binding in PARAMETER_BINDINGS {
        let mut patch = ResonatorSynthPatch::default();
        prepare_patch_for_binding(&mut patch, *binding);

        let value = non_default_probe_value(binding.info().range);
        binding.apply_plain(&mut patch, value);

        let actual = binding.plain_value(&patch);
        assert!(
            (actual - value).abs() < 0.001,
            "parameter {} ({}) round-tripped as {actual}, expected {value}",
            binding.id().0,
            binding.info().name
        );
    }
}

#[test]
fn formatters_are_owned_by_bindings() {
    assert_eq!(parameter_binding(7).unwrap().format_plain_value(2.0), "HP");
    assert_eq!(
        parameter_binding(10).unwrap().format_plain_value(1.0),
        "Series"
    );
    assert_eq!(
        parameter_binding(13).unwrap().format_plain_value(1.0),
        "Retrigger"
    );
    assert_eq!(
        parameter_binding(100).unwrap().format_plain_value(2.0),
        "MIDI + Audio"
    );
    assert_eq!(
        parameter_binding(120).unwrap().format_plain_value(3.0),
        "Cont + Latch"
    );
    assert_eq!(
        parameter_binding(20).unwrap().format_plain_value(1.0),
        "Waveguide"
    );
    assert_eq!(
        parameter_binding(35).unwrap().format_plain_value(1.0),
        "Tube"
    );
    assert_eq!(
        parameter_binding(81).unwrap().format_plain_value(5.0),
        "Brightness"
    );
    assert_eq!(
        parameter_binding(82).unwrap().format_plain_value(4.0),
        "Res B Position"
    );
}

#[test]
fn live_smoothed_parameters_are_declared_in_registry() {
    for id in [
        MASTER_GAIN_PARAMETER_ID,
        FILTER_CUTOFF_PARAMETER_ID,
        SATURATION_PARAMETER_ID,
        MASTER_PAN_PARAMETER_ID,
        FILTER_RESONANCE_PARAMETER_ID,
        PARALLEL_MIX_A_PARAMETER_ID,
        PARALLEL_MIX_B_PARAMETER_ID,
    ] {
        let binding = parameter_binding(id).expect("missing parameter binding");
        assert!(
            binding.smoothed_atomic_spec().is_some(),
            "parameter {} ({}) should declare smoothing metadata",
            binding.id().0,
            binding.info().name
        );
    }

    assert!(
        parameter_binding(7)
            .unwrap()
            .smoothed_atomic_spec()
            .is_none()
    );
    assert!(
        parameter_binding(10)
            .unwrap()
            .smoothed_atomic_spec()
            .is_none()
    );
}

#[test]
fn audio_expression_parameters_update_the_runtime_patch() {
    for id in [
        AUDIO_EXPRESSION_ENABLE_PARAMETER_ID,
        AUDIO_EXPRESSION_PITCH_RANGE_PARAMETER_ID,
        AUDIO_EXPRESSION_PRESSURE_FLOOR_PARAMETER_ID,
        AUDIO_EXPRESSION_PRESSURE_CEILING_PARAMETER_ID,
        AUDIO_EXPRESSION_BRIGHTNESS_FLOOR_PARAMETER_ID,
        AUDIO_EXPRESSION_BRIGHTNESS_CEILING_PARAMETER_ID,
    ] {
        let binding = parameter_binding(id).expect("missing audio expression binding");
        assert_eq!(
            binding.runtime_target(),
            RuntimeParameterTarget::Patch,
            "{} should update runtime patch state for audio expression",
            binding.info().name
        );
    }
}

#[test]
fn live_excitation_gain_updates_the_runtime_patch() {
    let binding = parameter_binding(LIVE_EXCITATION_GAIN_PARAMETER_ID)
        .expect("missing live excitation gain binding");

    assert_eq!(
        binding.runtime_target(),
        RuntimeParameterTarget::Patch,
        "live excitation gain should update runtime patch state"
    );
}

#[test]
fn registry_smoothing_metadata_maps_master_gain_to_linear_gain() {
    let spec = parameter_binding(MASTER_GAIN_PARAMETER_ID)
        .unwrap()
        .smoothed_atomic_spec()
        .unwrap();

    assert_eq!(spec.info.id, ParameterId(MASTER_GAIN_PARAMETER_ID));
    assert_eq!(spec.smoothed_value(0.0), 1.0);
    assert!((spec.smoothed_value(-60.0) - MASTER_GAIN_LINEAR.min).abs() < 0.000_001);
    assert!((spec.smoothed_value(12.0) - MASTER_GAIN_LINEAR.max).abs() < 0.000_01);
}

#[test]
fn smoothed_runtime_parameter_update_does_not_allocate() {
    let mut parameter =
        smoothed_runtime_parameter(FILTER_CUTOFF_PARAMETER_ID, 48_000.0, 20_000.0).unwrap();

    assert_no_allocations("smoothed runtime parameter update", || {
        parameter.atomic().store_normalized(0.25);
        assert!(parameter.sync_from_atomic());
        for _ in 0..8 {
            parameter.next_sample();
        }
    });
}

#[test]
fn editor_bindings_are_single_source_metadata() {
    let mut count = 0;
    for binding in editor_parameter_bindings() {
        count += 1;
        assert_editor_binding_roundtrips(binding);
        assert_editor_metadata_valid(binding);
    }

    assert_eq!(count, EditorSurfaceSlot::ALL.len());
    assert_eq!(PARAMETER_BINDING_COUNT, PARAMETER_BINDINGS.len());
}

#[test]
fn editor_surface_bindings_project_from_registry_metadata() {
    let surface_bindings = resonator_editor_parameter_bindings().collect::<Vec<_>>();
    assert_eq!(surface_bindings.len(), EditorSurfaceSlot::ALL.len());

    for surface_binding in surface_bindings {
        let binding = parameter_binding(surface_binding.id()).expect("projected binding id");
        let editor = binding.editor().expect("projected binding editor metadata");

        assert_eq!(surface_binding.slot(), editor.slot());
        assert_eq!(surface_binding.label(), editor.label());
        assert_eq!(surface_binding.control(), editor.control());
    }
}

#[test]
fn v2_audio_parameters_are_visible_on_the_registry_backed_editor_surface() {
    for (id, slot) in [
        (
            AUDIO_INPUT_MODE_PARAMETER_ID,
            EditorSurfaceSlot::AudioInputMode,
        ),
        (
            AUDIO_EXPRESSION_ENABLE_PARAMETER_ID,
            EditorSurfaceSlot::AudioExpressionEnable,
        ),
        (
            AUDIO_EXPRESSION_PITCH_RANGE_PARAMETER_ID,
            EditorSurfaceSlot::AudioExpressionPitchRange,
        ),
        (
            AUDIO_NOTE_ONSET_SENSITIVITY_PARAMETER_ID,
            EditorSurfaceSlot::AudioNoteOnsetSensitivity,
        ),
        (
            AUDIO_NOTE_PITCH_CONFIDENCE_PARAMETER_ID,
            EditorSurfaceSlot::AudioNotePitchConfidence,
        ),
        (
            LIVE_EXCITATION_MODE_PARAMETER_ID,
            EditorSurfaceSlot::LiveExcitationMode,
        ),
        (
            LIVE_EXCITATION_LATCH_WINDOW_PARAMETER_ID,
            EditorSurfaceSlot::LiveExcitationLatchWindow,
        ),
    ] {
        let binding = parameter_binding(id).expect("v2 parameter binding");
        assert_eq!(
            binding.editor().expect("visible v2 editor binding").slot(),
            slot
        );
    }

    assert!(matches!(
        parameter_binding(AUDIO_INPUT_MODE_PARAMETER_ID)
            .unwrap()
            .editor()
            .unwrap()
            .control(),
        EditorControlKind::Segmented { .. }
    ));
    assert!(matches!(
        parameter_binding(LIVE_EXCITATION_MODE_PARAMETER_ID)
            .unwrap()
            .editor()
            .unwrap()
            .control(),
        EditorControlKind::Segmented { .. }
    ));
}

fn assert_editor_binding_roundtrips(binding: &ParameterBinding) {
    let editor = binding
        .editor()
        .expect("visible binding should have editor metadata");
    assert_eq!(
        editor_parameter_binding(editor.slot())
            .expect("surface slot should map back to a binding")
            .id(),
        binding.id()
    );
    assert_eq!(
        parameter_binding(binding.id().0)
            .expect("editor binding should be real")
            .id(),
        binding.id()
    );
}

fn assert_editor_metadata_valid(binding: &ParameterBinding) {
    let editor = binding
        .editor()
        .expect("visible binding should have editor metadata");
    assert!(!editor.label().is_empty());
    match editor.control() {
        EditorControlKind::Knob | EditorControlKind::Slider { .. } => {}
        EditorControlKind::Binary {
            left_label,
            right_label,
            width,
        } => {
            assert!(!left_label.is_empty());
            assert!(!right_label.is_empty());
            assert!(width > 0.0);
        }
        EditorControlKind::Segmented { labels, width }
        | EditorControlKind::Selector { labels, width } => {
            assert!(!labels.is_empty());
            assert!(width > 0.0);
        }
    }
}

#[test]
fn every_editor_surface_slot_resolves_to_a_binding() {
    for slot in EditorSurfaceSlot::ALL {
        let binding = editor_parameter_binding(slot)
            .unwrap_or_else(|| panic!("missing binding for editor slot {slot:?}"));
        assert_eq!(binding.editor().unwrap().slot(), slot);
    }
}

#[test]
fn required_editor_surface_groups_have_visible_parameters() {
    for group in EditorSurfaceGroup::REQUIRED {
        assert!(
            editor_parameter_bindings_for_group(group).next().is_some(),
            "missing visible editor parameter for {group:?}",
        );
    }
}

#[test]
fn editor_surface_group_orders_are_unique() {
    for group in EditorSurfaceGroup::REQUIRED {
        let mut orders = Vec::new();
        for binding in editor_parameter_bindings_for_group(group) {
            let order = binding.editor().unwrap().order();
            assert!(
                !orders.contains(&order),
                "duplicate editor order {order} in {group:?}",
            );
            orders.push(order);
        }
    }
}

#[test]
fn enum_codecs_round_trip() {
    assert_codec_roundtrip(&[
        FilterMode::LowPass,
        FilterMode::BandPass,
        FilterMode::HighPass,
    ]);
    assert_codec_roundtrip(&[
        AudioInputMode::Off,
        AudioInputMode::AudioCreatesNotes,
        AudioInputMode::MidiPlusAudioCreatesNotes,
    ]);
    assert_codec_roundtrip(&[
        LiveExcitationMode::Off,
        LiveExcitationMode::Continuous,
        LiveExcitationMode::NoteLatched,
        LiveExcitationMode::ContinuousAndNoteLatched,
    ]);
    assert_codec_roundtrip(&[RoutingMode::Parallel, RoutingMode::Series]);
    assert_codec_roundtrip(&[ResonatorModel::Modal, ResonatorModel::Waveguide]);
    assert_codec_roundtrip(&[
        ModalPreset::Kalimba,
        ModalPreset::Marimba,
        ModalPreset::Bell,
        ModalPreset::GlassBowl,
        ModalPreset::MetalBar,
        ModalPreset::Woodblock,
        ModalPreset::GenericStrike,
    ]);
    assert_codec_roundtrip(&[
        LfoShape::Sine,
        LfoShape::Triangle,
        LfoShape::Saw,
        LfoShape::Square,
        LfoShape::SampleAndHold,
    ]);
    assert_codec_roundtrip(&[
        ModulationSource::SecondaryEnvelope,
        ModulationSource::Lfo,
        ModulationSource::Velocity,
        ModulationSource::Aftertouch,
        ModulationSource::ModWheel,
        ModulationSource::Brightness,
    ]);
    assert_codec_roundtrip(&[
        ModulationDestination::FilterCutoff,
        ModulationDestination::ResonatorADamping,
        ModulationDestination::ResonatorBDamping,
        ModulationDestination::ResonatorAPosition,
        ModulationDestination::ResonatorBPosition,
        ModulationDestination::ExcitationGain,
        ModulationDestination::LfoRate,
    ]);
    assert_codec_roundtrip(&[WaveguideStyle::String, WaveguideStyle::Tube]);
}

fn prepare_patch_for_binding(patch: &mut ResonatorSynthPatch, binding: ParameterBinding) {
    if let ParameterPath::Resonator { slot, parameter } = binding.path() {
        match parameter {
            ResonatorParameter::Modal(_) => {
                *slot.config_mut(patch) = ResonatorConfig::Modal(ModalConfig::default());
            }
            ResonatorParameter::Waveguide(_) => {
                *slot.config_mut(patch) = ResonatorConfig::Waveguide(WaveguideConfig::default());
            }
            ResonatorParameter::Model => {}
        }
    }
}

fn non_default_probe_value(range: ParameterRange) -> f32 {
    if (range.default - range.min).abs() > 0.001 {
        range.min
    } else {
        range.max
    }
}

fn assert_codec_roundtrip<T>(values: &[T])
where
    T: ParameterCodec + std::fmt::Debug + PartialEq,
{
    for (index, value) in values.iter().copied().enumerate() {
        assert_eq!(value.to_index(), index as u32);
        assert_eq!(T::from_plain(value.plain()), value);
        assert!(!value.label().is_empty());
    }
}
