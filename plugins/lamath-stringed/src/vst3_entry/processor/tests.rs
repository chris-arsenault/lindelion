use super::*;

#[test]
fn reports_output_and_midi_buses() {
    assert_eq!(
        vst3_bus_count(
            &STRINGED_BUSES,
            MediaTypes_::kAudio as MediaType,
            BusDirections_::kOutput as BusDirection,
        ),
        1
    );
    assert_eq!(
        vst3_bus_count(
            &STRINGED_BUSES,
            MediaTypes_::kEvent as MediaType,
            BusDirections_::kInput as BusDirection,
        ),
        1
    );
}

#[test]
fn exposes_sparse_parameter_count() {
    let processor = LamathStringedVst3Processor::new();
    assert_eq!(
        unsafe { processor.getParameterCount() },
        parameters::PARAMETER_COUNT as i32
    );
}

#[test]
fn editor_state_does_not_collapse_while_plugin_is_borrowed() {
    let processor = LamathStringedVst3Processor::new();
    let _busy = processor.plugin.borrow_mut();

    assert_eq!(processor.editor_knobs().len(), parameters::PARAMETER_COUNT);
    assert_eq!(processor.selected_driver(), LamathStringedDriverId::Pick);
    assert_eq!(processor.selected_body(), LamathStringedBodyId::Guitar);
    assert!(!processor.model_switches().is_empty());
    assert_eq!(
        processor.articulation_slot_list_view().slots.len(),
        crate::processor::ARTICULATION_SLOT_COUNT
    );
}

#[test]
fn editor_parameter_change_queues_while_plugin_is_borrowed() {
    let processor = LamathStringedVst3Processor::new();
    let parameter = parameters::PARAMETERS[0];
    let _busy = processor.plugin.borrow_mut();

    processor.set_editor_parameter(parameter.id.0, 0.79);
    let knob = processor
        .editor_knobs()
        .into_iter()
        .find(|knob| knob.id == parameter.id.0)
        .expect("queued parameter should remain visible in editor knobs");
    assert!((knob.normalized - 0.79).abs() < 0.000_001);

    drop(_busy);
    let mut plugin = processor.plugin.borrow_mut();
    crate::assert_no_allocations("lamath_stringed_pending_editor_parameter", || {
        processor.apply_pending_values(&mut plugin);
    });
    let normalized =
        parameters::normalized_value(plugin.patch(), parameter.id.0).expect("known parameter");
    assert!((normalized - 0.79).abs() < 0.000_001);
}
