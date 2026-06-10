use super::*;

#[test]
fn reports_output_and_midi_buses() {
    assert_eq!(
        vst3_bus_count(
            &CYMBAL_BUSES,
            MediaTypes_::kAudio as MediaType,
            BusDirections_::kOutput as BusDirection,
        ),
        1
    );
    assert_eq!(
        vst3_bus_count(
            &CYMBAL_BUSES,
            MediaTypes_::kEvent as MediaType,
            BusDirections_::kInput as BusDirection,
        ),
        1
    );
}

#[test]
fn exposes_sparse_parameter_count() {
    let processor = LamathCymbalVst3Processor::new();
    assert_eq!(
        unsafe { processor.getParameterCount() },
        parameters::PARAMETER_COUNT as i32
    );
}

#[test]
fn editor_state_does_not_collapse_while_plugin_is_borrowed() {
    let processor = LamathCymbalVst3Processor::new();
    let _busy = processor.plugin.borrow_mut();

    assert_eq!(processor.editor_knobs().len(), parameters::PARAMETER_COUNT);
    assert_eq!(
        processor.editor_presets().len(),
        crate::presets::CYMBAL_PRESETS.len()
    );
    assert_eq!(processor.active_editor_preset(), Some(0));
    assert_eq!(
        processor.striker_slot_list_view().slots.len(),
        crate::processor::STRIKER_SLOT_COUNT
    );
}

#[test]
fn editor_parameter_change_queues_while_plugin_is_borrowed() {
    let processor = LamathCymbalVst3Processor::new();
    let parameter = parameters::PARAMETERS[0];
    let _busy = processor.plugin.borrow_mut();

    processor.set_editor_parameter(parameter.id.0, 0.91);
    let knob = processor
        .editor_knobs()
        .into_iter()
        .find(|knob| knob.id == parameter.id.0)
        .expect("queued parameter should remain visible in editor knobs");
    assert!((knob.normalized - 0.91).abs() < 0.000_001);

    drop(_busy);
    let mut plugin = processor.plugin.borrow_mut();
    crate::assert_no_allocations("lamath_cymbal_pending_editor_parameter", || {
        processor.apply_pending_values(&mut plugin);
    });
    let normalized =
        parameters::normalized_value(plugin.patch(), parameter.id.0).expect("known parameter");
    assert!((normalized - 0.91).abs() < 0.000_001);
}

#[test]
fn editor_preset_change_queues_while_plugin_is_borrowed() {
    let processor = LamathCymbalVst3Processor::new();
    let _busy = processor.plugin.borrow_mut();

    processor.apply_editor_preset(3);

    assert_eq!(processor.active_editor_preset(), Some(3));

    drop(_busy);
    let mut plugin = processor.plugin.borrow_mut();
    crate::assert_no_allocations("lamath_cymbal_pending_editor_preset", || {
        processor.apply_pending_values(&mut plugin);
    });
    assert_eq!(plugin.active_preset(), Some(3));
}
