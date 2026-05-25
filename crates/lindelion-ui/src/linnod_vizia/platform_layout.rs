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
        Button::new(cx, move |cx| {
            SourceWaveformView::new(cx, signals.summary, signals.drop_active)
                .class("source-view")
                .class("ll-visual-frame")
                .class("ll-visual-audio")
                .height(Stretch(1.0))
        })
        .on_press(|cx| cx.emit(EditorEvent::LoadSourceDialog))
        .class("ll-visual-frame")
        .class("ll-visual-audio")
        .height(Pixels(240.0));
        HStack::new(cx, move |cx| {
            crate::vizia_controls::metric(cx, "source", source_rate_text(signals.summary));
            crate::vizia_controls::metric(cx, "detection", detection_detail_text(signals.summary));
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
        slice_trim_controls(cx, signals.summary);
        slice_pitch_controls(cx, signals.summary);
        slice_gain_pan_controls(cx, signals.summary);
        slice_filter_controls(cx, signals.summary);
        slice_playback_controls(cx, signals.summary);
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
    slice_nudge_row(
        cx,
        "trim",
        selected_slice_trim_text(summary),
        move |cx| cx.emit(slice_trim_event(summary, -1.0, 0.0)),
        move |cx| cx.emit(slice_trim_event(summary, 1.0, 0.0)),
    );
    slice_nudge_row(
        cx,
        "end",
        selected_slice_range(summary),
        move |cx| cx.emit(slice_trim_event(summary, 0.0, -1.0)),
        move |cx| cx.emit(slice_trim_event(summary, 0.0, 1.0)),
    );
}

fn slice_pitch_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    slice_nudge_row(
        cx,
        "pitch",
        selected_pitch_text(summary),
        move |cx| cx.emit(slice_pitch_event(summary, -1, 0.0)),
        move |cx| cx.emit(slice_pitch_event(summary, 1, 0.0)),
    );
    slice_nudge_row(
        cx,
        "cents",
        selected_pitch_text(summary),
        move |cx| cx.emit(slice_pitch_event(summary, 0, -5.0)),
        move |cx| cx.emit(slice_pitch_event(summary, 0, 5.0)),
    );
}

fn slice_gain_pan_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    slice_nudge_row(
        cx,
        "gain",
        selected_gain_pan_text(summary),
        move |cx| cx.emit(slice_gain_event(summary, -1.0)),
        move |cx| cx.emit(slice_gain_event(summary, 1.0)),
    );
    slice_nudge_row(
        cx,
        "pan",
        selected_gain_pan_text(summary),
        move |cx| cx.emit(slice_pan_event(summary, -0.05)),
        move |cx| cx.emit(slice_pan_event(summary, 0.05)),
    );
}

fn slice_filter_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    slice_nudge_row(
        cx,
        "filter",
        selected_filter_text(summary),
        move |cx| cx.emit(slice_filter_event(summary, 0.5)),
        move |cx| cx.emit(slice_filter_event(summary, 2.0)),
    );
}

fn slice_playback_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        playback_button(cx, summary, "one", LinnodEditorPlaybackMode::OneShot);
        playback_button(cx, summary, "gate", LinnodEditorPlaybackMode::Gated);
        playback_button(cx, summary, "loop", LinnodEditorPlaybackMode::Looped);
        crate::vizia_controls::compact_text_button(cx, "rev", "Toggle reverse")
        .on_press(move |cx| cx.emit(slice_reverse_event(summary)))
        .height(Pixels(24.0));
    })
    .height(Pixels(27.0))
    .horizontal_gap(Pixels(5.0));
}

fn playback_button(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    label: &'static str,
    mode: LinnodEditorPlaybackMode,
) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| cx.emit(slice_playback_event(summary, mode)))
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class(
        "ll-seg-active",
        summary.map(move |summary| selected_slice(summary).playback_mode == mode),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn slice_nudge_row<T, Down, Up>(
    cx: &mut Context,
    label: &'static str,
    value: T,
    down: Down,
    up: Up,
) where
    T: Res<String> + Clone + 'static,
    Down: Fn(&mut EventContext) + Copy + Send + Sync + 'static,
    Up: Fn(&mut EventContext) + Copy + Send + Sync + 'static,
{
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("ll-control-label")
            .width(Pixels(44.0));
        Button::new(cx, |cx| {
            Svg::new(cx, ICON_MINUS)
                .class("toolbar-icon")
                .class("ll-toolbar-icon")
        })
        .on_press(down)
        .class("toolbar-button")
        .class("ll-tool-button")
        .width(Pixels(28.0))
        .height(Pixels(24.0));
        Label::new(cx, value.clone())
            .class("ll-control-value")
            .width(Stretch(1.0))
            .alignment(Alignment::Center);
        Button::new(cx, |cx| {
            Svg::new(cx, ICON_PLUS)
                .class("toolbar-icon")
                .class("ll-toolbar-icon")
        })
        .on_press(up)
        .class("toolbar-button")
        .class("ll-tool-button")
        .width(Pixels(28.0))
        .height(Pixels(24.0));
    })
    .height(Pixels(27.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(5.0));
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

fn slice_trim_event(
    summary: Signal<LinnodEditorPatchSummary>,
    start_delta: f32,
    end_delta: f32,
) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Offsets {
        slice_index: slice.index,
        start_offset_ms: (slice.start_offset_ms + start_delta).max(0.0),
        end_offset_ms: (slice.end_offset_ms + end_delta).max(0.0),
    })
}

fn slice_pitch_event(
    summary: Signal<LinnodEditorPatchSummary>,
    semitone_delta: i32,
    cent_delta: f32,
) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Pitch {
        slice_index: slice.index,
        semitones: (slice.pitch_semitones + semitone_delta).clamp(-48, 48),
        cents: (slice.pitch_cents + cent_delta).clamp(-100.0, 100.0),
    })
}

fn slice_gain_event(summary: Signal<LinnodEditorPatchSummary>, delta: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::GainDb {
        slice_index: slice.index,
        gain_db: (slice.gain_db + delta).clamp(-60.0, 24.0),
    })
}

fn slice_pan_event(summary: Signal<LinnodEditorPatchSummary>, delta: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Pan {
        slice_index: slice.index,
        pan: (slice.pan + delta).clamp(-1.0, 1.0),
    })
}

fn slice_filter_event(summary: Signal<LinnodEditorPatchSummary>, factor: f32) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::FilterCutoff {
        slice_index: slice.index,
        cutoff_hz: (slice.filter_cutoff_hz * factor).clamp(20.0, 20_000.0),
    })
}

fn slice_reverse_event(summary: Signal<LinnodEditorPatchSummary>) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::Reverse {
        slice_index: slice.index,
        reverse: !slice.reverse,
    })
}

fn slice_playback_event(
    summary: Signal<LinnodEditorPatchSummary>,
    mode: LinnodEditorPlaybackMode,
) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::PlaybackMode {
        slice_index: slice.index,
        mode,
    })
}
