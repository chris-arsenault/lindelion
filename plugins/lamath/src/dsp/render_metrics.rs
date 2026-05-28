use lindelion_dsp_utils::analysis::{
    AudioWindowMetrics, HarmonicDecayMeasurement, audio_window_metrics, gain_fitted_rms_difference,
    harmonic_decay_profile, rms, rms_difference, sampled_high_frequency_ratio,
};

use super::{
    modal::{ModalBank, ModalBankParams},
    waveguide::{WaveguideParams, WaveguideResonator},
};

const EARLY_METRIC_START: usize = 512;
const EARLY_METRIC_END: usize = 2_560;
const LATE_METRIC_START: usize = 12_000;
const LATE_METRIC_END: usize = 14_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderExcitation {
    Impulse,
    ShapedPluck,
    NoiseBurst,
    SidechainBurst,
    Sustained,
}

impl RenderExcitation {
    pub(crate) const ALL: [Self; 5] = [
        Self::Impulse,
        Self::ShapedPluck,
        Self::NoiseBurst,
        Self::SidechainBurst,
        Self::Sustained,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RenderMetricProfile {
    pub(crate) early: AudioWindowMetrics,
    pub(crate) late: AudioWindowMetrics,
    pub(crate) harmonic_decay: Vec<HarmonicDecayMeasurement>,
    pub(crate) high_frequency_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RenderComparisonMetrics {
    pub(crate) rms_difference: f32,
    pub(crate) gain_fitted_rms_difference: f32,
    pub(crate) normalized_shape_difference: f32,
    pub(crate) early_centroid_delta_hz: f32,
    pub(crate) high_frequency_ratio_delta: f32,
}

pub(crate) fn render_response(
    sample_rate: f32,
    frequency_hz: f32,
    sample_count: usize,
    excitation: RenderExcitation,
    mut process_sample: impl FnMut(f32) -> f32,
) -> Vec<f32> {
    let mut output = Vec::with_capacity(sample_count);

    for index in 0..sample_count {
        output.push(process_sample(excitation_sample(
            index,
            sample_rate,
            frequency_hz,
            excitation,
        )));
    }

    output
}

pub(crate) fn render_modal_response(
    sample_rate: f32,
    params: ModalBankParams,
    sample_count: usize,
    excitation: RenderExcitation,
) -> Vec<f32> {
    let mut bank = ModalBank::new(sample_rate, params);
    render_response(
        sample_rate,
        params.fundamental_hz,
        sample_count,
        excitation,
        |sample| bank.process_sample(sample),
    )
}

pub(crate) fn render_waveguide_response(
    sample_rate: f32,
    params: WaveguideParams,
    sample_count: usize,
    excitation: RenderExcitation,
) -> Vec<f32> {
    let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
    render_response(
        sample_rate,
        params.frequency_hz,
        sample_count,
        excitation,
        |sample| waveguide.process_sample(sample, params),
    )
}

pub(crate) fn render_metric_profile(
    samples: &[f32],
    sample_rate: f32,
    fundamental_hz: f32,
) -> RenderMetricProfile {
    let early = clamped_slice(samples, EARLY_METRIC_START, EARLY_METRIC_END);
    let late = clamped_slice(samples, LATE_METRIC_START, LATE_METRIC_END);
    let high_frequency_start = (fundamental_hz * 6.0).clamp(2_000.0, sample_rate * 0.45);
    let high_frequency_step = fundamental_hz.clamp(80.0, 1_200.0);

    RenderMetricProfile {
        early: audio_window_metrics(early, sample_rate),
        late: audio_window_metrics(late, sample_rate),
        harmonic_decay: harmonic_decay_profile(
            samples,
            sample_rate,
            fundamental_hz,
            8,
            EARLY_METRIC_START..EARLY_METRIC_END,
            LATE_METRIC_START..LATE_METRIC_END,
        ),
        high_frequency_ratio: sampled_high_frequency_ratio(
            early,
            sample_rate,
            high_frequency_start,
            high_frequency_step,
        ),
    }
}

pub(crate) fn compare_render_metrics(
    reference: &[f32],
    rendered: &[f32],
    sample_rate: f32,
    fundamental_hz: f32,
) -> RenderComparisonMetrics {
    let reference_profile = render_metric_profile(reference, sample_rate, fundamental_hz);
    let rendered_profile = render_metric_profile(rendered, sample_rate, fundamental_hz);
    let reference_tail = tail_after_initial_transient(reference);
    let rendered_tail = tail_after_initial_transient(rendered);
    let gain_fitted_rms_difference = gain_fitted_rms_difference(reference_tail, rendered_tail);
    let normalization = rms(rendered_tail).max(1.0e-12);
    let reference_centroid = reference_profile.early.spectral_centroid_hz.unwrap_or(0.0);
    let rendered_centroid = rendered_profile.early.spectral_centroid_hz.unwrap_or(0.0);

    RenderComparisonMetrics {
        rms_difference: rms_difference(reference_tail, rendered_tail),
        gain_fitted_rms_difference,
        normalized_shape_difference: gain_fitted_rms_difference / normalization,
        early_centroid_delta_hz: (reference_centroid - rendered_centroid).abs(),
        high_frequency_ratio_delta: (reference_profile.high_frequency_ratio
            - rendered_profile.high_frequency_ratio)
            .abs(),
    }
}

fn excitation_sample(
    index: usize,
    sample_rate: f32,
    frequency_hz: f32,
    excitation: RenderExcitation,
) -> f32 {
    match excitation {
        RenderExcitation::Impulse => (index == 0) as u8 as f32,
        RenderExcitation::ShapedPluck => shaped_pluck(index, sample_rate),
        RenderExcitation::NoiseBurst => {
            deterministic_noise(index) * burst_envelope(index, 384) * 0.35
        }
        RenderExcitation::SidechainBurst => {
            deterministic_noise(index + 17) * burst_envelope(index, 1_536) * 0.2
        }
        RenderExcitation::Sustained => {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate;
            phase.sin() * 0.015
        }
    }
}

fn shaped_pluck(index: usize, sample_rate: f32) -> f32 {
    let len = (sample_rate * 0.004).round() as usize;
    if index >= len || len < 2 {
        return 0.0;
    }
    let phase = index as f32 / (len - 1) as f32;
    (std::f32::consts::PI * phase).sin() * (1.0 - phase) * 0.8
}

fn burst_envelope(index: usize, len: usize) -> f32 {
    if index >= len || len < 2 {
        return 0.0;
    }
    let phase = index as f32 / (len - 1) as f32;
    (std::f32::consts::PI * phase).sin().powi(2)
}

fn deterministic_noise(index: usize) -> f32 {
    let mut state = index as u32 ^ 0x9E37_79B9;
    state ^= state >> 16;
    state = state.wrapping_mul(0x7FEB_352D);
    state ^= state >> 15;
    state = state.wrapping_mul(0x846C_A68B);
    state ^= state >> 16;
    (state as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn tail_after_initial_transient(samples: &[f32]) -> &[f32] {
    clamped_slice(samples, EARLY_METRIC_START, samples.len())
}

fn clamped_slice(samples: &[f32], start: usize, end: usize) -> &[f32] {
    let start = start.min(samples.len());
    let end = end.min(samples.len());
    if start >= end {
        &samples[0..0]
    } else {
        &samples[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::assert_all_finite;

    #[test]
    fn render_response_covers_all_excitation_shapes() {
        let sample_rate = 48_000.0;

        for excitation in RenderExcitation::ALL {
            let output = render_response(sample_rate, 220.0, 2_048, excitation, |sample| sample);
            let metrics = audio_window_metrics(&output, sample_rate);

            assert_all_finite(&output);
            assert!(metrics.peak_abs > 0.0, "{excitation:?}");
            assert!(metrics.peak_abs <= 1.0, "{excitation:?}");
        }
    }

    #[test]
    fn comparison_metrics_ignore_simple_gain_changes() {
        let sample_rate = 48_000.0;
        let reference = sine_wave(220.0, sample_rate, 8_192);
        let scaled = reference
            .iter()
            .map(|sample| sample * 0.25)
            .collect::<Vec<_>>();
        let different = sine_wave(660.0, sample_rate, 8_192);

        let scaled_comparison = compare_render_metrics(&reference, &scaled, sample_rate, 220.0);
        let different_comparison =
            compare_render_metrics(&reference, &different, sample_rate, 220.0);

        assert!(scaled_comparison.normalized_shape_difference < 0.000_01);
        assert!(
            different_comparison.normalized_shape_difference
                > scaled_comparison.normalized_shape_difference + 0.1
        );
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }
}
