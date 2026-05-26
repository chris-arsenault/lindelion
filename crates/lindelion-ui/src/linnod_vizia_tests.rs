use super::*;

fn callbacks() -> LinnodEditorCallbacks {
    LinnodEditorCallbacks {
        parameter_value,
        set_parameter,
        parameter_value_text,
        default_normalized,
        status,
        telemetry,
        summary,
        directories,
        request_status,
        request_telemetry,
        handle_command,
        edit_marker,
        edit_pad,
        edit_playback,
        edit_detection,
        edit_slice,
    }
}

unsafe fn parameter_value(_context: usize, _parameter_id: u32) -> f32 {
    0.25
}

unsafe fn set_parameter(_context: usize, _parameter_id: u32, _normalized: f64) {}

unsafe fn parameter_value_text(_context: usize, parameter_id: u32, normalized: f64) -> String {
    format!("{parameter_id}:{normalized:.2}")
}

unsafe fn default_normalized(_context: usize, _parameter_id: u32) -> f32 {
    0.5
}

unsafe fn status(_context: usize) -> LinnodEditorStatus {
    LinnodEditorStatus {
        source_status: LinnodEditorSourceStatus::Ready,
        has_source: true,
        has_analysis: true,
        marker_count: 2,
        selected_slice_index: Some(1),
    }
}

unsafe fn telemetry(_context: usize) -> LinnodEditorTelemetry {
    LinnodEditorTelemetry {
        active_voices: 3.0,
        ..LinnodEditorTelemetry::default()
    }
}

unsafe fn summary(_context: usize) -> LinnodEditorPatchSummary {
    LinnodEditorPatchSummary {
        patch_name: "Mock".to_string(),
        markers: vec![LinnodEditorMarker {
            position_samples: 128,
            kind: LinnodEditorMarkerKind::User,
        }],
        ..LinnodEditorPatchSummary::default()
    }
}

unsafe fn directories(_context: usize) -> LinnodEditorDirectories {
    LinnodEditorDirectories::default()
}

unsafe fn request_status(_context: usize) {}

unsafe fn request_telemetry(_context: usize) {}

unsafe fn handle_command(_context: usize, _request: LinnodEditorCommandRequest<'_>) {}

unsafe fn edit_marker(_context: usize, _edit: LinnodEditorMarkerEdit) {}

unsafe fn edit_pad(_context: usize, _edit: LinnodEditorPadEdit) {}

unsafe fn edit_playback(_context: usize, _edit: LinnodEditorPlaybackEdit) {}

unsafe fn edit_detection(_context: usize, _edit: LinnodEditorDetectionEdit) {}

unsafe fn edit_slice(_context: usize, _edit: LinnodEditorSliceEdit) {}

fn binding(slot: LinnodEditorSurfaceSlot) -> LinnodEditorParameterBinding {
    LinnodEditorParameterBinding::new(
        slot.index() as u32 + 1,
        slot,
        "Control",
        LinnodEditorControlKind::Slider { width: 120.0 },
    )
}

#[test]
fn host_requires_complete_parameter_surface() {
    let bindings = LinnodEditorSurfaceSlot::ALL
        .into_iter()
        .filter(|slot| *slot != LinnodEditorSurfaceSlot::TuningReference)
        .map(binding);

    let error = LinnodEditorHost::new(0, bindings, callbacks()).unwrap_err();

    assert_eq!(
        error,
        LinnodEditorHostError::MissingSlot(LinnodEditorSurfaceSlot::TuningReference)
    );
}

#[test]
fn host_rejects_duplicate_surface_slot() {
    let mut bindings = LinnodEditorSurfaceSlot::ALL
        .into_iter()
        .map(binding)
        .collect::<Vec<_>>();
    bindings.push(binding(LinnodEditorSurfaceSlot::MasterGain));

    let error = LinnodEditorHost::new(0, bindings, callbacks()).unwrap_err();

    assert_eq!(
        error,
        LinnodEditorHostError::DuplicateSlot(LinnodEditorSurfaceSlot::MasterGain)
    );
}

#[test]
fn host_projects_linnod_editor_state() {
    let host = LinnodEditorHost::new(
        0,
        LinnodEditorSurfaceSlot::ALL.into_iter().map(binding),
        callbacks(),
    )
    .unwrap();

    assert_eq!(unsafe { host.parameter_value(1) }, 0.25);
    assert_eq!(unsafe { host.parameter_value_text(3, 0.5) }, "3:0.50");
    assert_eq!(
        unsafe { host.status() }.source_status,
        LinnodEditorSourceStatus::Ready
    );
    assert_eq!(unsafe { host.summary() }.markers[0].position_samples, 128);
    assert_eq!(unsafe { host.telemetry() }.active_voices, 3.0);
}
