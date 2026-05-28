use lindelion_dsp_utils::{
    db_to_gain,
    math::{ms_to_samples, semitones_to_ratio},
    playback::{PlaybackCursor, PlaybackDirection, PlaybackRegion, playback_increment},
};
use lindelion_pitch_shift::{PitchShiftRatios, PitchShiftSynthesisAlgorithm};

use crate::{
    SourceAnalysis,
    patch::{
        ChokeGroupId, EnvelopeConfig, LinnodPatch, PlaybackMode, SLICE_COUNT, SliceParams,
        TriggerMode, pad_assignment_for_note,
    },
};

use super::declick::PlaybackDeclick;

#[derive(Debug, Clone, Copy)]
pub(super) struct LinnodVoiceTrigger {
    pub(super) slice_index: usize,
    pub(super) source_start_sample: usize,
    pub(super) source_end_sample: usize,
    pub(super) cursor: PlaybackCursor,
    pub(super) declick: PlaybackDeclick,
    pub(super) algorithm: PitchShiftSynthesisAlgorithm,
    pub(super) ratios: PitchShiftRatios,
    pub(super) reverse: bool,
    pub(super) playback_mode: PlaybackMode,
    pub(super) choke_group: Option<ChokeGroupId>,
    pub(super) envelope: EnvelopeConfig,
    pub(super) gain: f32,
    pub(super) pan: f32,
    pub(super) filter_cutoff: f32,
}

pub(super) fn voice_trigger_from_note(
    patch: &LinnodPatch,
    analysis: &SourceAnalysis,
    note: u8,
    output_sample_rate: f32,
    _velocity: f32,
) -> Option<LinnodVoiceTrigger> {
    let resolved = resolve_note_trigger(patch, note)?;
    let slice = patch.slice(resolved.slice_index)?;
    let summary = analysis
        .pitch_shift_cache
        .slice_summary(resolved.slice_index)
        .copied()?;
    let source_sample_rate = analysis.audio.sample_rate();
    let playback = patch.effective_playback_config(resolved.slice_index);
    let source_start_sample = summary.start_sample;
    let source_end_sample = slice_playback_end_sample(
        playback.mode,
        summary.end_sample,
        analysis.audio.samples().len(),
    );
    let region = slice_playback_region(
        slice,
        playback.mode,
        source_start_sample,
        source_end_sample,
        analysis.pitch_shift_cache.sample_rate,
    );
    if region.is_empty() {
        return None;
    }

    let pitch_ratio = slice.pitch.ratio() * semitones_to_ratio(resolved.chromatic_semitones);
    let playback_pitch_ratio = patch.playback_pitch_ratio(pitch_ratio);
    Some(LinnodVoiceTrigger {
        slice_index: resolved.slice_index,
        source_start_sample,
        source_end_sample,
        cursor: PlaybackCursor::new(
            region,
            0.0,
            playback_increment(source_sample_rate, output_sample_rate, playback_pitch_ratio),
            if slice.reverse {
                PlaybackDirection::Reverse
            } else {
                PlaybackDirection::Forward
            },
            matches!(playback.mode, PlaybackMode::Looped),
        ),
        declick: PlaybackDeclick::new(region, output_sample_rate),
        algorithm: patch.pitch_shift_synthesis_algorithm(),
        ratios: patch.pitch_shift_ratios(pitch_ratio),
        reverse: slice.reverse,
        playback_mode: playback.mode,
        choke_group: resolved.choke_group,
        envelope: playback.envelope,
        gain: db_to_gain(slice.gain_db),
        pan: slice.pan,
        filter_cutoff: slice.filter_cutoff,
    })
}

pub(super) fn for_each_preparable_trigger_note(patch: &LinnodPatch, mut visit: impl FnMut(u8)) {
    match patch.trigger_mode {
        TriggerMode::Pad => {
            for assignment in &patch.pad_map {
                visit(assignment.midi_note);
            }
        }
        TriggerMode::Chromatic => {
            let root_note = patch
                .pad_map
                .iter()
                .find(|assignment| {
                    assignment.pad.sanitized() == patch.active_chromatic_pad.sanitized()
                })
                .map(|assignment| assignment.midi_note)
                .unwrap_or(60);
            visit(root_note);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NoteTriggerResolution {
    pub(super) slice_index: usize,
    pub(super) chromatic_semitones: f32,
    pub(super) choke_group: Option<ChokeGroupId>,
}

pub(super) fn resolve_note_trigger(patch: &LinnodPatch, note: u8) -> Option<NoteTriggerResolution> {
    match patch.trigger_mode {
        TriggerMode::Pad => {
            pad_assignment_for_note(&patch.pad_map, note).map(|assignment| NoteTriggerResolution {
                slice_index: assignment.slice_index.min(SLICE_COUNT - 1),
                chromatic_semitones: 0.0,
                choke_group: assignment.choke_group,
            })
        }
        TriggerMode::Chromatic => {
            let slice_index = patch.selected_slice_index()?;
            let root_note = patch
                .pad_map
                .iter()
                .find(|assignment| {
                    assignment.pad.sanitized() == patch.active_chromatic_pad.sanitized()
                })
                .map(|assignment| assignment.midi_note)
                .unwrap_or(60);
            Some(NoteTriggerResolution {
                slice_index,
                chromatic_semitones: note as f32 - root_note as f32,
                choke_group: None,
            })
        }
    }
}

fn slice_playback_region(
    slice: &SliceParams,
    playback_mode: PlaybackMode,
    start_sample: usize,
    end_sample: usize,
    source_sample_rate: u32,
) -> PlaybackRegion {
    let duration = end_sample.saturating_sub(start_sample);
    let start_offset = ms_to_samples(slice.start_offset_ms, source_sample_rate).min(duration);
    let end_offset = if matches!(playback_mode, PlaybackMode::Continue) {
        0
    } else {
        ms_to_samples(slice.end_offset_ms, source_sample_rate).min(duration - start_offset)
    };
    PlaybackRegion::new(
        start_offset as f32,
        duration.saturating_sub(end_offset) as f32,
    )
}

fn slice_playback_end_sample(
    playback_mode: PlaybackMode,
    slice_end_sample: usize,
    source_len: usize,
) -> usize {
    if matches!(playback_mode, PlaybackMode::Continue) {
        source_len
    } else {
        slice_end_sample
    }
}
