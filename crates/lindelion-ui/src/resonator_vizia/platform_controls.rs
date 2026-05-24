fn binary_switch(cx: &mut Context, parameter: EditorParameterControl) {
    segmented_switch(cx, parameter);
}

fn compact_binary_switch(cx: &mut Context, parameter: EditorParameterControl) {
    segmented_switch(cx, parameter);
}

fn parameter_segmented(cx: &mut Context, parameter: EditorParameterControl) {
    HStack::new(cx, |cx| {
        Label::new(cx, parameter.label())
            .class("meter-label")
            .width(Pixels(48.0));
        segmented_switch(cx, parameter);
    })
    .height(Pixels(26.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

fn segmented_switch(cx: &mut Context, parameter: EditorParameterControl) {
    let id = parameter.id;
    let signal = parameter.signal;
    let control = parameter.editor.control();
    let (labels, width) = match control {
        ResonatorEditorControlKind::Binary {
            left_label,
            right_label,
            width,
        } => {
            HStack::new(cx, move |cx| {
                segmented_switch_button(cx, id, signal, 0.0, 0.5, left_label);
                segmented_switch_button(cx, id, signal, 1.0, 0.5, right_label);
            })
            .class("segmented")
            .height(Pixels(26.0))
            .width(Pixels(width))
            .horizontal_gap(Pixels(2.0));
            return;
        }
        ResonatorEditorControlKind::Segmented { labels, width }
        | ResonatorEditorControlKind::Selector { labels, width } => (labels, width),
        ResonatorEditorControlKind::Knob | ResonatorEditorControlKind::Slider { .. } => {
            panic!("segmented switch requires segmented editor metadata")
        }
    };
    segmented_buttons(cx, id, signal, labels, width);
}

fn segmented_buttons(
    cx: &mut Context,
    id: u32,
    signal: Signal<f32>,
    labels: &[&'static str],
    width: f32,
) {
    HStack::new(cx, move |cx| {
        let denominator = labels.len().saturating_sub(1).max(1) as f32;
        let tolerance = 0.5 / denominator;
        for (index, label) in labels.iter().copied().enumerate() {
            let normalized = index as f32 / denominator;
            segmented_switch_button(cx, id, signal, normalized, tolerance, label);
        }
    })
    .class("segmented")
    .height(Pixels(26.0))
    .width(Pixels(width))
    .horizontal_gap(Pixels(2.0));
}

fn segmented_switch_button(
    cx: &mut Context,
    id: u32,
    signal: Signal<f32>,
    normalized: f32,
    tolerance: f32,
    label: &'static str,
) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| {
        cx.emit(EditorEvent::SetParameter { id, normalized });
    })
    .class("seg-button")
    .toggle_class(
        "seg-active",
        signal.map(move |value| (value - normalized).abs() <= tolerance),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn parameter_knob(cx: &mut Context, parameter: EditorParameterControl) {
    let id = parameter.id;
    let signal = parameter.signal;
    let label = parameter.label();
    VStack::new(cx, move |cx| {
        Knob::new(cx, default_normalized(parameter.host, id), signal, false).on_change(
            move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter { id, normalized });
            },
        );
        Label::new(cx, label)
            .class("value-label")
            .alignment(Alignment::Center)
            .width(Pixels(84.0));
        Label::new(cx, value_text(parameter))
            .class("muted")
            .alignment(Alignment::Center)
            .width(Pixels(84.0));
    })
    .width(Pixels(92.0))
    .height(Pixels(88.0))
    .alignment(Alignment::Center)
    .vertical_gap(Pixels(3.0));
}

fn parameter_slider(cx: &mut Context, parameter: EditorParameterControl) {
    let id = parameter.id;
    let signal = parameter.signal;
    let label = parameter.label();
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("meter-label")
            .width(Pixels(58.0));
        Slider::new(cx, signal)
            .range(0.0..1.0)
            .on_change(move |cx, normalized| {
                cx.emit(EditorEvent::SetParameter { id, normalized });
            })
            .width(Stretch(1.0));
        Label::new(cx, value_text(parameter))
            .class("value-label")
            .width(Pixels(78.0));
    })
    .height(Pixels(18.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

fn sample_drawer(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, |cx| {
        VStack::new(cx, |cx| {
            Label::new(cx, "Sample Library").class("section-title");
            Label::new(cx, library_count_text(signals.library_samples)).class("muted");
        })
        .width(Pixels(112.0))
        .vertical_gap(Pixels(2.0));

        List::new(cx, signals.library_samples, move |cx, index, item| {
            library_sample_row(cx, index, item, signals);
        })
        .class("strip")
        .width(Stretch(1.0))
        .height(Pixels(54.0));

        icon_button(cx, ICON_LIBRARY, "Open library", UiCommand::OpenLibrary);
        icon_button(
            cx,
            ICON_DOWNLOAD,
            "Load selected slot",
            UiCommand::LoadSelectedExcitationSlot,
        );
        icon_button(
            cx,
            ICON_TRASH,
            "Clear selected slot",
            UiCommand::ClearSelectedExcitationSlot,
        );
    })
    .class("panel")
    .height(Pixels(72.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(14.0));
}

fn library_sample_row(
    cx: &mut Context,
    index: usize,
    item: impl SignalGet<ResonatorEditorSampleSummary> + Copy + 'static,
    signals: EditorSignals,
) {
    Button::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            LibraryWaveform::new(cx, signals.library_samples, index)
                .width(Pixels(84.0))
                .height(Pixels(32.0));
            VStack::new(cx, move |cx| {
                Label::new(cx, Memo::new(move |_| item.get().label)).class("value-label");
                Label::new(cx, Memo::new(move |_| item.get().detail)).class("muted");
            })
            .vertical_gap(Pixels(1.0));
        })
        .horizontal_gap(Pixels(8.0))
        .alignment(Alignment::Center)
    })
    .on_press(move |cx| {
        cx.emit(EditorEvent::SelectLibrarySample(index));
    })
    .class("sample-row")
    .toggle_class(
        "sample-selected",
        signals
            .selected_sample
            .map(move |selected| selected.round() as usize == index),
    )
    .height(Pixels(46.0))
    .width(Stretch(1.0));
}

