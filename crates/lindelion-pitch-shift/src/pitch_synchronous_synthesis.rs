use lindelion_dsp_utils::interpolation;

use crate::{
    PitchShiftSourceCache,
    synthesis::PitchShiftRenderConfig,
    synthesis_support::{frame_at_position, raised_cosine_window},
};

const PSOLA_CENTER_SEARCH_RADIUS: isize = 4;
const PSOLA_MIN_WINDOW_RADIUS_SAMPLES: f32 = 32.0;
const PSOLA_MAX_WINDOW_RADIUS_SAMPLES: f32 = 4096.0;

pub(crate) fn pitch_synchronous_sample(
    source: &[f32],
    cache: &PitchShiftSourceCache,
    start_sample: usize,
    end_sample: usize,
    offset_samples: f32,
    config: PitchShiftRenderConfig,
) -> Option<f32> {
    let absolute_position = start_sample as f64 + offset_samples as f64;
    let frame = frame_at_position(&cache.frames, absolute_position as usize);
    let Some(f0_hz) = frame.f0_hz.filter(|f0| frame.voiced && *f0 > 0.0) else {
        return Some(interpolation::linear_f64(source, absolute_position) * config.unvoiced_level);
    };
    if cache.epoch_samples.is_empty() {
        return None;
    }
    let ratios = config.ratios.sanitized();
    let source_period = cache.sample_rate as f32 / f0_hz;
    let target_period = source_period / ratios.pitch_ratio;
    if source_period <= 0.0
        || target_period <= 0.0
        || !source_period.is_finite()
        || !target_period.is_finite()
    {
        return None;
    }

    let origin = psola_origin_offset(cache, start_sample)?;
    let nearest_center = ((offset_samples - origin) / target_period).round() as isize;
    let window_radius = psola_window_radius(source_period, target_period);
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    for center_index in
        nearest_center - PSOLA_CENTER_SEARCH_RADIUS..=nearest_center + PSOLA_CENTER_SEARCH_RADIUS
    {
        let synth_center = origin + center_index as f32 * target_period;
        let distance = offset_samples - synth_center;
        if distance.abs() > window_radius {
            continue;
        }
        let source_center = nearest_epoch_sample(cache, start_sample as f64 + synth_center as f64)?;
        let source_position = source_center as f64 + (distance * ratios.pitch_ratio) as f64;
        if source_position < start_sample as f64
            || source_position >= end_sample.saturating_sub(1) as f64
        {
            continue;
        }
        let weight = raised_cosine_window(distance / window_radius);
        weighted_sum += interpolation::linear_f64(source, source_position) * weight;
        weight_sum += weight;
    }

    (weight_sum > f32::EPSILON).then_some(weighted_sum / weight_sum * config.harmonic_level)
}

fn psola_origin_offset(cache: &PitchShiftSourceCache, start_sample: usize) -> Option<f32> {
    nearest_epoch_sample(cache, start_sample as f64).map(|epoch| epoch as f32 - start_sample as f32)
}

fn nearest_epoch_sample(cache: &PitchShiftSourceCache, position_samples: f64) -> Option<usize> {
    if cache.epoch_samples.is_empty() {
        return None;
    }
    let position = position_samples.round().max(0.0) as usize;
    let right = cache
        .epoch_samples
        .partition_point(|epoch| *epoch < position);
    let left = right.saturating_sub(1);
    match (
        cache.epoch_samples.get(left),
        cache.epoch_samples.get(right),
    ) {
        (Some(left), Some(right)) => {
            if position.saturating_sub(*left) <= right.saturating_sub(position) {
                Some(*left)
            } else {
                Some(*right)
            }
        }
        (Some(left), None) => Some(*left),
        (None, Some(right)) => Some(*right),
        (None, None) => None,
    }
}

fn psola_window_radius(source_period: f32, target_period: f32) -> f32 {
    (source_period * 1.5).max(target_period * 0.75).clamp(
        PSOLA_MIN_WINDOW_RADIUS_SAMPLES,
        PSOLA_MAX_WINDOW_RADIUS_SAMPLES,
    )
}
