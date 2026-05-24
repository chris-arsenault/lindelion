fn live_input_column(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, |cx| {
        HStack::new(cx, |cx| {
            Svg::new(cx, ICON_ACTIVITY).class("toolbar-icon");
            Label::new(cx, "Live Input").class("section-title");
            Spacer::new(cx);
            Label::new(cx, sidechain_status_text(signals)).class("muted");
        })
        .height(Pixels(22.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Element::new(cx)
                    .class("chip")
                    .toggle_class("chip-on", signals.sidechain_input_detected)
                    .width(Pixels(52.0))
                    .height(Pixels(20.0))
                    .text("Input");
                Element::new(cx)
                    .class("chip")
                    .toggle_class("chip-on", signals.sidechain_signal_active)
                    .toggle_class("chip-warm", sidechain_warning(signals))
                    .width(Pixels(52.0))
                    .height(Pixels(20.0))
                    .text("Signal");
                Element::new(cx)
                    .class("chip")
                    .toggle_class("chip-on", signals.audio_note_detected)
                    .width(Pixels(48.0))
                    .height(Pixels(20.0))
                    .text("Note");
                Label::new(
                    cx,
                    pitch_confidence_text(
                        signals.audio_note_detected,
                        signals.audio_note_pitch_confidence,
                    ),
                )
                .class("value-label")
                .width(Stretch(1.0));
            })
            .height(Pixels(24.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(6.0));

            parameter_segmented(cx, signals.parameter(ResonatorEditorSurfaceSlot::AudioInputMode));
            parameter_segmented(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationMode),
            );
        })
        .class("strip")
        .height(Pixels(100.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(7.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Note Detection").class("value-label");
                Spacer::new(cx);
                Label::new(cx, "Audio").class("muted");
            })
            .height(Pixels(18.0))
            .alignment(Alignment::Center);
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteOnsetSensitivity),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteReleaseFloor),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteMinimumLength),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNotePitchConfidence),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteVelocityAmount),
            );
        })
        .class("strip")
        .height(Pixels(128.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(6.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Expression").class("value-label");
                Spacer::new(cx);
                binary_switch(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionEnable),
                );
            })
            .height(Pixels(26.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(8.0));
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPitchRange),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPressureFloor),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPressureCeiling),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionBrightnessFloor),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionBrightnessCeiling),
            );
        })
        .class("strip")
        .height(Pixels(136.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(6.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Excitation").class("value-label");
                Spacer::new(cx);
                let gain = signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationGain);
                Label::new(cx, value_text(gain)).class("value-label");
            })
            .height(Pixels(18.0))
            .alignment(Alignment::Center);
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationGain),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchWindow),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchPreRoll),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchFade),
            );
        })
        .class("strip")
        .height(Pixels(116.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(6.0));
    })
    .class("panel")
    .width(Pixels(260.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}
