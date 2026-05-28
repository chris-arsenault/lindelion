const DETECTION_LABELS: [&str; 6] = ["super", "complex", "sparse", "pitch", "energy", "grid"];

fn linnod_detection_controls(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            detection_metric(cx, "source", source_rate_text(signals.summary));
            detection_metric(cx, "markers", marker_count_text(signals.status));
            detection_metric(cx, "algorithm", detection_detail_text(signals.summary));
        })
        .width(Pixels(250.0))
        .horizontal_gap(Pixels(8.0));
        detection_algorithm_controls(cx, signals.summary);
        detection_value_controls(cx, signals.summary);
        linnod_command_button(
            cx,
            ICON_ACTIVITY,
            "Redetect slices",
            EditorEvent::Command(LinnodEditorCommand::RedetectSlices),
        );
    })
    .height(Pixels(46.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn detection_algorithm_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        for (index, label) in DETECTION_LABELS.iter().copied().enumerate() {
            detection_algorithm_button(cx, summary, index, label);
        }
    })
    .class("segmented")
    .class("ll-segmented")
    .width(Pixels(362.0))
    .height(Pixels(25.0))
    .horizontal_gap(Pixels(2.0));
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

fn detection_value_controls(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    HStack::new(cx, move |cx| {
        crate::vizia_controls::dynamic_drag_value(
            cx,
            "MIN",
            detection_min_slice_text(summary),
            move || summary.get().detection.min_slice_ms,
            || {
                crate::vizia_controls::DragValueSpec::new(
                    0.0,
                    2_000.0,
                    50.0,
                    5.0,
                    1.0,
                    92.0,
                    crate::vizia_controls::Accent::Audio,
                )
            },
            move |cx, value| {
                cx.emit(EditorEvent::DetectionEdit(
                    LinnodEditorDetectionEdit::MinSliceMs {
                        min_slice_ms: value,
                    },
                ));
            },
        );
        crate::vizia_controls::dynamic_drag_value(
            cx,
            "PRIMARY",
            detection_primary_param_text(summary),
            move || primary_detection_value(summary),
            move || primary_detection_spec(summary),
            move |cx, value| {
                cx.emit(EditorEvent::DetectionEdit(primary_detection_edit_from_value(
                    summary, value,
                )));
            },
        );
        crate::vizia_controls::dynamic_drag_value(
            cx,
            "DETAIL",
            detection_secondary_param_text(summary),
            move || secondary_detection_value(summary),
            move || secondary_detection_spec(summary),
            move |cx, value| {
                cx.emit(EditorEvent::DetectionEdit(secondary_detection_edit_from_value(
                    summary, value,
                )));
            },
        );
    })
    .height(Pixels(44.0))
    .horizontal_gap(Pixels(6.0));
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

fn primary_detection_value(summary: Signal<LinnodEditorPatchSummary>) -> f32 {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux
        | LinnodEditorDetectionAlgorithm::ComplexFlux => detection.lookback_frames as f32,
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            (detection.spectral_window_size.max(1) as f32).log2()
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            detection.pitch_stability_threshold_cents
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            (detection.energy_frame_size.max(1) as f32).log2()
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => detection.manual_grid_divisions as f32,
    }
}

fn secondary_detection_value(summary: Signal<LinnodEditorPatchSummary>) -> f32 {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux => detection.max_filter_radius as f32,
        LinnodEditorDetectionAlgorithm::ComplexFlux => detection.group_delay_weight,
        LinnodEditorDetectionAlgorithm::SpectralSparsity => detection.lookback_frames as f32,
        LinnodEditorDetectionAlgorithm::PitchStability => {
            detection.pitch_stability_duration_ms
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            (detection.energy_frame_size.max(1) as f32).log2()
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => detection.manual_grid_offset_ms,
    }
}

