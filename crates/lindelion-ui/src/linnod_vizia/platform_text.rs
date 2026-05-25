fn patch_detail(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| format!("{} / {}", summary.get().patch_name, summary.get().source_label))
}

fn source_label(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| summary.get().source_label)
}

fn source_rate_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| format!("{} Hz", summary.get().source_sample_rate))
}

fn marker_count_text(status: Signal<LinnodEditorStatus>) -> Memo<String> {
    Memo::new(move |_| format!("{} markers", status.get().marker_count))
}

fn detection_detail_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let detection = summary.get().detection;
        format!(
            "{} / min {:.0} ms",
            detection_algorithm_label(detection.algorithm),
            detection.min_slice_ms
        )
    })
}

fn detection_min_slice_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| format!("Min {:.0} ms", summary.get().detection.min_slice_ms))
}

fn detection_primary_param_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let detection = summary.get().detection;
        match detection.algorithm {
            LinnodEditorDetectionAlgorithm::SuperFlux => {
                format!("Lookback {}", detection.lookback_frames)
            }
            LinnodEditorDetectionAlgorithm::ComplexFlux => {
                format!("Lookback {}", detection.lookback_frames)
            }
            LinnodEditorDetectionAlgorithm::SpectralSparsity => {
                format!("Window {}", detection.spectral_window_size)
            }
            LinnodEditorDetectionAlgorithm::PitchStability => {
                format!("{:.0} cents", detection.pitch_stability_threshold_cents)
            }
            LinnodEditorDetectionAlgorithm::EnergyTransient => {
                format!("Frame {}", detection.energy_frame_size)
            }
            LinnodEditorDetectionAlgorithm::ManualGrid => {
                format!("Divisions {}", detection.manual_grid_divisions)
            }
        }
    })
}

fn detection_secondary_param_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let detection = summary.get().detection;
        match detection.algorithm {
            LinnodEditorDetectionAlgorithm::SuperFlux => {
                format!("Radius {}", detection.max_filter_radius)
            }
            LinnodEditorDetectionAlgorithm::ComplexFlux => {
                format!("Delay {:.2}", detection.group_delay_weight)
            }
            LinnodEditorDetectionAlgorithm::SpectralSparsity => {
                format!("Lookback {}", detection.lookback_frames)
            }
            LinnodEditorDetectionAlgorithm::PitchStability => {
                format!("Stable {:.0} ms", detection.pitch_stability_duration_ms)
            }
            LinnodEditorDetectionAlgorithm::EnergyTransient => {
                format!("Frame x2 {}", detection.energy_frame_size)
            }
            LinnodEditorDetectionAlgorithm::ManualGrid => {
                format!("Offset {:.0} ms", detection.manual_grid_offset_ms)
            }
        }
    })
}

fn detection_algorithm_label(algorithm: LinnodEditorDetectionAlgorithm) -> &'static str {
    match algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux => "SuperFlux",
        LinnodEditorDetectionAlgorithm::ComplexFlux => "ComplexFlux",
        LinnodEditorDetectionAlgorithm::SpectralSparsity => "SpectralSparsity",
        LinnodEditorDetectionAlgorithm::PitchStability => "PitchStability",
        LinnodEditorDetectionAlgorithm::EnergyTransient => "EnergyTransient",
        LinnodEditorDetectionAlgorithm::ManualGrid => "ManualGrid",
    }
}

fn tuning_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let summary = summary.get();
        format!(
            "{} {} / {:.1} Hz",
            summary.tuning_root_label, summary.tuning_scale_label, summary.tuning_reference_hz
        )
    })
}

fn command_status_text(command: Signal<Option<LinnodEditorCommand>>) -> Memo<String> {
    Memo::new(move |_| {
        match command.get() {
            Some(LinnodEditorCommand::LoadSource) => "Source load requested",
            Some(LinnodEditorCommand::RedetectSlices) => "Redetect requested",
            Some(LinnodEditorCommand::TuneSelectedSlice) => "Selected slice tuned",
            Some(LinnodEditorCommand::TuneAllSlices) => "All slices tuned",
            Some(LinnodEditorCommand::SnapAllSlicesToScale) => "Scale snap requested",
            Some(LinnodEditorCommand::SavePatch) => "Patch saved",
            Some(LinnodEditorCommand::LoadPatch) => "Patch loaded",
            Some(LinnodEditorCommand::ExportPatchWithSamples) => "Patch exported",
            Some(LinnodEditorCommand::SetTriggerMode(_)) => "Trigger mode changed",
            Some(LinnodEditorCommand::SelectPad(_)) => "Pad selected",
            None => "Ready",
        }
        .to_string()
    })
}

fn source_status_text(status: Signal<LinnodEditorStatus>) -> Memo<String> {
    Memo::new(move |_| source_status_label(status.get().source_status).to_string())
}

fn analysis_status_text(status: Signal<LinnodEditorStatus>) -> Memo<String> {
    Memo::new(move |_| {
        let status = status.get();
        if status.has_analysis {
            "Detected"
        } else if status.has_source {
            "Waiting"
        } else {
            "No source"
        }
        .to_string()
    })
}

