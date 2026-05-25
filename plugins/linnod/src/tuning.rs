use lindelion_dsp_utils::math::finite_clamp;
use lindelion_midi::nearest_scale_midi_note;
use lindelion_pitch_shift::{PitchShiftSliceSummary, PitchShiftSourceCache};

use crate::patch::{
    LinnodPatch, MAX_TUNING_REFERENCE_HZ, MIN_TUNING_REFERENCE_HZ, PitchOffset, TuningConfig,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceTuningInfo {
    pub slice_index: usize,
    pub detected_f0_hz: f32,
    pub detected_midi_note: f32,
    pub nearest_midi_note: u8,
    pub nearest_scale_midi_note: u8,
    pub cents_deviation: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceTuneTarget {
    NearestChromatic,
    NearestScale,
}

pub fn slice_tuning_info(
    cache: &PitchShiftSourceCache,
    slice_index: usize,
    tuning: &TuningConfig,
) -> Option<SliceTuningInfo> {
    tuning_info_from_summary(cache.slice_summary(slice_index)?, tuning)
}

pub fn tune_slice_to_nearest_note(
    patch: &mut LinnodPatch,
    cache: &PitchShiftSourceCache,
    slice_index: usize,
) -> bool {
    apply_slice_tuning(patch, cache, slice_index, SliceTuneTarget::NearestChromatic)
}

pub fn tune_all_slices_to_nearest_notes(
    patch: &mut LinnodPatch,
    cache: &PitchShiftSourceCache,
) -> usize {
    tune_all_slices_to_nearest_note_with_target(patch, cache, SliceTuneTarget::NearestChromatic)
}

pub fn snap_slice_to_scale(
    patch: &mut LinnodPatch,
    cache: &PitchShiftSourceCache,
    slice_index: usize,
) -> bool {
    apply_slice_tuning(patch, cache, slice_index, SliceTuneTarget::NearestScale)
}

pub fn snap_all_slices_to_scale(patch: &mut LinnodPatch, cache: &PitchShiftSourceCache) -> usize {
    tune_all_slices_to_nearest_note_with_target(patch, cache, SliceTuneTarget::NearestScale)
}

fn tune_all_slices_to_nearest_note_with_target(
    patch: &mut LinnodPatch,
    cache: &PitchShiftSourceCache,
    target: SliceTuneTarget,
) -> usize {
    let indices = cache
        .slice_summaries
        .iter()
        .map(|summary| summary.slice_index)
        .collect::<Vec<_>>();
    indices
        .into_iter()
        .filter(|slice_index| apply_slice_tuning(patch, cache, *slice_index, target))
        .count()
}

fn apply_slice_tuning(
    patch: &mut LinnodPatch,
    cache: &PitchShiftSourceCache,
    slice_index: usize,
    target: SliceTuneTarget,
) -> bool {
    let Some(info) = slice_tuning_info(cache, slice_index, &patch.tuning) else {
        return false;
    };
    let target_note = match target {
        SliceTuneTarget::NearestChromatic => info.nearest_midi_note,
        SliceTuneTarget::NearestScale => info.nearest_scale_midi_note,
    };
    let target_hz = hz_from_midi_note_with_reference(target_note as f32, patch.tuning.reference_hz);
    let Some(slice) = patch.slice_mut(slice_index) else {
        return false;
    };
    slice.pitch = PitchOffset::from_frequency_ratio(target_hz / info.detected_f0_hz);
    true
}

fn tuning_info_from_summary(
    summary: &PitchShiftSliceSummary,
    tuning: &TuningConfig,
) -> Option<SliceTuningInfo> {
    let detected_f0_hz = summary.detected_f0_hz?;
    let detected_midi_note = midi_note_from_hz_with_reference(detected_f0_hz, tuning.reference_hz)?;
    let nearest_midi_note = detected_midi_note.round().clamp(0.0, 127.0) as u8;
    let nearest_scale = nearest_scale_midi_note(detected_midi_note, tuning.root, &tuning.scale);
    Some(SliceTuningInfo {
        slice_index: summary.slice_index,
        detected_f0_hz,
        detected_midi_note,
        nearest_midi_note,
        nearest_scale_midi_note: nearest_scale,
        cents_deviation: (detected_midi_note - nearest_midi_note as f32) * 100.0,
    })
}

fn midi_note_from_hz_with_reference(frequency_hz: f32, reference_hz: f32) -> Option<f32> {
    let reference_hz = sanitized_reference_hz(reference_hz);
    if frequency_hz > 0.0 && frequency_hz.is_finite() {
        Some(69.0 + 12.0 * (frequency_hz / reference_hz).log2())
    } else {
        None
    }
}

fn hz_from_midi_note_with_reference(note: f32, reference_hz: f32) -> f32 {
    sanitized_reference_hz(reference_hz) * 2.0_f32.powf((note - 69.0) / 12.0)
}

fn sanitized_reference_hz(reference_hz: f32) -> f32 {
    finite_clamp(
        reference_hz,
        MIN_TUNING_REFERENCE_HZ,
        MAX_TUNING_REFERENCE_HZ,
        crate::patch::DEFAULT_TUNING_REFERENCE_HZ,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_midi::{RootNote, Scale};
    use lindelion_pitch_shift::{
        PitchShiftAnalysisConfig, PitchShiftSourceCache, SourceCacheKey, VoicingKind,
    };

    #[test]
    fn slice_tuning_info_reports_detected_pitch_and_cents_deviation() {
        let cache = cache_with_summaries(&[(0, 443.0)]);
        let info = slice_tuning_info(&cache, 0, &TuningConfig::default()).unwrap();

        assert_eq!(info.nearest_midi_note, 69);
        assert!((info.cents_deviation - 11.76).abs() < 0.1);
    }

    #[test]
    fn tune_one_and_all_apply_slice_pitch_offsets() {
        let cache = cache_with_summaries(&[(0, 443.0), (1, 220.0)]);
        let mut patch = LinnodPatch::default();

        assert!(tune_slice_to_nearest_note(&mut patch, &cache, 0));
        assert!((patch.slices[0].pitch.cents + 11.76).abs() < 0.2);

        let count = tune_all_slices_to_nearest_notes(&mut patch, &cache);
        assert_eq!(count, 2);
        assert_eq!(patch.slices[1].pitch.semitones, 0);
    }

    #[test]
    fn scale_snap_uses_shared_midi_scale_degree_search() {
        let mut patch = LinnodPatch {
            tuning: TuningConfig {
                scale: Scale::Major,
                root: RootNote::C,
                ..TuningConfig::default()
            },
            ..LinnodPatch::default()
        };
        let detected = hz_from_midi_note_with_reference(66.8, 440.0);
        let cache = cache_with_summaries(&[(0, detected)]);

        assert!(snap_slice_to_scale(&mut patch, &cache, 0));

        assert!(patch.slices[0].pitch.cents > 15.0);
        assert_eq!(patch.slices[0].pitch.semitones, 0);
    }

    fn cache_with_summaries(values: &[(usize, f32)]) -> PitchShiftSourceCache {
        PitchShiftSourceCache {
            key: SourceCacheKey {
                sample_rate: 48_000,
                source_len_samples: 4_800,
                audio_hash: 0,
                marker_hash: 0,
                pitch_contour_hash: 0,
                config_hash: 0,
            },
            sample_rate: 48_000,
            source_len_samples: 4_800,
            config: PitchShiftAnalysisConfig::default(),
            frames: Vec::new(),
            voicing_segments: vec![lindelion_pitch_shift::VoicingSegment {
                kind: VoicingKind::Voiced,
                start_sample: 0,
                end_sample: 4_800,
                frame_count: values.len(),
                median_f0_hz: values.first().map(|(_, f0)| *f0),
                mean_confidence: 0.95,
                mean_rms: 0.2,
            }],
            slice_summaries: values
                .iter()
                .map(|(slice_index, f0)| PitchShiftSliceSummary {
                    slice_index: *slice_index,
                    start_sample: slice_index * 2_400,
                    end_sample: (slice_index + 1) * 2_400,
                    detected_f0_hz: Some(*f0),
                    mean_confidence: 0.95,
                    voiced_ratio: 1.0,
                    mean_residual_energy: 0.0,
                    mean_aperiodic_ratio: 0.0,
                })
                .collect(),
        }
    }
}
