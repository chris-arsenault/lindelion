const DETECTION_LABELS: [&str; 6] = ["super", "complex", "sparse", "pitch", "energy", "grid"];

fn linnod_detection_controls(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            for (index, label) in DETECTION_LABELS.iter().copied().enumerate() {
                detection_algorithm_button(cx, signals.summary, index, label);
            }
        })
        .class("segmented")
        .class("ll-segmented")
        .height(Pixels(25.0))
        .horizontal_gap(Pixels(2.0));
        HStack::new(cx, move |cx| {
            detection_metric(cx, "markers", marker_count_text(signals.status));
            detection_metric(cx, "algorithm", detection_detail_text(signals.summary));
        })
        .height(Pixels(34.0))
        .horizontal_gap(Pixels(8.0));
        detection_min_slice_row(cx, signals.summary);
        detection_primary_row(cx, signals.summary);
        detection_secondary_row(cx, signals.summary);
        HStack::new(cx, move |cx| {
            linnod_command_button(
                cx,
                ICON_ACTIVITY,
                "Redetect slices",
                EditorEvent::Command(LinnodEditorCommand::RedetectSlices),
            );
            Label::new(cx, "top 16 candidate slices are kept")
                .class("ll-section-subtitle")
                .width(Stretch(1.0));
        })
        .height(Pixels(28.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
    })
    .vertical_gap(Pixels(7.0));
}

fn detection_algorithm_button(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    index: usize,
    label: &'static str,
) {
    let algorithm = algorithm_for_index(index);
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| {
        cx.emit(EditorEvent::DetectionEdit(
            LinnodEditorDetectionEdit::Algorithm { algorithm },
        ));
    })
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class(
        "seg-active",
        summary.map(move |summary| summary.detection.algorithm == algorithm),
    )
    .toggle_class(
        "ll-seg-active",
        summary.map(move |summary| summary.detection.algorithm == algorithm),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn detection_min_slice_row(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    detection_nudge_row(
        cx,
        "min slice",
        detection_min_slice_text(summary),
        move |cx| {
            let value = (summary.get().detection.min_slice_ms - 5.0).max(0.0);
            cx.emit(EditorEvent::DetectionEdit(
                LinnodEditorDetectionEdit::MinSliceMs { min_slice_ms: value },
            ));
        },
        move |cx| {
            let value = (summary.get().detection.min_slice_ms + 5.0).min(2_000.0);
            cx.emit(EditorEvent::DetectionEdit(
                LinnodEditorDetectionEdit::MinSliceMs { min_slice_ms: value },
            ));
        },
    );
}

fn detection_primary_row(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    detection_nudge_row(
        cx,
        "primary",
        detection_primary_param_text(summary),
        move |cx| cx.emit(EditorEvent::DetectionEdit(primary_detection_edit(summary, -1))),
        move |cx| cx.emit(EditorEvent::DetectionEdit(primary_detection_edit(summary, 1))),
    );
}

fn detection_secondary_row(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    detection_nudge_row(
        cx,
        "detail",
        detection_secondary_param_text(summary),
        move |cx| cx.emit(EditorEvent::DetectionEdit(secondary_detection_edit(summary, -1))),
        move |cx| cx.emit(EditorEvent::DetectionEdit(secondary_detection_edit(summary, 1))),
    );
}

fn detection_nudge_row<T, Down, Up>(
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
            .width(Pixels(58.0));
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
    .height(Pixels(26.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(5.0));
}

fn detection_metric<T>(cx: &mut Context, label: &'static str, value: T)
where
    T: Res<String> + Clone + 'static,
{
    crate::vizia_controls::metric(cx, label, value);
}

fn algorithm_for_index(index: usize) -> LinnodEditorDetectionAlgorithm {
    match index {
        0 => LinnodEditorDetectionAlgorithm::SuperFlux,
        1 => LinnodEditorDetectionAlgorithm::ComplexFlux,
        2 => LinnodEditorDetectionAlgorithm::SpectralSparsity,
        3 => LinnodEditorDetectionAlgorithm::PitchStability,
        4 => LinnodEditorDetectionAlgorithm::EnergyTransient,
        _ => LinnodEditorDetectionAlgorithm::ManualGrid,
    }
}

fn primary_detection_edit(
    summary: Signal<LinnodEditorPatchSummary>,
    direction: i32,
) -> LinnodEditorDetectionEdit {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux
        | LinnodEditorDetectionAlgorithm::ComplexFlux => {
            let next = offset_u32(detection.lookback_frames, direction, 1, 32);
            LinnodEditorDetectionEdit::LookbackFrames {
                lookback_frames: next,
            }
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            LinnodEditorDetectionEdit::SpectralWindowSize {
                window_size: scale_pow2(detection.spectral_window_size, direction, 64, 8192),
            }
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            LinnodEditorDetectionEdit::PitchStabilityThresholdCents {
                threshold_cents: offset_f32(
                    detection.pitch_stability_threshold_cents,
                    direction,
                    10.0,
                    1.0,
                    2_400.0,
                ),
            }
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            LinnodEditorDetectionEdit::EnergyFrameSize {
                frame_size: scale_pow2(detection.energy_frame_size, direction, 32, 8192),
            }
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => LinnodEditorDetectionEdit::ManualGridDivisions {
            divisions: offset_usize(detection.manual_grid_divisions, direction, 1, 16),
        },
    }
}

fn secondary_detection_edit(
    summary: Signal<LinnodEditorPatchSummary>,
    direction: i32,
) -> LinnodEditorDetectionEdit {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux => {
            LinnodEditorDetectionEdit::MaxFilterRadius {
                max_filter_radius: offset_u32(detection.max_filter_radius, direction, 0, 32),
            }
        }
        LinnodEditorDetectionAlgorithm::ComplexFlux => {
            LinnodEditorDetectionEdit::GroupDelayWeight {
                group_delay_weight: offset_f32(
                    detection.group_delay_weight,
                    direction,
                    0.1,
                    0.0,
                    8.0,
                ),
            }
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            LinnodEditorDetectionEdit::LookbackFrames {
                lookback_frames: offset_u32(detection.lookback_frames, direction, 1, 32),
            }
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            LinnodEditorDetectionEdit::PitchStabilityDurationMs {
                duration_ms: offset_f32(
                    detection.pitch_stability_duration_ms,
                    direction,
                    5.0,
                    1.0,
                    5_000.0,
                ),
            }
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            LinnodEditorDetectionEdit::EnergyFrameSize {
                frame_size: scale_pow2(detection.energy_frame_size, direction, 32, 8192),
            }
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => {
            LinnodEditorDetectionEdit::ManualGridOffsetMs {
                offset_ms: offset_f32(detection.manual_grid_offset_ms, direction, 5.0, 0.0, 60_000.0),
            }
        }
    }
}

fn offset_u32(value: u32, direction: i32, min: u32, max: u32) -> u32 {
    if direction < 0 {
        value.saturating_sub(1).max(min)
    } else {
        value.saturating_add(1).min(max)
    }
}

fn offset_usize(value: usize, direction: i32, min: usize, max: usize) -> usize {
    if direction < 0 {
        value.saturating_sub(1).max(min)
    } else {
        value.saturating_add(1).min(max)
    }
}

fn offset_f32(value: f32, direction: i32, step: f32, min: f32, max: f32) -> f32 {
    let signed_step = if direction < 0 { -step } else { step };
    (value + signed_step).clamp(min, max)
}

fn scale_pow2(value: usize, direction: i32, min: usize, max: usize) -> usize {
    if direction < 0 {
        (value / 2).max(min)
    } else {
        value.saturating_mul(2).min(max)
    }
}
