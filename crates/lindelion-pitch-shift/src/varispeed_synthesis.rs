use lindelion_dsp_utils::interpolation;

pub(crate) fn varispeed_sample(
    source: &[f32],
    start_sample: usize,
    end_sample: usize,
    offset_samples: f32,
    pitch_ratio: f32,
) -> Option<f32> {
    let start_sample = start_sample.min(source.len());
    let end_sample = end_sample.min(source.len()).max(start_sample);
    let offset_samples = offset_samples as f64;
    let pitch_ratio = pitch_ratio as f64;
    let duration = end_sample.saturating_sub(start_sample) as f64;
    if duration <= 0.0 || offset_samples < 0.0 || pitch_ratio <= 0.0 || !pitch_ratio.is_finite() {
        return Some(0.0);
    }

    let source_offset = offset_samples * pitch_ratio;
    if source_offset >= duration {
        return Some(0.0);
    }
    Some(interpolation::cubic_f64(
        source,
        start_sample as f64 + source_offset,
    ))
}
