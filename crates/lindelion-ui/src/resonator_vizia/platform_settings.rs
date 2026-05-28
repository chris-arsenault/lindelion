fn resonator_settings_overlay(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        settings_header(cx, "Lamath Settings");
        crate::vizia_controls::static_section_header(
            cx,
            "Live Excitation",
            "sidechain capture engine",
            crate::vizia_controls::Accent::Audio,
        );
        resonator_parameter_control(
            cx,
            signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationMode),
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchWindow),
                crate::vizia_controls::Accent::Audio,
            );
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchPreRoll),
                crate::vizia_controls::Accent::Audio,
            );
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchFade),
                crate::vizia_controls::Accent::Audio,
            );
        })
        .horizontal_gap(Pixels(4.0));
        crate::vizia_controls::static_section_header(
            cx,
            "Audio Expression",
            "sidechain mapping",
            crate::vizia_controls::Accent::Mod,
        );
        HStack::new(cx, move |cx| {
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPitchRange),
                crate::vizia_controls::Accent::Mod,
            );
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNotePitchConfidence),
                crate::vizia_controls::Accent::Mod,
            );
        })
        .horizontal_gap(Pixels(4.0));
    })
    .class("ll-settings-panel")
    .width(Pixels(420.0))
    .height(Pixels(378.0))
    .position_type(PositionType::Absolute)
    .right(Pixels(18.0))
    .top(Pixels(84.0))
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
