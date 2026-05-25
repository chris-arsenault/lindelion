fn build_editor(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        glirdir_top_strip(cx, signals);
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                glirdir_capture_section(cx, signals);
                glirdir_detection_section(cx, signals);
            })
            .width(Pixels(280.0))
            .height(Stretch(1.0))
            .vertical_gap(Pixels(10.0));
            glirdir_preview_section(cx, signals);
            VStack::new(cx, move |cx| {
                glirdir_quantize_section(cx, signals);
                glirdir_audition_section(cx, signals);
            })
            .width(Pixels(280.0))
            .height(Stretch(1.0))
            .vertical_gap(Pixels(10.0));
        })
        .height(Stretch(1.0))
        .horizontal_gap(Pixels(10.0));
    })
    .class("root")
    .class("ll-shell")
    .padding(Pixels(12.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

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
        cx.add_stylesheet(crate::vizia_controls::COMMON_CONTROL_STYLE)
            .expect("failed to add common control style");
        let signals = EditorSignals {
            host,
            parent_view,
            parameters: EditorParameterSignals::new(values.parameters),
            status: Signal::new(values.status),
            preview: Signal::new(values.preview.clone()),
            command_status: Signal::new(values.command_status),
        };
        EditorModel {
            host,
            signals,
            pending_export: None,
        }
        .build(cx);
        let sync_timer = cx.add_timer(Duration::from_millis(66), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(EditorEvent::SyncFromController);
            }
        });
        cx.start_timer(sync_timer);
        build_editor(cx, signals);
    })
    .ignore_default_theme()
    .title("Glirdir")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