fn voice_status_text(telemetry: Signal<LinnodEditorTelemetry>) -> Memo<String> {
    Memo::new(move |_| format!("{:.0} active", telemetry.get().active_voices.max(0.0)))
}

fn source_status_label(status: LinnodEditorSourceStatus) -> &'static str {
    match status {
        LinnodEditorSourceStatus::Idle => "None",
        LinnodEditorSourceStatus::PendingLoad => "Loading",
        LinnodEditorSourceStatus::Analyzing => "Analyzing",
        LinnodEditorSourceStatus::Ready => "Loaded",
        LinnodEditorSourceStatus::MissingSource => "Missing",
        LinnodEditorSourceStatus::Error => "Error",
    }
}

fn trigger_mode_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        match summary.get().trigger_mode {
            LinnodEditorTriggerMode::Pad => "Pad",
            LinnodEditorTriggerMode::Chromatic => "Chromatic",
        }
        .to_string()
    })
}

fn slice_count_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| format!("{}", summary.get().slices.len()))
}

fn pad_title(pad: PadId) -> String {
    format!("Pad {}", pad.0)
}

fn pad_choke_text(summary: Signal<LinnodEditorPatchSummary>, pad: PadId) -> Memo<String> {
    Memo::new(move |_| match pad_summary(&summary.get(), pad).and_then(|pad| pad.choke_group) {
        Some(group) => format!("Ch {group}"),
        None => "Ch --".to_string(),
    })
}

fn pad_slice_text(summary: Signal<LinnodEditorPatchSummary>, pad: PadId) -> Memo<String> {
    Memo::new(move |_| {
        pad_summary(&summary.get(), pad)
            .map(|pad| format!("Slice {}", pad.slice_index + 1))
            .unwrap_or_else(|| "Slice --".to_string())
    })
}

fn pad_midi_text(summary: Signal<LinnodEditorPatchSummary>, pad: PadId) -> Memo<String> {
    Memo::new(move |_| {
        pad_summary(&summary.get(), pad)
            .map(|pad| format!("MIDI {}", pad.midi_note))
            .unwrap_or_else(|| "MIDI --".to_string())
    })
}

fn pad_selected(summary: &LinnodEditorPatchSummary, pad: PadId) -> bool {
    pad_summary(summary, pad).is_some_and(|pad| pad.selected)
}

fn pad_summary(summary: &LinnodEditorPatchSummary, pad: PadId) -> Option<LinnodEditorPadSummary> {
    summary
        .pads
        .iter()
        .find(|summary| summary.pad == pad)
        .cloned()
}

fn slice_index_text(index: usize) -> String {
    format!("{:02}", index + 1)
}

fn selected_slice_title(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{} / {}", slice_index_text(slice.index), slice.name)
    })
}

fn selected_slice_range(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let summary = summary.get();
        let slice = selected_slice(&summary);
        let Some((start_sample, end_sample)) = effective_slice_bounds(&summary, &slice) else {
            return "Range pending".to_string();
        };
        let rate = summary.source_sample_rate.max(1) as f32;
        let start_ms = start_sample as f32 / rate * 1_000.0;
        let end_ms = end_sample as f32 / rate * 1_000.0;
        format!("{start_ms:.1}-{end_ms:.1} ms")
    })
}

fn selected_slice_start_offset_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).start_offset_ms)
}

fn selected_slice_end_offset_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).end_offset_ms)
}

fn selected_slice_start_offset_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:+.1} ms", slice.start_offset_ms)
    })
}

fn selected_slice_end_offset_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:+.1} ms", -slice.end_offset_ms)
    })
}

fn selected_slice_semitone_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).pitch_semitones as f32)
}

fn selected_slice_cent_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).pitch_cents)
}

fn selected_slice_semitone_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:+} st", slice.pitch_semitones)
    })
}

fn selected_slice_cent_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:+.1} ct", slice.pitch_cents)
    })
}

fn selected_pitch_diagnostic_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let summary = summary.get();
        let slice = selected_slice(&summary);
        let detected = detected_pitch_text(&slice);
        let target = selected_pitch_target_text_for_slice(&slice, summary.tuning_reference_hz);
        let pad = selected_pad(&summary)
            .map(|pad| format!("MIDI {}", pad.midi_note))
            .unwrap_or_else(|| "MIDI --".to_string());
        format!("{detected} -> {target} / {pad}")
    })
}

fn selected_slice_gain_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).gain_db)
}

fn selected_slice_pan_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).pan)
}

fn selected_slice_gain_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:+.1} dB", slice.gain_db)
    })
}

fn selected_slice_pan_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:+.2}", slice.pan)
    })
}

fn selected_filter_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let slice = selected_slice(&summary.get());
        format!("{:.0} Hz", slice.filter_cutoff_hz)
    })
}

fn selected_filter_octave_value(summary: Signal<LinnodEditorPatchSummary>) -> Memo<f32> {
    Memo::new(move |_| selected_slice(&summary.get()).filter_cutoff_hz.max(20.0).log2())
}

