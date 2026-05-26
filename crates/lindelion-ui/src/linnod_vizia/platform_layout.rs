fn build_editor(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        linnod_top_strip(cx, signals);
        HStack::new(cx, move |cx| {
            linnod_source_section(cx, signals);
            linnod_pad_section(cx, signals);
            linnod_slice_section(cx, signals);
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

fn linnod_top_strip(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "LINNOD").class("title");
            Label::new(cx, patch_detail(signals.summary)).class("muted");
            Label::new(cx, command_status_text(signals.command_status)).class("muted");
        })
        .width(Stretch(1.0))
        .vertical_gap(Pixels(2.0));
        linnod_status_chip(cx, source_status_text(signals.status), crate::vizia_controls::ChipKind::Audio);
        linnod_status_chip(cx, analysis_status_text(signals.status), crate::vizia_controls::ChipKind::Slice);
        crate::vizia_controls::metric(cx, "slices", slice_count_text(signals.summary));
        crate::vizia_controls::metric(cx, "voices", voice_status_text(signals.telemetry));
        OutputMeterView::new(cx, signals.telemetry)
            .class("ll-visual-frame")
            .class("ll-visual-tone")
            .width(Pixels(86.0))
            .height(Pixels(34.0));
        HStack::new(cx, move |cx| {
            linnod_command_button(cx, ICON_FILE_MUSIC, "Load source audio", EditorEvent::LoadSourceDialog);
            linnod_command_button(cx, ICON_DEVICE_FLOPPY, "Save patch", EditorEvent::SavePatchDialog);
            linnod_command_button(cx, ICON_FOLDER_OPEN, "Load patch", EditorEvent::LoadPatchDialog);
            linnod_command_button(cx, ICON_DOWNLOAD, "Export patch with samples", EditorEvent::ExportPatchDialog);
        })
        .horizontal_gap(Pixels(5.0));
    })
    .class("topbar")
    .class("ll-top-strip")
    .height(Pixels(60.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn linnod_source_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::section_header(
            cx,
            "Source + Detection",
            source_label(signals.summary),
            crate::vizia_controls::Accent::Audio,
        );
        SourceWaveformView::new_editable(cx, signals.summary, signals.drop_active)
            .class("source-view")
            .class("ll-visual-frame")
            .class("ll-visual-audio")
            .height(Pixels(240.0));
        HStack::new(cx, move |cx| {
            crate::vizia_controls::metric(cx, "source", source_rate_text(signals.summary));
            crate::vizia_controls::metric(cx, "detection", detection_detail_text(signals.summary));
            crate::vizia_controls::metric(cx, "markers", marker_count_text(signals.status));
            linnod_command_button(
                cx,
                ICON_WAVE_SINE,
                "Load source audio",
                EditorEvent::LoadSourceDialog,
            );
        })
        .height(Pixels(38.0))
        .horizontal_gap(Pixels(8.0));
        linnod_detection_controls(cx, signals);
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-audio")
    .width(Pixels(470.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn linnod_pad_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::section_header(
            cx,
            "Pad Matrix",
            trigger_mode_text(signals.summary),
            crate::vizia_controls::Accent::Slice,
        );
        VStack::new(cx, move |cx| {
            for row in 0..4 {
                HStack::new(cx, move |cx| {
                    for col in 0..4 {
                        let index = row * 4 + col + 1;
                        linnod_pad_button(cx, signals, PadId(index as u8));
                    }
                })
                .height(Stretch(1.0))
                .horizontal_gap(Pixels(6.0));
            }
        })
        .height(Stretch(1.0))
        .vertical_gap(Pixels(6.0));
        linnod_trigger_controls(cx, signals);
        linnod_choke_controls(cx, signals);
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-slice")
    .width(Pixels(322.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn linnod_pad_button(cx: &mut Context, signals: EditorSignals, pad: PadId) {
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, pad_title(pad))
                .class("ll-control-value")
                .alignment(Alignment::Center);
            Label::new(cx, pad_slice_text(signals.summary, pad))
                .class("ll-section-subtitle")
                .alignment(Alignment::Center);
            Label::new(cx, pad_midi_text(signals.summary, pad))
                .class("ll-section-subtitle")
                .alignment(Alignment::Center);
            Label::new(cx, pad_choke_text(signals.summary, pad))
                .class("ll-section-subtitle")
                .alignment(Alignment::Center);
        })
        .vertical_gap(Pixels(1.0))
    })
    .on_press(move |cx| cx.emit(EditorEvent::Command(LinnodEditorCommand::SelectPad(pad))))
    .class("pad-button")
    .class("ll-pad-button")
    .toggle_class(
        "pad-selected",
        signals.summary.map(move |summary| pad_selected(summary, pad)),
    )
    .toggle_class(
        "ll-pad-selected",
        signals.summary.map(move |summary| pad_selected(summary, pad)),
    )
    .toggle_class(
        "ll-pad-choked",
        signals
            .summary
            .map(move |summary| pad_summary(summary, pad).is_some_and(|pad| pad.choke_group.is_some())),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn linnod_trigger_controls(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        trigger_button(cx, signals.summary, "pad", LinnodEditorTriggerMode::Pad);
        trigger_button(
            cx,
            signals.summary,
            "chrom",
            LinnodEditorTriggerMode::Chromatic,
        );
    })
    .class("segmented")
    .class("ll-segmented")
    .height(Pixels(25.0))
    .horizontal_gap(Pixels(2.0));
}

fn trigger_button(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    label: &'static str,
    mode: LinnodEditorTriggerMode,
) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| cx.emit(EditorEvent::Command(LinnodEditorCommand::SetTriggerMode(mode))))
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class("seg-active", summary.map(move |summary| summary.trigger_mode == mode))
    .toggle_class(
        "ll-seg-active",
        summary.map(move |summary| summary.trigger_mode == mode),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn linnod_choke_controls(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        Label::new(cx, selected_pad_text(signals.summary))
            .class("ll-control-label")
            .width(Stretch(1.0));
        crate::vizia_controls::compact_text_button(cx, "none", "Clear choke group")
        .on_press(move |cx| cx.emit(pad_choke_event(signals.summary, ChokeChange::Clear)))
        .height(Pixels(24.0));
        Button::new(cx, |cx| {
            Svg::new(cx, ICON_MINUS)
                .class("toolbar-icon")
                .class("ll-toolbar-icon")
        })
        .on_press(move |cx| cx.emit(pad_choke_event(signals.summary, ChokeChange::Previous)))
        .class("toolbar-button")
        .class("ll-tool-button")
        .width(Pixels(28.0))
        .height(Pixels(24.0));
        Button::new(cx, |cx| {
            Svg::new(cx, ICON_PLUS)
                .class("toolbar-icon")
                .class("ll-toolbar-icon")
        })
        .on_press(move |cx| cx.emit(pad_choke_event(signals.summary, ChokeChange::Next)))
        .class("toolbar-button")
        .class("ll-tool-button")
        .width(Pixels(28.0))
        .height(Pixels(24.0));
    })
    .height(Pixels(28.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(5.0));
}