fn primary_detection_spec(
    summary: Signal<LinnodEditorPatchSummary>,
) -> crate::vizia_controls::DragValueSpec {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux
        | LinnodEditorDetectionAlgorithm::ComplexFlux => {
            drag_spec(1.0, 32.0, 3.0, 1.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => drag_spec(
            64.0_f32.log2(),
            8_192.0_f32.log2(),
            1_024.0_f32.log2(),
            1.0,
            1.0,
            crate::vizia_controls::Accent::Audio,
        ),
        LinnodEditorDetectionAlgorithm::PitchStability => {
            drag_spec(1.0, 2_400.0, 120.0, 10.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => drag_spec(
            32.0_f32.log2(),
            8_192.0_f32.log2(),
            512.0_f32.log2(),
            1.0,
            1.0,
            crate::vizia_controls::Accent::Audio,
        ),
        LinnodEditorDetectionAlgorithm::ManualGrid => {
            drag_spec(1.0, 16.0, 16.0, 1.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
    }
}

fn secondary_detection_spec(
    summary: Signal<LinnodEditorPatchSummary>,
) -> crate::vizia_controls::DragValueSpec {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux => {
            drag_spec(0.0, 32.0, 3.0, 1.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
        LinnodEditorDetectionAlgorithm::ComplexFlux => {
            drag_spec(0.0, 8.0, 1.0, 0.1, 0.01, crate::vizia_controls::Accent::Audio)
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            drag_spec(1.0, 32.0, 3.0, 1.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            drag_spec(1.0, 5_000.0, 64.0, 5.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => drag_spec(
            32.0_f32.log2(),
            8_192.0_f32.log2(),
            512.0_f32.log2(),
            1.0,
            1.0,
            crate::vizia_controls::Accent::Audio,
        ),
        LinnodEditorDetectionAlgorithm::ManualGrid => {
            drag_spec(0.0, 60_000.0, 0.0, 5.0, 1.0, crate::vizia_controls::Accent::Audio)
        }
    }
}

fn primary_detection_edit_from_value(
    summary: Signal<LinnodEditorPatchSummary>,
    value: f32,
) -> LinnodEditorDetectionEdit {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux
        | LinnodEditorDetectionAlgorithm::ComplexFlux => {
            LinnodEditorDetectionEdit::LookbackFrames {
                lookback_frames: value.round().clamp(1.0, 32.0) as u32,
            }
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            LinnodEditorDetectionEdit::SpectralWindowSize {
                window_size: pow2_from_log2(value, 64, 8192),
            }
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            LinnodEditorDetectionEdit::PitchStabilityThresholdCents {
                threshold_cents: value,
            }
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            LinnodEditorDetectionEdit::EnergyFrameSize {
                frame_size: pow2_from_log2(value, 32, 8192),
            }
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => LinnodEditorDetectionEdit::ManualGridDivisions {
            divisions: value.round().clamp(1.0, 16.0) as usize,
        },
    }
}

fn secondary_detection_edit_from_value(
    summary: Signal<LinnodEditorPatchSummary>,
    value: f32,
) -> LinnodEditorDetectionEdit {
    let detection = summary.get().detection;
    match detection.algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux => {
            LinnodEditorDetectionEdit::MaxFilterRadius {
                max_filter_radius: value.round().clamp(0.0, 32.0) as u32,
            }
        }
        LinnodEditorDetectionAlgorithm::ComplexFlux => {
            LinnodEditorDetectionEdit::GroupDelayWeight {
                group_delay_weight: value,
            }
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            LinnodEditorDetectionEdit::LookbackFrames {
                lookback_frames: value.round().clamp(1.0, 32.0) as u32,
            }
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            LinnodEditorDetectionEdit::PitchStabilityDurationMs {
                duration_ms: value,
            }
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            LinnodEditorDetectionEdit::EnergyFrameSize {
                frame_size: pow2_from_log2(value, 32, 8192),
            }
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => {
            LinnodEditorDetectionEdit::ManualGridOffsetMs {
                offset_ms: value,
            }
        }
    }
}

fn drag_spec(
    min: f32,
    max: f32,
    default: f32,
    coarse_step: f32,
    fine_step: f32,
    accent: crate::vizia_controls::Accent,
) -> crate::vizia_controls::DragValueSpec {
    crate::vizia_controls::DragValueSpec::new(
        min,
        max,
        default,
        coarse_step,
        fine_step,
        92.0,
        accent,
    )
}

fn pow2_from_log2(value: f32, min: usize, max: usize) -> usize {
    (2.0_f32.powf(value.round()) as usize).clamp(min, max)
}
