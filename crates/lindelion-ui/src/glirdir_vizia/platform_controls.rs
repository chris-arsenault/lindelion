fn glirdir_parameter_control(cx: &mut Context, control: EditorParameterControl) {
    match control.editor.control() {
        GlirdirEditorControlKind::Knob => glirdir_knob(cx, control, false),
        GlirdirEditorControlKind::Slider { .. } => glirdir_knob(cx, control, false),
        GlirdirEditorControlKind::Binary {
            left_label,
            right_label,
            width,
        } => crate::vizia_controls::inline_binary_segmented(
            cx,
            control.editor.label(),
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
        GlirdirEditorControlKind::Segmented { labels, width } => {
            crate::vizia_controls::inline_parameter_segmented(
                cx,
                control.editor.label(),
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
        GlirdirEditorControlKind::Selector { labels, .. } => crate::vizia_controls::parameter_stepper(
            cx,
            control.editor.label(),
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

fn glirdir_knob(cx: &mut Context, control: EditorParameterControl, centered: bool) {
    crate::vizia_controls::parameter_knob(
        cx,
        control.editor.label(),
        control.value_text(),
        control.signal,
        control.default_normalized(),
        centered,
        crate::vizia_controls::Accent::Audio,
        move |cx, normalized| {
            cx.emit(EditorEvent::SetParameter {
                id: control.id,
                normalized,
            });
        },
    );
}

fn glirdir_tool_button(
    cx: &mut Context,
    icon: &'static str,
    tooltip: &'static str,
    command: GlirdirEditorCommand,
) {
    crate::vizia_controls::icon_tool_button(cx, icon, tooltip)
        .on_press(move |cx| cx.emit(EditorEvent::Command(command)));
}

fn glirdir_export_button(cx: &mut Context) {
    crate::vizia_controls::icon_tool_button(cx, ICON_DOWNLOAD, "Export MIDI file")
        .on_press(|cx| cx.emit(EditorEvent::ExportMidiFile));
}

fn glirdir_status_chip<T>(cx: &mut Context, text: T)
where
    T: Res<String> + Clone + 'static,
{
    crate::vizia_controls::status_chip(cx, text, crate::vizia_controls::ChipKind::Neutral);
}
