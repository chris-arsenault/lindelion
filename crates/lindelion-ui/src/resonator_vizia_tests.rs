use super::*;

#[test]
fn host_requires_complete_parameter_surface() {
    let mut bindings = all_parameter_bindings();
    bindings.pop();

    assert!(matches!(
        ResonatorEditorHost::new(0, bindings, mock_callbacks()),
        Err(ResonatorEditorHostError::MissingSlot(
            ResonatorEditorSurfaceSlot::Mod1Amount
        ))
    ));
}

#[test]
fn host_rejects_duplicate_surface_slot() {
    let mut bindings = all_parameter_bindings();
    bindings.push(ResonatorEditorParameterBinding::new(
        999,
        ResonatorEditorSurfaceSlot::Master,
        "Duplicate",
        ResonatorEditorControlKind::Knob,
    ));

    assert!(matches!(
        ResonatorEditorHost::new(0, bindings, mock_callbacks()),
        Err(ResonatorEditorHostError::DuplicateSlot(
            ResonatorEditorSurfaceSlot::Master
        ))
    ));
}

#[test]
fn host_callbacks_project_mock_editor_state() {
    let host = ResonatorEditorHost::new(0, all_parameter_bindings(), mock_callbacks()).unwrap();

    assert_eq!(unsafe { host.parameter_value(12) }, 0.25);
    assert_eq!(unsafe { host.parameter_value_text(12, 0.25) }, "12=0.25");
    assert_eq!(unsafe { host.summary() }.patch_name, "Mock");
}

#[cfg(target_os = "macos")]
#[test]
fn constructs_vizia_application_from_mock_binding() {
    let host = ResonatorEditorHost::new(0, all_parameter_bindings(), mock_callbacks()).unwrap();
    let _application =
        unsafe { build_resonator_application(host, platform::ResonatorEditorSize::default()) };
}

fn all_parameter_bindings() -> Vec<ResonatorEditorParameterBinding> {
    ResonatorEditorSurfaceSlot::ALL
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            ResonatorEditorParameterBinding::new(
                index as u32 + 1,
                *slot,
                "Parameter",
                ResonatorEditorControlKind::Knob,
            )
        })
        .collect()
}

fn mock_callbacks() -> ResonatorEditorCallbacks {
    ResonatorEditorCallbacks {
        refresh_library: |_| {},
        parameter_value: |_, _| 0.25,
        set_parameter: |_, _, _| {},
        parameter_value_text: |_, id, normalized| format!("{id}={normalized:.2}"),
        default_normalized: |_, _| 0.5,
        summary: |_| ResonatorEditorPatchSummary {
            patch_name: "Mock".to_string(),
            ..ResonatorEditorPatchSummary::default()
        },
        telemetry: |_| ResonatorEditorTelemetry::default(),
        directories: |_| ResonatorEditorDirectories::default(),
        request_telemetry: |_| {},
        handle_command: |_, _| {},
    }
}
