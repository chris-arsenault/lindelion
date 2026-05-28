use crate::{
    PitchShiftFrameAnalysis, PitchShiftRatios, SpectralEnvelope,
    synthesis::{PitchShiftRenderConfig, ResidualMixPolicy},
};

const FORMANT_ENVELOPE_GAIN_MIN: f64 = 1.0 / 32.0;
const FORMANT_ENVELOPE_GAIN_MAX: f64 = 32.0;
const FORMANT_ENVELOPE_FLOOR: f32 = 1.0e-9;

pub(crate) fn frame_at_position(
    frames: &[PitchShiftFrameAnalysis],
    position_samples: usize,
) -> &PitchShiftFrameAnalysis {
    let index = frames.partition_point(|frame| frame.center_sample <= position_samples);
    &frames[index.saturating_sub(1).min(frames.len() - 1)]
}

pub(crate) fn frame_pair_at_position(
    frames: &[PitchShiftFrameAnalysis],
    position_samples: usize,
) -> (&PitchShiftFrameAnalysis, &PitchShiftFrameAnalysis, f32) {
    let right = frames.partition_point(|frame| frame.center_sample <= position_samples);
    let left = right.saturating_sub(1).min(frames.len() - 1);
    let right = right.min(frames.len() - 1);
    let left_frame = &frames[left];
    let right_frame = &frames[right];
    if left == right {
        return (left_frame, right_frame, 0.0);
    }
    let span = right_frame
        .center_sample
        .saturating_sub(left_frame.center_sample)
        .max(1) as f32;
    let mix =
        (position_samples.saturating_sub(left_frame.center_sample) as f32 / span).clamp(0.0, 1.0);
    (left_frame, right_frame, mix)
}

pub(crate) fn raised_cosine_window(normalized_distance: f32) -> f32 {
    let distance = normalized_distance.abs();
    if distance >= 1.0 {
        0.0
    } else {
        0.5 + 0.5 * (std::f32::consts::PI * distance).cos()
    }
}

pub(crate) fn residual_sample(
    source_sample: f32,
    frame: &PitchShiftFrameAnalysis,
    config: PitchShiftRenderConfig,
) -> f32 {
    match config.residual_policy {
        ResidualMixPolicy::Muted => 0.0,
        ResidualMixPolicy::Preserve if frame.voiced => {
            source_sample * frame.residual.aperiodic_ratio * config.residual_level
        }
        ResidualMixPolicy::Preserve => source_sample * config.unvoiced_level,
    }
}

pub(crate) fn spectral_envelope_formant_gain(
    envelope: &SpectralEnvelope,
    source_frequency_hz: f64,
    ratios: PitchShiftRatios,
) -> f64 {
    if source_frequency_hz <= 0.0 || !source_frequency_hz.is_finite() {
        return 1.0;
    }
    let ratios = ratios.sanitized();
    let pitch_ratio = ratios.pitch_ratio as f64;
    let envelope_shift = ratios.effective_formant_ratio() as f64;
    if pitch_ratio <= 0.0 || envelope_shift <= 0.0 {
        return 1.0;
    }

    let final_frequency_hz = source_frequency_hz * pitch_ratio;
    let target_envelope_hz = final_frequency_hz / envelope_shift;
    let source_envelope = envelope
        .magnitude_at(source_frequency_hz as f32)
        .max(FORMANT_ENVELOPE_FLOOR) as f64;
    let target_envelope = envelope
        .magnitude_at(target_envelope_hz as f32)
        .max(FORMANT_ENVELOPE_FLOOR) as f64;

    (target_envelope / source_envelope).clamp(FORMANT_ENVELOPE_GAIN_MIN, FORMANT_ENVELOPE_GAIN_MAX)
}
