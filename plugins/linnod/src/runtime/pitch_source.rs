use lindelion_dsp_utils::interpolation;
use lindelion_pitch_shift::PitchShiftRatios;

use crate::SourceAnalysis;

const IDENTITY_PITCH_RATIO_EPSILON: f32 = 1.0e-4;

pub(super) fn direct_region_sample(
    analysis: &SourceAnalysis,
    source_start_sample: usize,
    source_end_sample: usize,
    offset_samples: f32,
) -> f32 {
    let source = analysis.audio.samples();
    let source_start_sample = source_start_sample.min(source.len());
    let source_end_sample = source_end_sample.min(source.len()).max(source_start_sample);
    let offset_samples = offset_samples as f64;
    let duration = source_end_sample.saturating_sub(source_start_sample) as f64;
    if offset_samples < 0.0 || offset_samples >= duration {
        return 0.0;
    }
    interpolation::cubic_f64(source, source_start_sample as f64 + offset_samples)
}

pub(super) fn is_identity_pitch_request(ratios: PitchShiftRatios) -> bool {
    (ratios.pitch_ratio - 1.0).abs() <= IDENTITY_PITCH_RATIO_EPSILON
        && ratios
            .formant_ratio
            .is_none_or(|ratio| (ratio - 1.0).abs() <= IDENTITY_PITCH_RATIO_EPSILON)
}