fn linnod_slice_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::section_header(
            cx,
            "Selected Slice",
            selected_slice_title(signals.summary),
            crate::vizia_controls::Accent::Slice,
        );
        SourceWaveformView::new(cx, signals.summary, signals.drop_active)
            .class("ll-visual-frame")
            .class("ll-visual-slice")
            .height(Pixels(112.0));
        linnod_status_chip(cx, selected_slice_range(signals.summary), crate::vizia_controls::ChipKind::Slice);
        linnod_status_chip(cx, selected_pitch_diagnostic_text(signals.summary), crate::vizia_controls::ChipKind::Slice);
        slice_trim_controls(cx, signals.summary);
        slice_pitch_controls(cx, signals.summary);
        slice_gain_pan_controls(cx, signals.summary);
        slice_filter_controls(cx, signals.summary);
        linnod_playback_panel(cx, signals);
        HStack::new(cx, move |cx| {
            linnod_command_button(
                cx,
                ICON_MUSIC,
                "Tune selected slice",
                EditorEvent::Command(LinnodEditorCommand::TuneSelectedSlice),
            );
            linnod_command_button(
                cx,
                ICON_ADJUSTMENTS_HORIZONTAL,
                "Tune all slices",
                EditorEvent::Command(LinnodEditorCommand::TuneAllSlices),
            );
            linnod_command_button(
                cx,
                ICON_ARROWS_SHUFFLE,
                "Snap all slices to scale",
                EditorEvent::Command(LinnodEditorCommand::SnapAllSlicesToScale),
            );
        })
        .height(Pixels(30.0))
        .horizontal_gap(Pixels(6.0));
        HStack::new(cx, move |cx| {
            linnod_parameter_control(
                cx,
                signals.parameter(LinnodEditorSurfaceSlot::MasterGain),
                crate::vizia_controls::Accent::Tone,
            );
            linnod_parameter_control(
                cx,
                signals.parameter(LinnodEditorSurfaceSlot::DetectionSensitivity),
                crate::vizia_controls::Accent::Audio,
            );
            linnod_parameter_control(
                cx,
                signals.parameter(LinnodEditorSurfaceSlot::TuningReference),
                crate::vizia_controls::Accent::Mod,
            );
        })
        .height(Pixels(88.0))
        .horizontal_gap(Pixels(4.0));
        Label::new(cx, tuning_text(signals.summary)).class("ll-section-subtitle");
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-slice")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn slice_trim_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        crate::vizia_controls::drag_value(
            cx,
            "START",
            selected_slice_start_offset_text(summary),
            selected_slice_start_offset_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                0.0,
                60_000.0,
                0.0,
                1.0,
                0.1,
                88.0,
                crate::vizia_controls::Accent::Slice,
            ),
            move |cx, value| cx.emit(slice_trim_start_event(summary, value)),
        );
        crate::vizia_controls::drag_value(
            cx,
            "END",
            selected_slice_end_offset_text(summary),
            selected_slice_end_offset_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                0.0,
                60_000.0,
                0.0,
                1.0,
                0.1,
                88.0,
                crate::vizia_controls::Accent::Slice,
            ),
            move |cx, value| cx.emit(slice_trim_end_event(summary, value)),
        );
    })
    .height(Pixels(44.0))
    .horizontal_gap(Pixels(6.0));
}

