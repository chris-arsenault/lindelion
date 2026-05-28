fn glirdir_settings_overlay(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        settings_header(cx, "Glirdir Settings");
        crate::vizia_controls::static_section_header(
            cx,
            "Analysis",
            "rare detection gates",
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Confidence));
            glirdir_parameter_control(
                cx,
                signals.parameter(GlirdirEditorSurfaceSlot::OnsetSensitivity),
            );
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::MinNote));
        })
        .horizontal_gap(Pixels(4.0));
        crate::vizia_controls::static_section_header(
            cx,
            "Audition",
            "preview engine",
            crate::vizia_controls::Accent::Transport,
        );
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::AuditionVolume));
    })
    .class("ll-settings-panel")
    .width(Pixels(384.0))
    .height(Pixels(246.0))
    .position_type(PositionType::Absolute)
    .right(Pixels(18.0))
    .top(Pixels(82.0))
    .z_index(20)
    .vertical_gap(Pixels(10.0))
    .display(settings_display(signals.settings_open));
}

fn settings_header(cx: &mut Context, title: &'static str) {
    HStack::new(cx, move |cx| {
        Label::new(cx, title).class("ll-settings-title");
        Spacer::new(cx);
        crate::vizia_controls::icon_tool_button(cx, ICON_X, "Close settings")
            .on_press(|cx| cx.emit(EditorEvent::CloseSettings));
    })
    .height(Pixels(28.0))
    .alignment(Alignment::Center);
}

fn settings_display(open: Signal<bool>) -> impl Res<Display> + Clone {
    open.map(|open| {
        if *open {
            Display::Flex
        } else {
            Display::None
        }
    })
}