fn selected_pad_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| {
        let summary = summary.get();
        let prefix = match summary.trigger_mode {
            LinnodEditorTriggerMode::Pad => "Pad",
            LinnodEditorTriggerMode::Chromatic => "Root pad",
        };
        selected_pad(&summary)
            .map(|pad| {
                format!(
                    "{prefix} {} / MIDI {} / Slice {}",
                    pad.pad.0,
                    pad.midi_note,
                    pad.slice_index + 1
                )
            })
            .unwrap_or_else(|| "No pad".to_string())
    })
}

fn detected_pitch_text(slice: &LinnodEditorSliceSummary) -> String {
    match (slice.detected_f0_hz, slice.detected_midi_note) {
        (Some(frequency), Some(note)) => format!("det {frequency:.1} Hz {}", midi_note_text(note)),
        (Some(frequency), None) => format!("det {frequency:.1} Hz"),
        _ => "det --".to_string(),
    }
}

fn selected_pitch_target_text_for_slice(
    slice: &LinnodEditorSliceSummary,
    reference_hz: f32,
) -> String {
    match slice.root_target_f0_hz {
        Some(frequency) => format!(
            "target {frequency:.1} Hz {}",
            midi_note_text(midi_note_from_frequency(frequency, reference_hz))
        ),
        None => "target --".to_string(),
    }
}

fn midi_note_text(note: f32) -> String {
    if !note.is_finite() {
        return "--".to_string();
    }
    let rounded = note.round().clamp(0.0, 127.0) as i32;
    let cents = (note - rounded as f32) * 100.0;
    let name = match rounded.rem_euclid(12) {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        _ => "B",
    };
    let octave = rounded / 12 - 1;
    if cents.abs() < 0.5 {
        format!("{name}{octave}")
    } else {
        format!("{name}{octave}{cents:+.0}ct")
    }
}

fn midi_note_from_frequency(frequency_hz: f32, reference_hz: f32) -> f32 {
    if frequency_hz > 0.0
        && frequency_hz.is_finite()
        && reference_hz > 0.0
        && reference_hz.is_finite()
    {
        69.0 + 12.0 * (frequency_hz / reference_hz).log2()
    } else {
        f32::NAN
    }
}

fn selected_slice(summary: &LinnodEditorPatchSummary) -> LinnodEditorSliceSummary {
    let fallback = summary.selected_slice_index.unwrap_or(0);
    summary
        .slices
        .get(fallback)
        .cloned()
        .or_else(|| summary.slices.first().cloned())
        .unwrap_or_else(|| LinnodEditorSliceSummary::empty(fallback))
}

fn selected_pad(summary: &LinnodEditorPatchSummary) -> Option<LinnodEditorPadSummary> {
    summary
        .pads
        .iter()
        .find(|pad| pad.selected)
        .cloned()
        .or_else(|| summary.pads.first().cloned())
}

fn source_span_samples(summary: &LinnodEditorPatchSummary) -> usize {
    let marker_max = summary
        .markers
        .iter()
        .map(|marker| marker.position_samples)
        .max()
        .unwrap_or(0);
    let slice_max = summary
        .slices
        .iter()
        .map(|slice| slice.end_sample)
        .max()
        .unwrap_or(0);
    let source_span = marker_max.max(slice_max);
    let source_span = if source_span > 0 {
        source_span
    } else {
        summary.source_sample_rate as usize
    };
    source_span.max(1)
}

fn slice_bounds(summary: &LinnodEditorPatchSummary, index: usize) -> Option<(usize, usize)> {
    if let Some(start) = summary
        .markers
        .get(index)
        .map(|marker| marker.position_samples)
    {
        let end = summary
            .markers
            .get(index + 1)
            .map(|marker| marker.position_samples)
            .unwrap_or_else(|| source_span_samples(summary));
        if end > start {
            return Some((start, end));
        }
    }
    if let Some(slice) = summary
        .slices
        .get(index)
        .filter(|slice| slice.end_sample > slice.start_sample)
    {
        return Some((slice.start_sample, slice.end_sample));
    }
    None
}

fn effective_slice_bounds(
    summary: &LinnodEditorPatchSummary,
    slice: &LinnodEditorSliceSummary,
) -> Option<(usize, usize)> {
    let (start, end) = slice_bounds(summary, slice.index)?;
    let rate = summary.source_sample_rate.max(1);
    let start_offset = ms_to_samples(slice.start_offset_ms, rate);
    let end_offset = ms_to_samples(slice.end_offset_ms, rate);
    let start = start.saturating_add(start_offset).min(end);
    let end = end.saturating_sub(end_offset).max(start);
    (end > start).then_some((start, end))
}

fn ms_to_samples(milliseconds: f32, sample_rate: u32) -> usize {
    if !milliseconds.is_finite() || milliseconds <= 0.0 {
        return 0;
    }
    (milliseconds * sample_rate as f32 / 1_000.0).round() as usize
}
