fn parameter_control(cx: &mut Context, parameter: EditorParameterControl) {
    match parameter.editor.control() {
        GlirdirEditorControlKind::Slider { .. } => parameter_slider(cx, parameter),
        GlirdirEditorControlKind::Segmented { labels, width } => {
            discrete_control(cx, parameter, labels, width)
        }
        GlirdirEditorControlKind::Selector { labels, width } => {
            selector_control(cx, parameter, labels, width)
        }
    }
}

fn parameter_slider(cx: &mut Context, parameter: EditorParameterControl) {
    let id = parameter.id;
    let signal = parameter.signal;
    HStack::new(cx, move |cx| {
        Label::new(cx, parameter.editor.label())
            .class("meter-label")
            .width(Pixels(66.0));
        Slider::new(cx, signal)
            .range(0.0..1.0)
            .on_change(move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter { id, normalized });
            })
            .width(Stretch(1.0));
        Label::new(cx, parameter.value_text())
            .class("value-label")
            .width(Pixels(58.0));
    })
    .height(Pixels(22.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

fn discrete_control(
    cx: &mut Context,
    parameter: EditorParameterControl,
    labels: &'static [&'static str],
    width: f32,
) {
    VStack::new(cx, move |cx| {
        Label::new(cx, parameter.editor.label()).class("meter-label");
        HStack::new(cx, move |cx| {
            for (index, label) in labels.iter().copied().enumerate() {
                discrete_button(cx, parameter, labels.len(), index, label);
            }
        })
        .class("segmented")
        .height(Pixels(26.0))
        .width(Pixels(width))
        .horizontal_gap(Pixels(2.0));
    })
    .height(Pixels(42.0))
    .vertical_gap(Pixels(2.0));
}

fn discrete_button(
    cx: &mut Context,
    parameter: EditorParameterControl,
    count: usize,
    index: usize,
    label: &'static str,
) {
    let id = parameter.id;
    let normalized = normalized_for_index(index, count);
    Button::new(cx, move |cx| Label::new(cx, label).alignment(Alignment::Center))
        .on_press(move |cx| {
            cx.emit(EditorEvent::SetParameter { id, normalized });
        })
        .class("seg-button")
        .toggle_class(
            "seg-active",
            parameter
                .signal
                .map(move |value| selected_index(*value, count) == Some(index)),
        )
        .width(Stretch(1.0))
        .height(Stretch(1.0));
}

fn selector_control(
    cx: &mut Context,
    parameter: EditorParameterControl,
    labels: &'static [&'static str],
    width: f32,
) {
    let id = parameter.id;
    let count = labels.len();
    let list = Signal::new(labels.iter().map(|label| (*label).to_string()).collect::<Vec<_>>());
    let selected = parameter
        .signal
        .map(move |value| selected_index(*value, count));

    VStack::new(cx, move |cx| {
        Label::new(cx, parameter.editor.label()).class("meter-label");
        Select::new(cx, list, selected, true)
            .placeholder(parameter.editor.label())
            .on_select(move |cx, index| {
                cx.emit(EditorEvent::SetParameter {
                    id,
                    normalized: normalized_for_index(index, count),
                });
            })
            .width(Pixels(width))
            .height(Pixels(28.0));
    })
    .height(Pixels(42.0))
    .vertical_gap(Pixels(2.0));
}

fn icon_button(
    cx: &mut Context,
    icon: &'static str,
    tooltip: &'static str,
    command: GlirdirEditorCommand,
) {
    Button::new(cx, move |cx| Svg::new(cx, icon).class("toolbar-icon"))
        .on_press(move |cx| {
            cx.emit(EditorEvent::Command(command));
        })
        .class("toolbar-button")
        .width(Pixels(34.0))
        .height(Pixels(30.0))
        .tooltip(move |cx| {
            Tooltip::new(cx, move |cx| {
                Label::new(cx, tooltip).padding(Pixels(5.0));
            })
            .class("tooltip")
            .padding(Pixels(3.0))
            .size(Auto)
            .placement(Placement::Bottom)
        });
}

fn export_file_button(cx: &mut Context, icon: &'static str, tooltip: &'static str) {
    Button::new(cx, move |cx| Svg::new(cx, icon).class("toolbar-icon"))
        .on_press(move |cx| {
            cx.emit(EditorEvent::ExportMidiFile);
        })
        .class("toolbar-button")
        .width(Pixels(34.0))
        .height(Pixels(30.0))
        .tooltip(move |cx| {
            Tooltip::new(cx, move |cx| {
                Label::new(cx, tooltip).padding(Pixels(5.0));
            })
            .class("tooltip")
            .padding(Pixels(3.0))
            .size(Auto)
            .placement(Placement::Bottom)
        });
}

