fn build_application(
    host: GlirdirEditorHost,
    values: EditorValues,
    size: GlirdirEditorSize,
    parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(GLIRDIR_EDITOR_WIDTH) as u32;
    let height = size.height.max(GLIRDIR_EDITOR_HEIGHT) as u32;

    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add glirdir editor style");

        let signals = EditorSignals {
            host,
            parent_view,
            parameters: EditorParameterSignals::new(values.parameters),
            status: Signal::new(values.status),
            preview: Signal::new(values.preview.clone()),
            command_status: Signal::new(values.command_status),
        };
        EditorModel { host, signals }.build(cx);
        build_editor(cx, signals);
    })
    .ignore_default_theme()
    .title("Glirdir")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

fn build_editor(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, |cx| {
        top_bar(cx, signals);
        HStack::new(cx, |cx| {
            left_column(cx, signals);
            preview_column(cx, signals);
        })
        .height(Pixels(542.0))
        .horizontal_gap(Pixels(12.0));
    })
    .class("root")
    .size(Stretch(1.0))
    .padding(Pixels(14.0))
    .vertical_gap(Pixels(10.0));
}

fn top_bar(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, |cx| {
        VStack::new(cx, |cx| {
            Label::new(cx, "Glirdir").class("title");
            Label::new(cx, status_summary(signals.status)).class("muted");
        })
        .width(Pixels(238.0))
        .vertical_gap(Pixels(2.0));

        status_chip(cx, "Capture", capture_status_text(signals.status));
        status_chip(cx, "Analysis", analysis_status_text(signals.status));
        Spacer::new(cx);
        icon_button(cx, ICON_LIBRARY, "Save scratchpad", GlirdirEditorCommand::SaveScratchpadToLibrary);
        export_file_button(cx, ICON_DOWNLOAD, "Export MIDI file");
    })
    .class("topbar")
    .height(Pixels(58.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(14.0));
}

fn left_column(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, |cx| {
        capture_panel(cx, signals);
        quantize_panel(cx, signals);
        audition_panel(cx, signals);
        detection_panel(cx, signals);
    })
    .width(Pixels(300.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(12.0));
}

fn preview_column(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, |cx| {
        HStack::new(cx, |cx| {
            Svg::new(cx, ICON_WAVE_SINE).class("toolbar-icon");
            Label::new(cx, "Audio + MIDI Preview").class("section-title");
            Spacer::new(cx);
            export_file_button(cx, ICON_FILE_MUSIC, "Export MIDI file");
        })
        .height(Pixels(24.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));

        WaveformPreviewView::new(cx, signals.preview)
            .class("preview")
            .height(Pixels(204.0))
            .width(Stretch(1.0));
        PianoRollPreviewView::new(cx, signals.preview, signals.host, signals.parent_view)
            .class("preview")
            .height(Pixels(244.0))
            .width(Stretch(1.0));

        HStack::new(cx, |cx| {
            command_status(cx, signals.command_status, signals.status);
            Spacer::new(cx);
            Label::new(cx, midi_preview_text(signals.preview)).class("muted");
        })
        .height(Pixels(28.0))
        .alignment(Alignment::Center);
    })
    .class("panel")
    .width(Pixels(620.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

fn capture_panel(cx: &mut Context, signals: EditorSignals) {
    panel(cx, "Transport / Capture", ICON_MICROPHONE, |cx| {
        HStack::new(cx, |cx| {
            icon_button(cx, ICON_ACTIVITY, "Arm capture", GlirdirEditorCommand::ArmCapture);
            icon_button(cx, ICON_ADJUSTMENTS_HORIZONTAL, "Analyze", GlirdirEditorCommand::FinalizeCapture);
            icon_button(cx, ICON_TRASH, "Clear", GlirdirEditorCommand::ClearScratchpad);
        })
        .height(Pixels(32.0))
        .horizontal_gap(Pixels(8.0));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::CaptureBars));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::SyncMode));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::CountIn));
    })
    .height(Pixels(150.0));
}

fn quantize_panel(cx: &mut Context, signals: EditorSignals) {
    panel(cx, "Quantize", ICON_ADJUSTMENTS_HORIZONTAL, |cx| {
        HStack::new(cx, |cx| {
            parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Root));
            parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Scale));
        })
        .height(Pixels(42.0))
        .horizontal_gap(Pixels(8.0));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Snap));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Grid));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::TimingStrength));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::VelocityAmount));
    })
    .height(Pixels(202.0));
}

fn audition_panel(cx: &mut Context, signals: EditorSignals) {
    panel(cx, "Audition", ICON_VOLUME_2, |cx| {
        HStack::new(cx, |cx| {
            icon_button(cx, ICON_PLAYER_PLAY, "Play audition", GlirdirEditorCommand::PlayAudition);
            icon_button(cx, ICON_PLAYER_STOP, "Stop audition", GlirdirEditorCommand::StopAudition);
            icon_button(cx, ICON_REPEAT, "Toggle loop", GlirdirEditorCommand::ToggleLoop);
            icon_button(cx, ICON_ACTIVITY, "Toggle live edit", GlirdirEditorCommand::ToggleLiveEdit);
        })
        .height(Pixels(32.0))
        .horizontal_gap(Pixels(8.0));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::AuditionVolume));
    })
    .height(Pixels(98.0));
}

fn detection_panel(cx: &mut Context, signals: EditorSignals) {
    panel(cx, "Detection", ICON_FILTER, |cx| {
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Confidence));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::OnsetSensitivity));
        parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::MinNote));
    })
    .height(Pixels(116.0));
}

fn panel<'a, F>(
    cx: &'a mut Context,
    title: &'static str,
    icon: &'static str,
    content: F,
) -> Handle<'a, VStack>
where
    F: FnOnce(&mut Context),
{
    VStack::new(cx, move |cx| {
        HStack::new(cx, |cx| {
            Svg::new(cx, icon).class("toolbar-icon");
            Label::new(cx, title).class("section-title");
            Spacer::new(cx);
        })
        .height(Pixels(20.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));
        content(cx);
    })
    .class("panel")
    .width(Stretch(1.0))
    .vertical_gap(Pixels(8.0))
}
