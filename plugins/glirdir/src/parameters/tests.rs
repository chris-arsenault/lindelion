use super::*;

#[test]
fn quantize_strength_updates_patch_through_binding() {
    let mut patch = GlirdirPatch::default();

    let apply = parameter_binding(TIMING_STRENGTH_PARAMETER_ID)
        .unwrap()
        .apply_plain(&mut patch, 0.25);

    assert_eq!(apply, ParameterApplyKind::Quantize);
    assert_eq!(patch.quantize.timing_strength, 0.25);
}

#[test]
fn scale_parameter_uses_shared_scale_type() {
    let mut patch = GlirdirPatch::default();

    parameter_binding(SCALE_PARAMETER_ID)
        .unwrap()
        .apply_plain(&mut patch, 2.0);

    assert_eq!(patch.quantize.scale, Scale::NaturalMinor);
}

#[test]
fn capture_bars_middle_host_step_selects_eight_bars() {
    let mut patch = GlirdirPatch::default();
    let binding = parameter_binding(CAPTURE_BARS_PARAMETER_ID).unwrap();

    let middle_plain = binding.info().range.denormalize(0.5);
    let apply = binding.apply_plain(&mut patch, middle_plain);

    assert_eq!(apply, ParameterApplyKind::Capture);
    assert_eq!(patch.capture.bars, 8);
    assert_eq!(binding.plain_value(&patch), 1.0);
    assert_eq!(binding.format_plain_value(middle_plain), "8");
}

#[test]
fn every_host_parameter_resolves_to_one_binding() {
    assert_eq!(PARAMETERS.len(), PARAMETER_BINDINGS.len());
    assert_eq!(PARAMETER_BINDING_COUNT, PARAMETER_BINDINGS.len());

    for (index, parameter) in PARAMETERS.iter().enumerate() {
        let matches = PARAMETER_BINDINGS
            .iter()
            .filter(|binding| binding.info().id == parameter.id)
            .count();
        assert_eq!(matches, 1, "parameter {:?} binding count", parameter.id);
        assert_eq!(
            parameter_binding(parameter.id.0).map(|binding| binding.info()),
            Some(*parameter)
        );
        assert_eq!(parameter_binding_index(parameter.id.0), Some(index));
        assert_eq!(
            parameter_binding_by_index(index).map(|binding| binding.info()),
            Some(*parameter)
        );
    }
}

#[test]
fn every_binding_round_trips_patch_get_set() {
    for binding in PARAMETER_BINDINGS {
        let mut patch = GlirdirPatch::default();
        let value = non_default_probe_value(binding.info().range);

        let apply = binding.apply_plain(&mut patch, value);

        assert_eq!(apply, binding.apply());
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
    assert_eq!(
        parameter_binding(CAPTURE_BARS_PARAMETER_ID)
            .unwrap()
            .format_plain_value(1.0),
        "8"
    );
    assert_eq!(
        parameter_binding(SYNC_MODE_PARAMETER_ID)
            .unwrap()
            .format_plain_value(2.0),
        "Bar"
    );
    assert_eq!(
        parameter_binding(SCALE_PARAMETER_ID)
            .unwrap()
            .format_plain_value(2.0),
        "Minor"
    );
    assert_eq!(
        parameter_binding(GRID_PARAMETER_ID)
            .unwrap()
            .format_plain_value(4.0),
        "1/4T"
    );
    assert_eq!(
        parameter_binding(TIMING_STRENGTH_PARAMETER_ID)
            .unwrap()
            .format_plain_value(0.25),
        "0.25"
    );
}

#[test]
fn every_visible_editor_control_resolves_to_one_parameter_binding() {
    let editor_bindings = editor_parameter_bindings().collect::<Vec<_>>();
    assert_eq!(editor_bindings.len(), GlirdirEditorSurfaceSlot::ALL.len());

    for editor in editor_bindings.iter().copied() {
        let matches = PARAMETER_BINDINGS
            .iter()
            .filter(|binding| binding.id().0 == editor.id())
            .count();
        assert_eq!(matches, 1, "editor binding {:?}", editor.slot());

        let registry_binding = parameter_binding(editor.id()).expect("projected binding id");
        let metadata = registry_binding
            .editor()
            .expect("projected binding editor metadata");
        assert_eq!(metadata.slot, editor.slot());
        assert_eq!(metadata.label, editor.label());
        assert_eq!(metadata.control, editor.control());
    }

    for slot in GlirdirEditorSurfaceSlot::ALL {
        let count = editor_bindings
            .iter()
            .filter(|binding| binding.slot() == slot)
            .count();
        assert_eq!(count, 1, "editor slot {slot:?}");
    }
}

#[test]
fn enum_codecs_round_trip() {
    assert_codec_roundtrip(&[CaptureBars::Four, CaptureBars::Eight, CaptureBars::Sixteen]);
    assert_codec_roundtrip(&[
        SyncModeParameter::Immediate,
        SyncModeParameter::PhraseBoundary,
        SyncModeParameter::NextDownbeat,
    ]);
    assert_codec_roundtrip(&[CountInBars::Zero, CountInBars::One, CountInBars::Two]);
    assert_codec_roundtrip(&[
        RootNoteParameter::C,
        RootNoteParameter::CSharp,
        RootNoteParameter::D,
        RootNoteParameter::DSharp,
        RootNoteParameter::E,
        RootNoteParameter::F,
        RootNoteParameter::FSharp,
        RootNoteParameter::G,
        RootNoteParameter::GSharp,
        RootNoteParameter::A,
        RootNoteParameter::ASharp,
        RootNoteParameter::B,
    ]);
    assert_codec_roundtrip(&[
        ScaleParameter::Chromatic,
        ScaleParameter::Major,
        ScaleParameter::NaturalMinor,
        ScaleParameter::HarmonicMinor,
        ScaleParameter::MelodicMinor,
        ScaleParameter::PentatonicMajor,
        ScaleParameter::PentatonicMinor,
        ScaleParameter::Blues,
        ScaleParameter::Dorian,
        ScaleParameter::Mixolydian,
    ]);
    assert_codec_roundtrip(&[
        SnapModeParameter::Hard,
        SnapModeParameter::Soft,
        SnapModeParameter::Off,
    ]);
    assert_codec_roundtrip(&[
        TimingGridParameter::Quarter,
        TimingGridParameter::Eighth,
        TimingGridParameter::Sixteenth,
        TimingGridParameter::ThirtySecond,
        TimingGridParameter::QuarterTriplet,
        TimingGridParameter::EighthTriplet,
        TimingGridParameter::SixteenthTriplet,
    ]);
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
    assert_eq!(values.len(), T::LABELS.len());
    assert_eq!(T::MAX_INDEX as usize + 1, values.len());

    for (index, value) in values.iter().copied().enumerate() {
        assert_eq!(value.to_index(), index as u32);
        assert_eq!(T::from_plain(value.plain()), value);
        assert_eq!(T::from_index(index as u32), value);
        assert!(!value.label().is_empty());
    }
}
