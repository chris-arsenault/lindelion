use super::*;

#[test]
fn controller_projects_pitch_map_source_summary() {
    let controller = LinnodVst3Controller::new();

    notify_controller(
        &controller,
        LinnodPluginMessage::SourceSummaryResponse(source_summary_payload().encode().unwrap()),
    );

    let summary = controller.summary.borrow();
    assert_eq!(summary.pitch_map.len(), 1);
    assert_eq!(summary.pitch_map[0].midi_note, 57);
}

#[test]
fn controller_sets_pitch_map_trigger_mode() {
    let controller = LinnodVst3Controller::new();

    controller.set_trigger_mode(lindelion_ui::linnod_vizia::LinnodEditorTriggerMode::PitchMap);

    assert_eq!(
        controller.patch.borrow().trigger_mode,
        crate::TriggerMode::PitchMap
    );
    assert_eq!(
        controller.summary.borrow().trigger_mode,
        lindelion_ui::linnod_vizia::LinnodEditorTriggerMode::PitchMap
    );
}
