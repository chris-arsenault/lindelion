use super::*;

#[test]
fn controller_applies_slice_auto_tune_edits_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();

    assert_eq!(
        controller.apply_slice_edit(LinnodSliceEditMessage::AutoTuneOverride {
            slice_index: 0,
            enabled: true,
        }),
        kResultFalse
    );
    assert_eq!(
        controller.apply_slice_edit(LinnodSliceEditMessage::AutoTuneEnabled {
            slice_index: 0,
            enabled: true,
        }),
        kResultFalse
    );

    assert!(controller.patch.borrow().slices[0].use_auto_tune_override);
    assert!(controller.patch.borrow().slices[0].auto_tune_enabled);
}

#[test]
fn controller_applies_auto_tune_edit_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();

    assert_eq!(
        controller.apply_auto_tune_edit(LinnodAutoTuneEditMessage::Enabled(true)),
        kResultFalse
    );

    assert!(controller.patch.borrow().auto_tune.enabled);
    assert!(controller.summary.borrow().auto_tune.enabled);
}

#[test]
fn processor_notify_applies_auto_tune_edit_payload() {
    let processor = LinnodVst3Processor::new();
    let message =
        LinnodPluginMessage::AutoTuneEdit(LinnodAutoTuneEditMessage::Enabled(true).encode())
            .into_com_message()
            .to_com_ptr::<IMessage>()
            .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert!(processor.plugin.borrow().patch().auto_tune.enabled);
}
