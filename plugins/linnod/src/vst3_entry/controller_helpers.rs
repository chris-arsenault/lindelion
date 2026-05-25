use lindelion_onset_detect::{
    AlgorithmParams, DetectionAlgorithm, DetectionConfig, MarkerKind, SliceMarker,
};
use lindelion_ui::{
    PadId as UiPadId,
    linnod_vizia::{
        LinnodEditorDetectionAlgorithm, LinnodEditorDetectionConfig, LinnodEditorMarker,
        LinnodEditorMarkerKind, LinnodEditorPadSummary, LinnodEditorPatchSummary,
        LinnodEditorPlaybackMode, LinnodEditorSliceSummary, LinnodEditorSourceStatus,
        LinnodEditorTriggerMode,
    },
    waveform_points_from_samples,
};

const LINNOD_WAVEFORM_PREVIEW_POINTS: usize = 16_384;

use crate::{
    LinnodPatch,
    patch::{PlaybackMode, TriggerMode},
    tuning::slice_tuning_info,
};

use super::messages::{LinnodSourceSlicePayload, LinnodSourceSummaryPayload};

pub(super) fn editor_summary_from_patch(patch: &LinnodPatch) -> LinnodEditorPatchSummary {
    LinnodEditorPatchSummary {
        patch_name: patch.name.clone(),
        source_label: source_label(patch),
        source_sample_rate: 48_000,
        waveform: Vec::new(),
        markers: patch.markers.iter().copied().map(editor_marker).collect(),
        pads: patch
            .pad_map
            .iter()
            .map(|pad| editor_pad(patch, pad))
            .collect(),
        slices: patch.slices.iter().enumerate().map(editor_slice).collect(),
        detection: editor_detection(patch.detection),
        trigger_mode: editor_trigger_mode(patch.trigger_mode),
        tuning_reference_hz: patch.tuning.reference_hz,
        tuning_root_label: format!("{:?}", patch.tuning.root),
        tuning_scale_label: format!("{:?}", patch.tuning.scale),
        selected_slice_index: patch.selected_slice_index(),
    }
}

pub(super) fn source_summary_payload_from_plugin(
    plugin: &crate::Linnod,
) -> Option<LinnodSourceSummaryPayload> {
    let analysis = plugin.source_analysis()?;
    Some(LinnodSourceSummaryPayload {
        source_label: source_analysis_label(analysis),
        source_sample_rate: analysis.source.sample_rate,
        waveform: waveform_points_from_samples(
            analysis.audio.samples(),
            LINNOD_WAVEFORM_PREVIEW_POINTS,
        )
        .into_iter()
        .map(Into::into)
        .collect(),
        slices: analysis
            .slice_pitch_summaries()
            .iter()
            .map(|summary| {
                let tuning_info = slice_tuning_info(
                    &analysis.pitch_shift_cache,
                    summary.slice_index,
                    &plugin.patch().tuning,
                );
                LinnodSourceSlicePayload {
                    index: summary.slice_index,
                    start_sample: summary.start_sample,
                    end_sample: summary.end_sample,
                    detected_f0_hz: summary.detected_f0_hz,
                    detected_midi_note: tuning_info.map(|info| info.detected_midi_note),
                    nearest_midi_note: tuning_info.map(|info| info.nearest_midi_note),
                    nearest_scale_midi_note: tuning_info.map(|info| info.nearest_scale_midi_note),
                    nearest_midi_note_hz: tuning_info.map(|info| info.nearest_midi_note_hz),
                    nearest_scale_midi_note_hz: tuning_info
                        .map(|info| info.nearest_scale_midi_note_hz),
                    cents_deviation: tuning_info.map(|info| info.cents_deviation),
                    root_target_f0_hz: root_target_f0_hz(plugin.patch(), summary),
                }
            })
            .collect(),
    })
}