fn icon_button(
    cx: &mut Context,
    icon: &'static str,
    tooltip: &'static str,
    command: UiCommand,
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

fn value_text(parameter: EditorParameterControl) -> Memo<String> {
    parameter.value_text()
}

fn sidechain_status_text(signals: EditorSignals) -> Memo<String> {
    Memo::new(move |_| {
        let required = signals.sidechain_required.get();
        let detected = signals.sidechain_input_detected.get();
        let active = signals.sidechain_signal_active.get();
        if required && !detected {
            "Sidechain missing".to_string()
        } else if required && !active {
            "Sidechain inactive".to_string()
        } else if active {
            "Sidechain active".to_string()
        } else {
            "Sidechain idle".to_string()
        }
    })
}

fn sidechain_warning(signals: EditorSignals) -> Memo<bool> {
    Memo::new(move |_| {
        signals.sidechain_required.get()
            && (!signals.sidechain_input_detected.get() || !signals.sidechain_signal_active.get())
    })
}

fn pitch_confidence_text(note_detected: Signal<bool>, confidence: Signal<f32>) -> Memo<String> {
    Memo::new(move |_| {
        if !note_detected.get() {
            return "Conf --".to_string();
        }
        let percent = (confidence.get().clamp(0.0, 1.0) * 100.0).round() as u8;
        format!("Conf {percent}%")
    })
}

fn command_status_text(signal: Signal<Option<UiCommand>>) -> Memo<String> {
    Memo::new(move |_| command_label(signal.get()).to_string())
}

fn handle_editor_command(
    host: ResonatorEditorHost,
    command: Option<UiCommand>,
    selected_sample: Option<usize>,
) {
    let directories = unsafe { host.directories() };
    let mut patch_save_path = None;
    let mut patch_load_path = None;
    let mut patch_export_directory = None;
    let mut sample_path = None;

    match command {
        Some(UiCommand::SavePatch) => {
            patch_save_path = FileDialog::new()
                .add_filter("Lamath Patch", &["toml"])
                .set_directory(&directories.patch_directory)
                .set_file_name("Lamath Patch.toml")
                .save_file();
        }
        Some(UiCommand::LoadPatch) => {
            patch_load_path = FileDialog::new()
                .add_filter("Lamath Patch", &["toml"])
                .set_directory(&directories.patch_directory)
                .pick_file();
        }
        Some(UiCommand::ExportPatchWithSamples) => {
            patch_export_directory = FileDialog::new()
                .set_directory(&directories.export_directory)
                .pick_folder();
        }
        Some(UiCommand::OpenLibrary) => {
            sample_path = sample_dialog(&directories).pick_file();
        }
        Some(UiCommand::LoadExcitationSlot(_)) if selected_sample.is_none() => {
            sample_path = sample_dialog(&directories).pick_file();
        }
        Some(UiCommand::LoadExcitationSlot(_))
        | Some(UiCommand::ClearExcitationSlot(_))
        | Some(UiCommand::SelectExcitationSlot(_))
        | Some(UiCommand::LoadSelectedExcitationSlot)
        | Some(UiCommand::ClearSelectedExcitationSlot)
        | Some(UiCommand::RedetectSlices)
        | Some(UiCommand::TuneSelectedSlice)
        | Some(UiCommand::TuneAllSlices)
        | Some(UiCommand::SnapAllSlicesToScale)
        | None => {}
    }

    let Some(command) = command else {
        return;
    };

    unsafe {
        host.handle_command(ResonatorEditorCommandRequest {
            command,
            patch_save_path: patch_save_path.as_deref(),
            patch_load_path: patch_load_path.as_deref(),
            patch_export_directory: patch_export_directory.as_deref(),
            sample_path: sample_path.as_deref(),
            selected_library_sample: selected_sample,
        });
    }
}

fn sample_dialog(directories: &ResonatorEditorDirectories) -> FileDialog {
    FileDialog::new()
        .add_filter("WAV audio", &["wav", "wave"])
        .set_directory(&directories.sample_directory)
}

unsafe fn request_telemetry_from_controller(host: ResonatorEditorHost) {
    unsafe { host.request_telemetry() };
}

unsafe fn sync_summary_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
    let summary = unsafe { host.summary() };
    signals.patch_name.set(summary.patch_name);
    signals.slot_summaries.set(summary.slots);
    signals.library_samples.set(summary.library_samples);
}

