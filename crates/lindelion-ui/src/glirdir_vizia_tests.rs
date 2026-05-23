use super::*;

fn callbacks() -> GlirdirEditorCallbacks {
    GlirdirEditorCallbacks {
        parameter_value,
        set_parameter,
        parameter_value_text,
        default_normalized,
        status,
        preview,
        request_status,
        handle_command,
        prepare_midi_drag,
    }
}

unsafe fn parameter_value(_context: usize, _parameter_id: u32) -> f32 {
    0.0
}

unsafe fn set_parameter(_context: usize, _parameter_id: u32, _normalized: f64) {}

unsafe fn parameter_value_text(_context: usize, _parameter_id: u32, _normalized: f64) -> String {
    String::new()
}

unsafe fn default_normalized(_context: usize, _parameter_id: u32) -> f32 {
    0.0
}

unsafe fn status(_context: usize) -> GlirdirEditorStatus {
    GlirdirEditorStatus::default()
}

unsafe fn preview(_context: usize) -> GlirdirEditorPreview {
    GlirdirEditorPreview::default()
}

unsafe fn request_status(_context: usize) {}

unsafe fn handle_command(_context: usize, _command: GlirdirEditorCommand) {}

unsafe fn prepare_midi_drag(_context: usize) -> GlirdirEditorMidiDrag {
    GlirdirEditorMidiDrag::Requested
}

fn binding(slot: GlirdirEditorSurfaceSlot) -> GlirdirEditorParameterBinding {
    GlirdirEditorParameterBinding::new(
        slot.index() as u32,
        slot,
        "Control",
        GlirdirEditorControlKind::Slider { width: 120.0 },
    )
}

#[test]
fn host_requires_complete_parameter_surface() {
    let bindings = GlirdirEditorSurfaceSlot::ALL
        .into_iter()
        .filter(|slot| *slot != GlirdirEditorSurfaceSlot::Grid)
        .map(binding);

    let error = GlirdirEditorHost::new(0, bindings, callbacks()).unwrap_err();

    assert_eq!(
        error,
        GlirdirEditorHostError::MissingSlot(GlirdirEditorSurfaceSlot::Grid)
    );
}

#[test]
fn host_rejects_duplicate_surface_slot() {
    let mut bindings = GlirdirEditorSurfaceSlot::ALL
        .into_iter()
        .map(binding)
        .collect::<Vec<_>>();
    bindings.push(binding(GlirdirEditorSurfaceSlot::Grid));

    let error = GlirdirEditorHost::new(0, bindings, callbacks()).unwrap_err();

    assert_eq!(
        error,
        GlirdirEditorHostError::DuplicateSlot(GlirdirEditorSurfaceSlot::Grid)
    );
}

#[test]
fn target_layout_fits_fixed_editor_size() {
    assert!(GlirdirEditorLayout::target().panels_fit());
}
