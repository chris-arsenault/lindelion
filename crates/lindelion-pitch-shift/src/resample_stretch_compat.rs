use lindelion_dsp_utils::interpolation;

use crate::{
    PitchShiftRatios, PitchShiftSourceCache, resample_pro_render, synthesis::PitchShiftRenderError,
};

pub(crate) fn render_region_to(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    end_sample: usize,
    ratios: PitchShiftRatios,
    output: &mut [f32],
) -> Result<(), PitchShiftRenderError> {
    let rendered = resample_pro_render::render_region_pitch_shift_with_source(
        source,
        cache,
        start_sample,
        end_sample,
        ratios,
    )
    .map_err(|_| PitchShiftRenderError::InvalidCache)?;
    if output.len() != rendered.len() {
        return Err(PitchShiftRenderError::OutputLength {
            expected: rendered.len(),
            actual: output.len(),
        });
    }
    output.copy_from_slice(&rendered);
    Ok(())
}

pub(crate) fn render_sample(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    position: f64,
    ratios: PitchShiftRatios,
) -> Option<f32> {
    let center = position.floor().max(0.0) as usize;
    let start = center.saturating_sub(2);
    let end = center.saturating_add(3).min(cache.source_len_samples);
    let rendered = resample_pro_render::render_region_pitch_shift_with_source(
        source, cache, start, end, ratios,
    )
    .ok()?;
    Some(interpolation::cubic_f64(&rendered, position - start as f64))
}