fn update_signal(id: u32, normalized: f32, signals: EditorSignals) {
    let normalized = normalized.clamp(0.0, 1.0);
    signals.parameters.set_by_id(id, normalized);
}

unsafe fn sync_signals_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
    for binding in host.parameter_bindings().iter().flatten() {
        let parameter_id = binding.id();
        update_signal(
            parameter_id,
            unsafe { host.parameter_value(parameter_id) },
            signals,
        );
    }
}

unsafe fn sync_telemetry_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
    let telemetry = unsafe { host.telemetry() };
    signals.left_peak.set(telemetry.left_peak);
    signals.right_peak.set(telemetry.right_peak);
    signals.left_rms.set(telemetry.left_rms);
    signals.right_rms.set(telemetry.right_rms);
    signals.active_voices.set(telemetry.active_voices);
    signals.sidechain_required.set(telemetry.sidechain_required);
    signals
        .sidechain_input_detected
        .set(telemetry.sidechain_input_detected);
    signals
        .sidechain_signal_active
        .set(telemetry.sidechain_signal_active);
    signals.audio_note_detected.set(telemetry.audio_note_detected);
    signals
        .audio_note_pitch_confidence
        .set(telemetry.audio_note_pitch_confidence);
}

fn default_normalized(host: ResonatorEditorHost, parameter_id: u32) -> f32 {
    unsafe { host.default_normalized(parameter_id) }
}

fn slot_label(slots: Signal<[ResonatorEditorSlotSummary; 4]>, index: usize) -> Memo<String> {
    Memo::new(move |_| slots.get()[index].label.clone())
}

fn slot_detail(slots: Signal<[ResonatorEditorSlotSummary; 4]>, index: usize) -> Memo<String> {
    Memo::new(move |_| slots.get()[index].detail.clone())
}

fn slot_pitch_track(
    slots: Signal<[ResonatorEditorSlotSummary; 4]>,
    index: usize,
) -> Memo<bool> {
    Memo::new(move |_| slots.get()[index].pitch_track)
}

fn slot_looping(slots: Signal<[ResonatorEditorSlotSummary; 4]>, index: usize) -> Memo<bool> {
    Memo::new(move |_| slots.get()[index].looping)
}

fn slot_waveform_phase(
    slots: Signal<[ResonatorEditorSlotSummary; 4]>,
    index: usize,
) -> Memo<f32> {
    Memo::new(move |_| {
        if slots.get()[index].sample_backed {
            index as f32 * 0.17 + 0.42
        } else {
            index as f32 * 0.21 + 0.2
        }
    })
}

fn library_count_text(samples: Signal<Vec<ResonatorEditorSampleSummary>>) -> Memo<String> {
    Memo::new(move |_| {
        let count = samples.get().len();
        match count {
            0 => "No samples".to_string(),
            1 => "1 sample".to_string(),
            count => format!("{count} samples"),
        }
    })
}