fn status_chip(cx: &mut Context, label: &'static str, value: Memo<String>) {
    VStack::new(cx, move |cx| {
        Label::new(cx, label).class("meter-label");
        Element::new(cx)
            .class("status-chip")
            .height(Pixels(24.0))
            .width(Pixels(142.0))
            .text(value);
    })
    .width(Pixels(146.0))
    .vertical_gap(Pixels(2.0));
}

fn command_status(
    cx: &mut Context,
    command: Signal<Option<GlirdirEditorCommand>>,
    status: Signal<GlirdirEditorStatus>,
) {
    HStack::new(cx, |cx| {
        Svg::new(cx, ICON_DEVICE_FLOPPY).class("toolbar-icon");
        Label::new(cx, command_status_text(command, status)).class("muted");
    })
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn normalized_for_index(index: usize, count: usize) -> f32 {
    if count <= 1 {
        0.0
    } else {
        index as f32 / (count - 1) as f32
    }
}

fn selected_index(value: f32, count: usize) -> Option<usize> {
    (count > 0).then(|| (value.clamp(0.0, 1.0) * (count - 1) as f32).round() as usize)
}

fn status_summary(signal: Signal<GlirdirEditorStatus>) -> Memo<String> {
    Memo::new(move |_| {
        let status = signal.get();
        if status.has_analysis {
            "Scratchpad analyzed".to_string()
        } else if status.has_scratchpad {
            "Scratchpad captured".to_string()
        } else {
            "Ready for capture".to_string()
        }
    })
}

fn capture_status_text(signal: Signal<GlirdirEditorStatus>) -> Memo<String> {
    Memo::new(move |_| match signal.get().capture_state {
        GlirdirEditorCaptureState::Idle => "Idle",
        GlirdirEditorCaptureState::Armed => "Armed",
        GlirdirEditorCaptureState::CountIn => "Count-in",
        GlirdirEditorCaptureState::Capturing => "Capturing",
        GlirdirEditorCaptureState::Captured => "Captured",
    }
    .to_string())
}

fn analysis_status_text(signal: Signal<GlirdirEditorStatus>) -> Memo<String> {
    Memo::new(move |_| match signal.get().analysis_status {
        GlirdirEditorAnalysisStatus::Idle => "Idle",
        GlirdirEditorAnalysisStatus::Capturing => "Capturing",
        GlirdirEditorAnalysisStatus::CapturedPendingAnalysis => "Pending",
        GlirdirEditorAnalysisStatus::Analyzing => "Analyzing",
        GlirdirEditorAnalysisStatus::Ready => "Ready",
        GlirdirEditorAnalysisStatus::Error => "Error",
    }
    .to_string())
}

fn midi_preview_text(signal: Signal<GlirdirEditorPreview>) -> Memo<String> {
    Memo::new(move |_| {
        let preview = signal.get();
        format!(
            "{} notes, {} bpm",
            preview.piano_roll.notes.len(),
            preview.piano_roll.bpm
        )
    })
}

fn command_status_text(
    command: Signal<Option<GlirdirEditorCommand>>,
    status: Signal<GlirdirEditorStatus>,
) -> Memo<String> {
    Memo::new(move |_| match command.get() {
        Some(GlirdirEditorCommand::ArmCapture) => "Arm sent",
        Some(GlirdirEditorCommand::ClearScratchpad) => "Clear sent",
        Some(GlirdirEditorCommand::FinalizeCapture) => "Analysis requested",
        Some(GlirdirEditorCommand::PlayAudition) => "Audition playing",
        Some(GlirdirEditorCommand::StopAudition) => "Audition stopped",
        Some(GlirdirEditorCommand::ToggleLoop) => "Loop toggled",
        Some(GlirdirEditorCommand::ToggleLiveEdit) => "Live edit toggled",
        Some(GlirdirEditorCommand::SaveScratchpadToLibrary) => {
            library_status_text(status.get().library_status)
        }
        Some(GlirdirEditorCommand::ExportMidi) => "MIDI export requested",
        None => "No command",
    }
    .to_string())
}

fn library_status_text(status: GlirdirEditorLibraryStatus) -> &'static str {
    match status {
        GlirdirEditorLibraryStatus::Idle => "Save requested",
        GlirdirEditorLibraryStatus::Saving => "Saving scratchpad",
        GlirdirEditorLibraryStatus::Saved => "Saved to library",
        GlirdirEditorLibraryStatus::EmptyScratchpad => "No scratchpad to save",
        GlirdirEditorLibraryStatus::Error => "Save failed",
    }
}