fn glirdir_top_strip(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "GLIRDIR").class("title");
            Label::new(cx, command_status_text(signals.command_status)).class("muted");
        })
        .width(Stretch(1.0))
        .vertical_gap(Pixels(2.0));
        glirdir_status_chip(cx, capture_state_text(signals.status));
        glirdir_status_chip(cx, analysis_state_text(signals.status));
        glirdir_status_chip(cx, library_state_text(signals.status));
        HStack::new(cx, move |cx| {
            glirdir_tool_button(cx, ICON_MICROPHONE, "Arm capture", GlirdirEditorCommand::ArmCapture);
            glirdir_tool_button(cx, ICON_TRASH, "Clear scratchpad", GlirdirEditorCommand::ClearScratchpad);
            glirdir_tool_button(cx, ICON_DEVICE_FLOPPY, "Save scratchpad to library", GlirdirEditorCommand::SaveScratchpadToLibrary);
            glirdir_export_button(cx);
        })
        .horizontal_gap(Pixels(5.0));
    })
    .class("topbar")
    .class("ll-top-strip")
    .height(Pixels(58.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn glirdir_capture_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Capture",
            "host sync and scratch input",
            crate::vizia_controls::Accent::Transport,
        );
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::CaptureBars));
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::SyncMode));
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::CountIn));
        HStack::new(cx, move |cx| {
            glirdir_tool_button(cx, ICON_PLAYER_PLAY, "Finalize capture", GlirdirEditorCommand::FinalizeCapture);
            glirdir_tool_button(cx, ICON_ACTIVITY, "Toggle live edit", GlirdirEditorCommand::ToggleLiveEdit);
            glirdir_tool_button(cx, ICON_REPEAT, "Toggle audition loop", GlirdirEditorCommand::ToggleLoop);
        })
        .horizontal_gap(Pixels(5.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-transport")
    .height(Pixels(178.0))
    .vertical_gap(Pixels(8.0));
}

fn glirdir_detection_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Detection",
            "confidence, onset, length",
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Confidence));
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::OnsetSensitivity));
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::MinNote));
        })
        .horizontal_gap(Pixels(4.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-audio")
    .height(Pixels(148.0))
    .vertical_gap(Pixels(8.0));
}

fn glirdir_preview_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::section_header(
            cx,
            "Preview",
            preview_detail_text(signals.preview),
            crate::vizia_controls::Accent::Audio,
        );
        VStack::new(cx, move |cx| {
            WaveformPreviewView::new(cx, signals.preview)
                .class("preview")
                .class("ll-visual-frame")
                .class("ll-visual-audio")
                .height(Pixels(178.0));
            PianoRollPreviewView::new(cx, signals.preview, signals.host, signals.parent_view)
                .class("preview")
                .class("ll-visual-frame")
                .class("ll-visual-mod")
                .height(Stretch(1.0));
        })
        .vertical_gap(Pixels(8.0))
        .height(Stretch(1.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-audio")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn glirdir_quantize_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Quantize",
            "key, grid, timing",
            crate::vizia_controls::Accent::Mod,
        );
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Root));
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Scale));
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Snap));
        glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::Grid));
        HStack::new(cx, move |cx| {
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::TimingStrength));
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::VelocityAmount));
        })
        .horizontal_gap(Pixels(4.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-mod")
    .height(Pixels(336.0))
    .vertical_gap(Pixels(7.0));
}

fn glirdir_audition_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Audition / Export",
            "listen and move MIDI",
            crate::vizia_controls::Accent::Transport,
        );
        HStack::new(cx, move |cx| {
            glirdir_parameter_control(cx, signals.parameter(GlirdirEditorSurfaceSlot::AuditionVolume));
            VStack::new(cx, move |cx| {
                HStack::new(cx, move |cx| {
                    glirdir_tool_button(cx, ICON_PLAYER_PLAY, "Play audition", GlirdirEditorCommand::PlayAudition);
                    glirdir_tool_button(cx, ICON_PLAYER_STOP, "Stop audition", GlirdirEditorCommand::StopAudition);
                    glirdir_export_button(cx);
                })
                .horizontal_gap(Pixels(5.0));
                Label::new(cx, "drag piano roll or export")
                    .class("ll-section-subtitle")
                    .height(Pixels(18.0));
            })
            .width(Stretch(1.0))
            .vertical_gap(Pixels(8.0));
        })
        .horizontal_gap(Pixels(8.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-transport")
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn capture_state_text(status: Signal<GlirdirEditorStatus>) -> Memo<String> {
    Memo::new(move |_| match status.get().capture_state {
        GlirdirEditorCaptureState::Idle => "capture idle",
        GlirdirEditorCaptureState::Armed => "capture armed",
        GlirdirEditorCaptureState::CountIn => "count-in",
        GlirdirEditorCaptureState::Capturing => "capturing",
        GlirdirEditorCaptureState::Captured => "captured",
    }
    .to_string())
}

fn analysis_state_text(status: Signal<GlirdirEditorStatus>) -> Memo<String> {
    Memo::new(move |_| match status.get().analysis_status {
        GlirdirEditorAnalysisStatus::Idle => "analysis idle",
        GlirdirEditorAnalysisStatus::Capturing => "input live",
        GlirdirEditorAnalysisStatus::CapturedPendingAnalysis => "pending analysis",
        GlirdirEditorAnalysisStatus::Analyzing => "analyzing",
        GlirdirEditorAnalysisStatus::Ready => "midi ready",
        GlirdirEditorAnalysisStatus::Error => "analysis error",
    }
    .to_string())
}

fn library_state_text(status: Signal<GlirdirEditorStatus>) -> Memo<String> {
    Memo::new(move |_| match status.get().library_status {
        GlirdirEditorLibraryStatus::Idle => "library idle",
        GlirdirEditorLibraryStatus::Saving => "saving",
        GlirdirEditorLibraryStatus::Saved => "saved",
        GlirdirEditorLibraryStatus::EmptyScratchpad => "empty scratchpad",
        GlirdirEditorLibraryStatus::Error => "library error",
    }
    .to_string())
}

fn command_status_text(command: Signal<Option<GlirdirEditorCommand>>) -> Memo<String> {
    Memo::new(move |_| match command.get() {
        Some(GlirdirEditorCommand::ArmCapture) => "Arm requested",
        Some(GlirdirEditorCommand::ClearScratchpad) => "Scratchpad cleared",
        Some(GlirdirEditorCommand::FinalizeCapture) => "Capture finalized",
        Some(GlirdirEditorCommand::PlayAudition) => "Audition playing",
        Some(GlirdirEditorCommand::StopAudition) => "Audition stopped",
        Some(GlirdirEditorCommand::ToggleLoop) => "Loop toggled",
        Some(GlirdirEditorCommand::ToggleLiveEdit) => "Live edit toggled",
        Some(GlirdirEditorCommand::SaveScratchpadToLibrary) => "Scratchpad saved",
        Some(GlirdirEditorCommand::ExportMidi) => "MIDI export requested",
        None => "Scratch audio, analyze melody, drag MIDI",
    }
    .to_string())
}

fn preview_detail_text(preview: Signal<GlirdirEditorPreview>) -> Memo<String> {
    Memo::new(move |_| {
        let preview = preview.get();
        format!(
            "{} waveform points / {} notes",
            preview.waveform.points.len(),
            preview.piano_roll.notes.len()
        )
    })
}
