fn linnod_parameter_control(
    cx: &mut Context,
    control: EditorParameterControl,
    accent: crate::vizia_controls::Accent,
) {
    let centered = matches!(control.editor.slot(), LinnodEditorSurfaceSlot::MasterGain);
    match control.editor.control() {
        LinnodEditorControlKind::Knob | LinnodEditorControlKind::Slider { .. } => {
            crate::vizia_controls::parameter_knob(
                cx,
                control.label(),
                control.value_text(),
                control.signal,
                control.default_normalized(),
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
        LinnodEditorControlKind::Binary {
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
        LinnodEditorControlKind::Segmented { labels, width } => {
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
        LinnodEditorControlKind::Selector { labels, width } => crate::vizia_controls::parameter_cycle_selector(
            cx,
            control.label(),
            control.value_text(),
            control.signal,
            labels.len(),
            width,
            move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter {
                    id: control.id,
                    normalized,
                });
            },
        ),
    }
}

fn linnod_command_button(
    cx: &mut Context,
    icon: &'static str,
    tooltip: &'static str,
    event: EditorEvent,
) {
    crate::vizia_controls::icon_tool_button(cx, icon, tooltip).on_press(move |cx| {
        cx.emit(event.clone());
    });
}

fn linnod_status_chip<T>(cx: &mut Context, text: T, kind: crate::vizia_controls::ChipKind)
where
    T: Res<String> + Clone + 'static,
{
    crate::vizia_controls::status_chip(cx, text, kind);
}