pub(super) fn apply_source_summary_payload(
    summary: &mut LinnodEditorPatchSummary,
    payload: &LinnodSourceSummaryPayload,
) {
    summary.source_label.clone_from(&payload.source_label);
    summary.source_sample_rate = payload.source_sample_rate;
    summary.waveform = payload.waveform.iter().copied().map(Into::into).collect();
    for source_slice in &payload.slices {
        if let Some(slice) = summary.slices.get_mut(source_slice.index) {
            slice.start_sample = source_slice.start_sample;
            slice.end_sample = source_slice.end_sample;
            slice.detected_f0_hz = source_slice.detected_f0_hz;
            slice.detected_midi_note = source_slice.detected_midi_note;
            slice.nearest_midi_note = source_slice.nearest_midi_note;
            slice.nearest_scale_midi_note = source_slice.nearest_scale_midi_note;
            slice.nearest_midi_note_hz = source_slice.nearest_midi_note_hz;
            slice.nearest_scale_midi_note_hz = source_slice.nearest_scale_midi_note_hz;
            slice.cents_deviation = source_slice.cents_deviation;
            slice.root_target_f0_hz = source_slice.root_target_f0_hz;
        }
    }
}

fn root_target_f0_hz(
    patch: &LinnodPatch,
    summary: &lindelion_pitch_shift::PitchShiftSliceSummary,
) -> Option<f32> {
    let detected_f0_hz = summary.detected_f0_hz?;
    let slice = patch.slice(summary.slice_index)?;
    Some(detected_f0_hz * slice.pitch.ratio())
}

