use lindelion_onset_detect::{MarkerKind, SliceMarker, normalize_markers};

use crate::{
    DetectionEdit, LinnodPatch, PlaybackEdit, SliceEdit,
    vst3_entry::messages::{
        LinnodAutoTuneEditMessage, LinnodDetectionEditMessage, LinnodMarkerEditMessage,
        LinnodPadEditMessage, LinnodPlaybackEditMessage, LinnodSliceEditMessage,
    },
};

pub(super) fn apply_pad_edit_message(patch: &mut LinnodPatch, edit: LinnodPadEditMessage) -> bool {
    patch.apply_pad_edit(edit.pad(), edit.edit())
}

pub(super) fn apply_detection_edit_message(
    patch: &mut LinnodPatch,
    edit: LinnodDetectionEditMessage,
) -> bool {
    patch.apply_detection_edit(match edit {
        LinnodDetectionEditMessage::Algorithm(algorithm) => DetectionEdit::Algorithm(algorithm),
        LinnodDetectionEditMessage::MinSliceMs(min_slice_ms) => {
            DetectionEdit::MinSliceMs(min_slice_ms)
        }
        LinnodDetectionEditMessage::LookbackFrames(lookback_frames) => {
            DetectionEdit::LookbackFrames(lookback_frames)
        }
        LinnodDetectionEditMessage::MaxFilterRadius(max_filter_radius) => {
            DetectionEdit::MaxFilterRadius(max_filter_radius)
        }
        LinnodDetectionEditMessage::GroupDelayWeight(group_delay_weight) => {
            DetectionEdit::GroupDelayWeight(group_delay_weight)
        }
        LinnodDetectionEditMessage::SpectralWindowSize(window_size) => {
            DetectionEdit::SpectralWindowSize(window_size)
        }
        LinnodDetectionEditMessage::PitchStabilityThresholdCents(threshold_cents) => {
            DetectionEdit::PitchStabilityThresholdCents(threshold_cents)
        }
        LinnodDetectionEditMessage::PitchStabilityDurationMs(duration_ms) => {
            DetectionEdit::PitchStabilityDurationMs(duration_ms)
        }
        LinnodDetectionEditMessage::EnergyFrameSize(frame_size) => {
            DetectionEdit::EnergyFrameSize(frame_size)
        }
        LinnodDetectionEditMessage::ManualGridDivisions(divisions) => {
            DetectionEdit::ManualGridDivisions(divisions)
        }
        LinnodDetectionEditMessage::ManualGridOffsetMs(offset_ms) => {
            DetectionEdit::ManualGridOffsetMs(offset_ms)
        }
    })
}

pub(super) fn apply_playback_edit_message(
    patch: &mut LinnodPatch,
    edit: LinnodPlaybackEditMessage,
) -> bool {
    patch.apply_playback_edit(match edit {
        LinnodPlaybackEditMessage::Mode(mode) => PlaybackEdit::Mode(mode),
        LinnodPlaybackEditMessage::Envelope(envelope) => PlaybackEdit::Envelope(envelope),
    })
}

pub(super) fn apply_auto_tune_edit_message(
    patch: &mut LinnodPatch,
    edit: LinnodAutoTuneEditMessage,
) -> bool {
    patch.apply_auto_tune_edit(edit.edit())
}

pub(super) fn apply_marker_edit_message(
    patch: &mut LinnodPatch,
    edit: LinnodMarkerEditMessage,
    source_len: usize,
) {
    match edit {
        LinnodMarkerEditMessage::AddUser { position_samples } => {
            patch.markers.push(SliceMarker {
                position_samples,
                kind: MarkerKind::User,
            });
        }
        LinnodMarkerEditMessage::RemoveAt { position_samples } => {
            patch
                .markers
                .retain(|marker| marker.position_samples != position_samples);
        }
    }
    patch.markers = normalize_markers(std::mem::take(&mut patch.markers), 1, source_len);
}

pub(super) fn apply_slice_edit_message(
    patch: &mut LinnodPatch,
    edit: LinnodSliceEditMessage,
) -> bool {
    match edit {
        LinnodSliceEditMessage::Select { slice_index } => select_slice(patch, slice_index),
        LinnodSliceEditMessage::Name { slice_index, name } => {
            patch.apply_slice_edit(slice_index, SliceEdit::Name(name))
        }
        LinnodSliceEditMessage::GainDb {
            slice_index,
            gain_db,
        } => patch.apply_slice_edit(slice_index, SliceEdit::GainDb(gain_db)),
        LinnodSliceEditMessage::Pan { slice_index, pan } => {
            patch.apply_slice_edit(slice_index, SliceEdit::Pan(pan))
        }
        LinnodSliceEditMessage::Pitch {
            slice_index,
            semitones,
            cents,
        } => patch.apply_slice_edit(
            slice_index,
            SliceEdit::Pitch(crate::PitchOffset { semitones, cents }),
        ),
        LinnodSliceEditMessage::Reverse {
            slice_index,
            reverse,
        } => patch.apply_slice_edit(slice_index, SliceEdit::Reverse(reverse)),
        LinnodSliceEditMessage::PlaybackOverride {
            slice_index,
            enabled,
        } => patch.apply_slice_edit(slice_index, SliceEdit::PlaybackOverride(enabled)),
        LinnodSliceEditMessage::PlaybackMode { slice_index, mode } => {
            patch.apply_slice_edit(slice_index, SliceEdit::PlaybackMode(mode))
        }
        LinnodSliceEditMessage::Envelope {
            slice_index,
            envelope,
        } => patch.apply_slice_edit(slice_index, SliceEdit::Envelope(envelope)),
        LinnodSliceEditMessage::AutoTuneOverride {
            slice_index,
            enabled,
        } => patch.apply_slice_edit(slice_index, SliceEdit::AutoTuneOverride(enabled)),
        LinnodSliceEditMessage::AutoTuneEnabled {
            slice_index,
            enabled,
        } => patch.apply_slice_edit(slice_index, SliceEdit::AutoTuneEnabled(enabled)),
        LinnodSliceEditMessage::Offsets {
            slice_index,
            start_offset_ms,
            end_offset_ms,
        } => patch.apply_slice_edit(
            slice_index,
            SliceEdit::Offsets {
                start_offset_ms,
                end_offset_ms,
            },
        ),
        LinnodSliceEditMessage::FilterCutoff {
            slice_index,
            cutoff_hz,
        } => patch.apply_slice_edit(slice_index, SliceEdit::FilterCutoff(cutoff_hz)),
    }
}

fn select_slice(patch: &mut LinnodPatch, slice_index: usize) -> bool {
    let Some(assignment) = patch
        .pad_map
        .iter()
        .find(|assignment| assignment.slice_index == slice_index)
    else {
        return false;
    };
    patch.active_chromatic_pad = assignment.pad.sanitized();
    true
}
