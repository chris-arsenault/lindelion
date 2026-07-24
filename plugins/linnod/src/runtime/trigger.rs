use lindelion_dsp_utils::{
    db_to_gain,
    math::{ms_to_samples, semitones_to_ratio},
    playback::{PlaybackCursor, PlaybackDirection, PlaybackRegion, playback_increment},
};
use lindelion_pitch_shift::{PitchShiftRatios, PitchShiftSynthesisAlgorithm};

use crate::{
    DEFAULT_PITCH_MAP_TOLERANCE_CENTS, SourceAnalysis,
    patch::{
        ChokeGroupId, DEFAULT_FILTER_CUTOFF_HZ, EnvelopeConfig, LinnodPatch, PlaybackMode,
        SLICE_COUNT, SliceParams, TriggerMode, pad_assignment_for_note,
    },
    tuning::chromatic_auto_tune_pitch_ratio,
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
    if matches!(patch.trigger_mode, TriggerMode::PitchMap) {
        return pitch_mapped_voice_trigger(patch, analysis, note, output_sample_rate);
    }

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

    let auto_tune_pitch_ratio = if patch
        .effective_auto_tune_config(resolved.slice_index)
        .enabled
    {
        chromatic_auto_tune_pitch_ratio(
            &analysis.pitch_shift_cache,
            resolved.slice_index,
            &patch.tuning,
        )
        .unwrap_or(1.0)
    } else {
        1.0
    };
    let pitch_ratio = slice.pitch.ratio()
        * auto_tune_pitch_ratio
        * semitones_to_ratio(resolved.chromatic_semitones);
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
        TriggerMode::PitchMap => {
            for note in 0..=127 {
                visit(note);
            }
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
        TriggerMode::PitchMap => None,
    }
}

fn pitch_mapped_voice_trigger(
    patch: &LinnodPatch,
    analysis: &SourceAnalysis,
    note: u8,
    output_sample_rate: f32,
) -> Option<LinnodVoiceTrigger> {
    let mapped = analysis.pitch_mapped_region(note)?;
    let source_sample_rate = analysis.audio.sample_rate();
    let playback = patch.playback.sanitized();
    let region = PlaybackRegion::new(0.0, mapped.duration_samples() as f32);
    if region.is_empty() {
        return None;
    }
    let correction_pitch_ratio =
        pitch_map_correction_ratio(patch.auto_tune.sanitized().enabled, mapped.cents_deviation);
    let playback_pitch_ratio = patch.playback_pitch_ratio(correction_pitch_ratio);
    let (algorithm, ratios) = if correction_pitch_ratio == 1.0 {
        (
            PitchShiftSynthesisAlgorithm::Auto,
            PitchShiftRatios::identity(),
        )
    } else {
        (
            patch.pitch_shift_synthesis_algorithm(),
            patch.pitch_shift_ratios(correction_pitch_ratio),
        )
    };

    Some(LinnodVoiceTrigger {
        slice_index: note as usize,
        source_start_sample: mapped.start_sample,
        source_end_sample: mapped.end_sample,
        cursor: PlaybackCursor::new(
            region,
            0.0,
            playback_increment(source_sample_rate, output_sample_rate, playback_pitch_ratio),
            PlaybackDirection::Forward,
            matches!(playback.mode, PlaybackMode::Looped),
        ),
        declick: PlaybackDeclick::new(region, output_sample_rate),
        algorithm,
        ratios,
        reverse: false,
        playback_mode: playback.mode,
        choke_group: None,
        envelope: playback.envelope,
        gain: 1.0,
        pan: 0.0,
        filter_cutoff: DEFAULT_FILTER_CUTOFF_HZ,
    })
}

fn pitch_map_correction_ratio(enabled: bool, cents_deviation: f32) -> f32 {
    if !enabled
        || !cents_deviation.is_finite()
        || cents_deviation.abs() > DEFAULT_PITCH_MAP_TOLERANCE_CENTS
    {
        return 1.0;
    }
    semitones_to_ratio(-cents_deviation / 100.0)
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