fn source_label(patch: &LinnodPatch) -> String {
    patch
        .source_sample
        .as_ref()
        .and_then(|reference| reference.last_known_path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("No source")
        .to_string()
}

fn source_analysis_label(analysis: &crate::SourceAnalysis) -> String {
    let filename = analysis.source.filename.trim();
    if !filename.is_empty() {
        filename.to_string()
    } else {
        analysis
            .source
            .reference
            .last_known_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("No source")
            .to_string()
    }
}

fn editor_marker(marker: SliceMarker) -> LinnodEditorMarker {
    LinnodEditorMarker {
        position_samples: marker.position_samples,
        kind: match marker.kind {
            MarkerKind::Auto => LinnodEditorMarkerKind::Auto,
            MarkerKind::User => LinnodEditorMarkerKind::User,
        },
    }
}

fn editor_pad(patch: &LinnodPatch, assignment: &crate::PadAssignment) -> LinnodEditorPadSummary {
    LinnodEditorPadSummary {
        pad: UiPadId(assignment.pad.sanitized().0),
        midi_note: assignment.midi_note,
        slice_index: assignment.slice_index,
        choke_group: assignment.choke_group.map(|group| group.sanitized().0),
        selected: assignment.pad.sanitized() == patch.active_chromatic_pad.sanitized(),
    }
}

fn editor_detection(config: DetectionConfig) -> LinnodEditorDetectionConfig {
    let config = config.sanitized();
    let profile = config.effective_profile();
    let mut editor = LinnodEditorDetectionConfig {
        algorithm: editor_detection_algorithm(config.algorithm),
        min_slice_ms: config.min_slice_ms,
        lookback_frames: profile.lookback_frames,
        max_filter_radius: profile.max_filter_radius,
        pitch_stability_threshold_cents: profile.pitch_stability_threshold_cents,
        pitch_stability_duration_ms: profile.pitch_stability_duration_ms,
        ..LinnodEditorDetectionConfig::default()
    };
    match config.params {
        AlgorithmParams::SuperFlux {
            lookback_frames,
            max_filter_radius,
        } => {
            editor.lookback_frames = lookback_frames;
            editor.max_filter_radius = max_filter_radius;
        }
        AlgorithmParams::ComplexFlux {
            lookback_frames,
            group_delay_weight,
        } => {
            editor.lookback_frames = lookback_frames;
            editor.group_delay_weight = group_delay_weight;
        }
        AlgorithmParams::SpectralSparsity { window_size } => {
            editor.spectral_window_size = window_size;
        }
        AlgorithmParams::PitchStability {
            threshold_cents,
            min_stable_duration_ms,
        } => {
            editor.pitch_stability_threshold_cents = threshold_cents;
            editor.pitch_stability_duration_ms = min_stable_duration_ms;
        }
        AlgorithmParams::EnergyTransient { frame_size } => {
            editor.energy_frame_size = frame_size;
        }
        AlgorithmParams::ManualGrid {
            divisions,
            offset_ms,
        } => {
            editor.manual_grid_divisions = divisions;
            editor.manual_grid_offset_ms = offset_ms;
        }
    }
    editor
}

fn editor_detection_algorithm(algorithm: DetectionAlgorithm) -> LinnodEditorDetectionAlgorithm {
    match algorithm {
        DetectionAlgorithm::SuperFlux => LinnodEditorDetectionAlgorithm::SuperFlux,
        DetectionAlgorithm::ComplexFlux => LinnodEditorDetectionAlgorithm::ComplexFlux,
        DetectionAlgorithm::SpectralSparsity => LinnodEditorDetectionAlgorithm::SpectralSparsity,
        DetectionAlgorithm::PitchStability => LinnodEditorDetectionAlgorithm::PitchStability,
        DetectionAlgorithm::EnergyTransient => LinnodEditorDetectionAlgorithm::EnergyTransient,
        DetectionAlgorithm::ManualGrid => LinnodEditorDetectionAlgorithm::ManualGrid,
    }
}

fn editor_slice((index, slice): (usize, &crate::SliceParams)) -> LinnodEditorSliceSummary {
    LinnodEditorSliceSummary {
        index,
        name: slice.name.clone(),
        start_sample: 0,
        end_sample: 0,
        start_offset_ms: slice.start_offset_ms,
        end_offset_ms: slice.end_offset_ms,
        detected_f0_hz: None,
        detected_midi_note: None,
        nearest_midi_note: None,
        nearest_scale_midi_note: None,
        nearest_midi_note_hz: None,
        nearest_scale_midi_note_hz: None,
        cents_deviation: None,
        root_target_f0_hz: None,
        gain_db: slice.gain_db,
        pan: slice.pan,
        pitch_semitones: slice.pitch.semitones,
        pitch_cents: slice.pitch.cents,
        reverse: slice.reverse,
        playback_mode: editor_playback_mode(slice.playback_mode),
        filter_cutoff_hz: slice.filter_cutoff,
    }
}

fn editor_trigger_mode(mode: TriggerMode) -> LinnodEditorTriggerMode {
    match mode {
        TriggerMode::Pad => LinnodEditorTriggerMode::Pad,
        TriggerMode::Chromatic => LinnodEditorTriggerMode::Chromatic,
    }
}

fn editor_playback_mode(mode: PlaybackMode) -> LinnodEditorPlaybackMode {
    match mode {
        PlaybackMode::OneShot => LinnodEditorPlaybackMode::OneShot,
        PlaybackMode::Gated => LinnodEditorPlaybackMode::Gated,
        PlaybackMode::Looped => LinnodEditorPlaybackMode::Looped,
    }
}

pub(super) fn editor_source_status(
    status: crate::SourceAnalysisStatus,
) -> LinnodEditorSourceStatus {
    match status {
        crate::SourceAnalysisStatus::Idle => LinnodEditorSourceStatus::Idle,
        crate::SourceAnalysisStatus::PendingLoad => LinnodEditorSourceStatus::PendingLoad,
        crate::SourceAnalysisStatus::Analyzing => LinnodEditorSourceStatus::Analyzing,
        crate::SourceAnalysisStatus::Ready => LinnodEditorSourceStatus::Ready,
        crate::SourceAnalysisStatus::MissingSource => LinnodEditorSourceStatus::MissingSource,
        crate::SourceAnalysisStatus::Error => LinnodEditorSourceStatus::Error,
    }
}

pub(super) fn sanitize_patch_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();
    if sanitized.is_empty() {
        "Untitled".to_string()
    } else {
        sanitized
    }
}
