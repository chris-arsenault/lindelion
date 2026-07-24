use super::*;

#[test]
fn controller_sets_linnod_pitch_shift_algorithm() {
    let controller = LinnodVst3Controller::new();

    controller.set_pitch_shift_algorithm(
        lindelion_ui::linnod_vizia::LinnodEditorPitchShiftAlgorithm::ResampleStretch,
    );

    assert_eq!(
        controller.patch.borrow().engine.pitch_shift_algorithm,
        PitchShiftAlgorithm::ResampleStretch
    );
    assert_eq!(
        controller.summary.borrow().pitch_shift_algorithm,
        lindelion_ui::linnod_vizia::LinnodEditorPitchShiftAlgorithm::ResampleStretch
    );
}
