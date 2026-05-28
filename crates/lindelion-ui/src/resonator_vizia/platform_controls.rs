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
        } => resonator_binary_control(cx, control, left_label, right_label, width),
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
        ResonatorEditorControlKind::Selector { labels, width } => crate::vizia_controls::parameter_cycle_selector(
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

fn resonator_binary_control(
    cx: &mut Context,
    control: EditorParameterControl,
    left_label: &'static str,
    right_label: &'static str,
    width: f32,
) {
    if let Some(choices) = resonator_icon_choices(control.editor.slot()) {
        crate::vizia_controls::inline_icon_segmented(
            cx,
            control.label(),
            control.signal,
            choices,
            62.0,
            move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter {
                    id: control.id,
                    normalized,
                });
            },
        );
        return;
    }

    crate::vizia_controls::inline_binary_segmented(
        cx,
        control.label(),
        control.signal,
        left_label,
        right_label,
        width.min(104.0),
        move |cx, normalized| {
            cx.emit(EditorEvent::SetParameter {
                id: control.id,
                normalized,
            });
        },
    );
}

fn resonator_compact_binary_control(cx: &mut Context, control: EditorParameterControl) {
    let ResonatorEditorControlKind::Binary {
        left_label,
        right_label,
        width,
    } = control.editor.control()
    else {
        resonator_parameter_control(cx, control, crate::vizia_controls::Accent::Tone);
        return;
    };

    crate::vizia_controls::compact_binary_segmented(
        cx,
        control.signal,
        left_label,
        right_label,
        width.min(118.0),
        move |cx, normalized| {
            cx.emit(EditorEvent::SetParameter {
                id: control.id,
                normalized,
            });
        },
    );
}

fn resonator_mix_control(cx: &mut Context, control: EditorParameterControl) {
    let value_text = mix_balance_text(control.signal);
    VStack::new(cx, move |cx| {
        Knob::new(
            cx,
            unsafe { control.host.default_normalized(control.id) },
            control.signal,
            false,
        )
        .on_change(move |cx, normalized| {
            cx.emit(EditorEvent::SetParameter {
                id: control.id,
                normalized,
            });
        })
        .class("ll-knob")
        .class("ll-knob-tone");
        Label::new(cx, control.label())
            .class("ll-control-label")
            .class("ll-knob-label")
            .alignment(Alignment::Center)
            .width(Pixels(62.0));
        Label::new(cx, value_text.clone())
            .class("ll-control-value")
            .class("ll-knob-value")
            .alignment(Alignment::Center)
            .width(Pixels(62.0));
        HStack::new(cx, move |cx| {
            Label::new(cx, "A").class("ll-range-mark");
            Spacer::new(cx);
            Label::new(cx, "=").class("ll-range-mark");
            Spacer::new(cx);
            Label::new(cx, "B").class("ll-range-mark");
        })
        .width(Pixels(54.0))
        .height(Pixels(7.0))
        .alignment(Alignment::Center);
    })
    .class("ll-knob-cell")
    .width(Pixels(66.0))
    .height(Pixels(66.0))
    .alignment(Alignment::Center)
    .vertical_gap(Pixels(1.0));
}

fn mix_balance_text(signal: Signal<f32>) -> Memo<String> {
    Memo::new(move |_| {
        let mix_b = signal.get().clamp(0.0, 1.0);
        if (mix_b - 0.5).abs() < 0.005 {
            "Equal".to_string()
        } else if mix_b < 0.5 {
            format!("A {:.0}", (1.0 - mix_b) * 100.0)
        } else {
            format!("B {:.0}", mix_b * 100.0)
        }
    })
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

const ROUTING_CHOICES: &[crate::vizia_controls::IconSegmentedChoice] = &[
    crate::vizia_controls::IconSegmentedChoice::new(ICON_ARROWS_SPLIT, "Parallel routing"),
    crate::vizia_controls::IconSegmentedChoice::new(ICON_ARROW_MERGE, "Series routing"),
];
const RETRIGGER_CHOICES: &[crate::vizia_controls::IconSegmentedChoice] = &[
    crate::vizia_controls::IconSegmentedChoice::new(ICON_ARROW_FORWARD, "Carry resonator state"),
    crate::vizia_controls::IconSegmentedChoice::new(ICON_REPEAT, "Retrigger resonators"),
];

fn resonator_icon_choices(
    slot: ResonatorEditorSurfaceSlot,
) -> Option<&'static [crate::vizia_controls::IconSegmentedChoice]> {
    match slot {
        ResonatorEditorSurfaceSlot::Routing => Some(ROUTING_CHOICES),
        ResonatorEditorSurfaceSlot::RetriggerResonators => Some(RETRIGGER_CHOICES),
        _ => None,
    }
}
