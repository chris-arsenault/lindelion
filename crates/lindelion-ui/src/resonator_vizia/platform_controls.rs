fn resonator_parameter_control(
    cx: &mut Context,
    control: EditorParameterControl,
    accent: crate::vizia_controls::Accent,
) {
    match control.editor.control() {
        ResonatorEditorControlKind::Knob => resonator_knob(cx, control, is_centered(control), accent),
        ResonatorEditorControlKind::Slider { .. } => {
            resonator_knob(cx, control, is_centered(control), accent)
        }
        ResonatorEditorControlKind::Binary {
            left_label,
            right_label,
            width,
        } => crate::vizia_controls::inline_binary_segmented(
            cx,
            control.label(),
            control.signal,
            left_label,
            right_label,
            width,
            move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter {
                    id: control.id,
                    normalized,
                });
            },
        ),
        ResonatorEditorControlKind::Segmented { labels, width } => {
            crate::vizia_controls::inline_parameter_segmented(
                cx,
                control.label(),
                control.signal,
                labels,
                width,
                move |cx, normalized| {
                    cx.emit(EditorEvent::SetParameter {
                        id: control.id,
                        normalized,
                    });
                },
            )
        }
        ResonatorEditorControlKind::Selector { labels, .. } => crate::vizia_controls::parameter_stepper(
            cx,
            control.label(),
            control.value_text(),
            control.signal,
            labels.len(),
            move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter {
                    id: control.id,
                    normalized,
                });
            },
        ),
    }
}

fn resonator_knob(
    cx: &mut Context,
    control: EditorParameterControl,
    centered: bool,
    accent: crate::vizia_controls::Accent,
) {
    crate::vizia_controls::parameter_knob(
        cx,
        control.label(),
        control.value_text(),
        control.signal,
        unsafe { control.host.default_normalized(control.id) },
        centered,
        accent,
        move |cx, normalized| {
            cx.emit(EditorEvent::SetParameter {
                id: control.id,
                normalized,
            });
        },
    );
}

fn resonator_tool_button(
    cx: &mut Context,
    icon: &'static str,
    tooltip: &'static str,
    command: UiCommand,
) {
    crate::vizia_controls::icon_tool_button(cx, icon, tooltip)
        .on_press(move |cx| cx.emit(EditorEvent::Command(command)));
}

fn is_centered(control: EditorParameterControl) -> bool {
    matches!(
        control.editor.slot(),
        ResonatorEditorSurfaceSlot::Pan | ResonatorEditorSurfaceSlot::Mod1Amount
    )
}
