fn linnod_settings_overlay(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        settings_header(cx, "Linnod Settings", EditorEvent::CloseSettings);
        crate::vizia_controls::section_header(
            cx,
            "Pitch Engine",
            pitch_shift_algorithm_detail(signals.summary),
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            pitch_shift_algorithm_button(
                cx,
                signals.summary,
                "Peak",
                LinnodEditorPitchShiftAlgorithm::SpectralPeak,
            );
            pitch_shift_algorithm_button(
                cx,
                signals.summary,
                "Speed",
                LinnodEditorPitchShiftAlgorithm::Varispeed,
            );
            pitch_shift_algorithm_button(
                cx,
                signals.summary,
                "Stretch",
                LinnodEditorPitchShiftAlgorithm::TimeStretch,
            );
            pitch_shift_algorithm_button(
                cx,
                signals.summary,
                "Pro",
                LinnodEditorPitchShiftAlgorithm::ResampleStretch,
            );
        })
        .class("segmented")
        .class("ll-segmented")
        .height(Pixels(28.0))
        .horizontal_gap(Pixels(2.0));
    })
    .class("ll-settings-panel")
    .width(Pixels(460.0))
    .height(Pixels(156.0))
    .position_type(PositionType::Absolute)
    .right(Pixels(18.0))
    .top(Pixels(84.0))
    .z_index(20)
    .vertical_gap(Pixels(10.0))
    .display(settings_display(signals.settings_open));
}

fn settings_header(cx: &mut Context, title: &'static str, close_event: EditorEvent) {
    HStack::new(cx, move |cx| {
        Label::new(cx, title).class("ll-settings-title");
        Spacer::new(cx);
        crate::vizia_controls::icon_tool_button(cx, ICON_X, "Close settings").on_press(move |cx| {
            cx.emit(close_event.clone());
        });
    })
    .height(Pixels(28.0))
    .alignment(Alignment::Center);
}

fn pitch_shift_algorithm_button(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    label: &'static str,
    algorithm: LinnodEditorPitchShiftAlgorithm,
) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| cx.emit(EditorEvent::Command(LinnodEditorCommand::SetPitchShiftAlgorithm(algorithm))))
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class(
        "seg-active",
        summary.map(move |summary| summary.pitch_shift_algorithm == algorithm),
    )
    .toggle_class(
        "ll-seg-active",
        summary.map(move |summary| summary.pitch_shift_algorithm == algorithm),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn pitch_shift_algorithm_detail(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| match summary.get().pitch_shift_algorithm {
        LinnodEditorPitchShiftAlgorithm::SpectralPeak => {
            "spectral peak, fixed duration".to_string()
        }
        LinnodEditorPitchShiftAlgorithm::Varispeed => {
            "simple playback speed, duration changes".to_string()
        }
        LinnodEditorPitchShiftAlgorithm::TimeStretch => {
            "time-domain stretch, fixed duration".to_string()
        }
        LinnodEditorPitchShiftAlgorithm::ResampleStretch => {
            "resample/stretch, formant-preserving".to_string()
        }
    })
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