fn slice_pitch_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        crate::vizia_controls::drag_value(
            cx,
            "SEMI",
            selected_slice_semitone_text(summary),
            selected_slice_semitone_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                -48.0,
                48.0,
                0.0,
                1.0,
                1.0,
                88.0,
                crate::vizia_controls::Accent::Slice,
            ),
            move |cx, value| cx.emit(slice_pitch_semitone_event(summary, value)),
        );
        crate::vizia_controls::drag_value(
            cx,
            "CENT",
            selected_slice_cent_text(summary),
            selected_slice_cent_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                -100.0,
                100.0,
                0.0,
                1.0,
                0.1,
                88.0,
                crate::vizia_controls::Accent::Slice,
            ),
            move |cx, value| cx.emit(slice_pitch_cent_event(summary, value)),
        );
    })
    .height(Pixels(44.0))
    .horizontal_gap(Pixels(6.0));
}

fn slice_gain_pan_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        crate::vizia_controls::drag_value(
            cx,
            "GAIN",
            selected_slice_gain_text(summary),
            selected_slice_gain_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                -60.0,
                24.0,
                0.0,
                0.5,
                0.1,
                88.0,
                crate::vizia_controls::Accent::Tone,
            ),
            move |cx, value| cx.emit(slice_gain_event(summary, value)),
        );
        crate::vizia_controls::drag_value(
            cx,
            "PAN",
            selected_slice_pan_text(summary),
            selected_slice_pan_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                -1.0,
                1.0,
                0.0,
                0.05,
                0.01,
                88.0,
                crate::vizia_controls::Accent::Mod,
            ),
            move |cx, value| cx.emit(slice_pan_event(summary, value)),
        );
    })
    .height(Pixels(44.0))
    .horizontal_gap(Pixels(6.0));
}

fn slice_filter_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        crate::vizia_controls::drag_value(
            cx,
            "FILTER",
            selected_filter_text(summary),
            selected_filter_octave_value(summary),
            crate::vizia_controls::DragValueSpec::new(
                20.0_f32.log2(),
                20_000.0_f32.log2(),
                20_000.0_f32.log2(),
                0.25,
                0.02,
                182.0,
                crate::vizia_controls::Accent::Tone,
            ),
            move |cx, value| cx.emit(slice_filter_event(summary, value)),
        );
    })
    .height(Pixels(44.0));
}

#[derive(Clone, Copy)]
enum ChokeChange {
    Clear,
    Previous,
    Next,
}

fn pad_choke_event(summary: Signal<LinnodEditorPatchSummary>, change: ChokeChange) -> EditorEvent {
    let summary = summary.get();
    let Some(pad) = selected_pad(&summary) else {
        return EditorEvent::Command(LinnodEditorCommand::SelectPad(PadId(1)));
    };
    let group = match change {
        ChokeChange::Clear => None,
        ChokeChange::Previous => Some(pad.choke_group.unwrap_or(1).saturating_sub(1).max(1)),
        ChokeChange::Next => Some(pad.choke_group.unwrap_or(0).saturating_add(1).min(16)),
    };
    EditorEvent::PadEdit(LinnodEditorPadEdit::ChokeGroup { pad: pad.pad, group })
}

fn slice_trim_start_event(summary: Signal<LinnodEditorPatchSummary>, start_offset_ms: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Offsets {
        slice_index: slice.index,
        start_offset_ms,
        end_offset_ms: slice.end_offset_ms,
    })
}

fn slice_trim_end_event(summary: Signal<LinnodEditorPatchSummary>, end_offset_ms: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Offsets {
        slice_index: slice.index,
        start_offset_ms: slice.start_offset_ms,
        end_offset_ms,
    })
}

fn slice_pitch_semitone_event(
    summary: Signal<LinnodEditorPatchSummary>,
    semitones: f32,
) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Pitch {
        slice_index: slice.index,
        semitones: semitones.round() as i32,
        cents: slice.pitch_cents,
    })
}

fn slice_pitch_cent_event(summary: Signal<LinnodEditorPatchSummary>, cents: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Pitch {
        slice_index: slice.index,
        semitones: slice.pitch_semitones,
        cents,
    })
}

fn slice_gain_event(summary: Signal<LinnodEditorPatchSummary>, gain_db: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::GainDb {
        slice_index: slice.index,
        gain_db,
    })
}

fn slice_pan_event(summary: Signal<LinnodEditorPatchSummary>, pan: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Pan {
        slice_index: slice.index,
        pan,
    })
}

fn slice_filter_event(summary: Signal<LinnodEditorPatchSummary>, cutoff_log2: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::FilterCutoff {
        slice_index: slice.index,
        cutoff_hz: cutoff_log2.exp2(),
    })
}

fn slice_reverse_event(summary: Signal<LinnodEditorPatchSummary>) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Reverse {
        slice_index: slice.index,
        reverse: !slice.reverse,
    })
}
