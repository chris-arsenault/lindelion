use lindelion_dsp_utils::math::finite_or;
use realfft::{RealFftPlanner, RealToComplex};

use crate::{
    PitchShiftAnalysisConfig, ResidualEnergyDescriptor, SpectralEnvelope, SpectralEnvelopePoint,
};

pub struct SpectralFrameAnalysis {
    pub start_sample: usize,
    pub end_sample: usize,
    pub envelope: SpectralEnvelope,
    pub residual: ResidualEnergyDescriptor,
}

pub fn analyze_spectral_frame(
    audio: &[f32],
    sample_rate: u32,
    center_sample: usize,
    f0_hz: Option<f32>,
    config: PitchShiftAnalysisConfig,
) -> SpectralFrameAnalysis {
    let frame_size = adaptive_frame_size(config.frame_size, sample_rate, f0_hz);
    let center_sample = center_sample.min(audio.len() - 1);
    let half = frame_size / 2;
    let start_sample = center_sample.saturating_sub(half);
    let end_sample = center_sample.saturating_add(half).min(audio.len());
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let magnitudes = frame_magnitudes(audio, start_sample, frame_size, &*fft);
    let bin_hz = sample_rate as f32 / frame_size as f32;
    let residual = residual_descriptor(&magnitudes, bin_hz, f0_hz);
    SpectralFrameAnalysis {
        start_sample,
        end_sample,
        envelope: spectral_envelope(&magnitudes, bin_hz, f0_hz, config),
        residual,
    }
}

fn adaptive_frame_size(configured: usize, sample_rate: u32, f0_hz: Option<f32>) -> usize {
    let configured = configured.max(256).next_power_of_two();
    let Some(f0_hz) = f0_hz.filter(|f0| f0.is_finite() && *f0 > 0.0) else {
        return configured;
    };
    let four_periods = (sample_rate as f32 / f0_hz * 4.0).round() as usize;
    four_periods.clamp(256, configured).next_power_of_two()
}

fn frame_magnitudes(
    audio: &[f32],
    start_sample: usize,
    frame_size: usize,
    fft: &dyn RealToComplex<f32>,
) -> Vec<f32> {
    let mut frame = fft.make_input_vec();
    for (index, sample) in frame.iter_mut().enumerate() {
        let source = audio.get(start_sample + index).copied().unwrap_or(0.0);
        *sample = finite_or(source, 0.0) * hann(index, frame_size);
    }

    let mut spectrum = fft.make_output_vec();
    if fft.process(&mut frame, &mut spectrum).is_err() {
        return Vec::new();
    }
    spectrum.iter().map(|bin| bin.norm_sqr().sqrt()).collect()
}

fn spectral_envelope(
    magnitudes: &[f32],
    bin_hz: f32,
    f0_hz: Option<f32>,
    config: PitchShiftAnalysisConfig,
) -> SpectralEnvelope {
    let point_count = config.envelope_points.min(magnitudes.len()).max(1);
    let nyquist_hz = bin_hz * magnitudes.len().saturating_sub(1) as f32;
    let smoothing_hz = envelope_smoothing_hz(bin_hz, f0_hz, config);
    let points = (0..point_count)
        .map(|index| {
            let frequency_hz = if point_count == 1 {
                0.0
            } else {
                nyquist_hz * index as f32 / (point_count - 1) as f32
            };
            SpectralEnvelopePoint {
                frequency_hz,
                magnitude: smoothed_magnitude(magnitudes, bin_hz, frequency_hz, smoothing_hz),
            }
        })
        .collect();
    SpectralEnvelope {
        harmonic_spacing_hz: f0_hz,
        points,
    }
}

fn envelope_smoothing_hz(bin_hz: f32, f0_hz: Option<f32>, config: PitchShiftAnalysisConfig) -> f32 {
    f0_hz
        .map(|f0| f0 * config.envelope_smoothing_harmonics)
        .unwrap_or(bin_hz * 6.0)
        .max(bin_hz)
}

fn smoothed_magnitude(
    magnitudes: &[f32],
    bin_hz: f32,
    frequency_hz: f32,
    smoothing_hz: f32,
) -> f32 {
    if magnitudes.is_empty() || bin_hz <= 0.0 {
        return 0.0;
    }
    let center_bin = (frequency_hz / bin_hz).round() as usize;
    let radius = (smoothing_hz / bin_hz).ceil().max(1.0) as usize;
    let start = center_bin.saturating_sub(radius);
    let end = (center_bin + radius + 1).min(magnitudes.len());
    rms_magnitude(&magnitudes[start..end])
}

fn residual_descriptor(
    magnitudes: &[f32],
    bin_hz: f32,
    f0_hz: Option<f32>,
) -> ResidualEnergyDescriptor {
    let total_energy = magnitudes.iter().map(|value| value * value).sum::<f32>();
    let harmonic_energy = f0_hz
        .filter(|f0| *f0 > 0.0 && bin_hz > 0.0)
        .map(|f0| harmonic_energy(magnitudes, bin_hz, f0))
        .unwrap_or(0.0)
        .min(total_energy);
    let residual_energy = (total_energy - harmonic_energy).max(0.0);
    let aperiodic_ratio = if total_energy <= f32::EPSILON {
        0.0
    } else {
        (residual_energy / total_energy).clamp(0.0, 1.0)
    };
    ResidualEnergyDescriptor {
        total_energy,
        harmonic_energy,
        residual_energy,
        aperiodic_ratio,
    }
}

fn harmonic_energy(magnitudes: &[f32], bin_hz: f32, f0_hz: f32) -> f32 {
    let nyquist_hz = bin_hz * magnitudes.len().saturating_sub(1) as f32;
    let radius_bins = (f0_hz * 0.15 / bin_hz).ceil().max(1.0) as usize;
    let mut energy = 0.0;
    let mut harmonic = f0_hz;
    while harmonic <= nyquist_hz {
        let center = (harmonic / bin_hz).round() as usize;
        let start = center.saturating_sub(radius_bins);
        let end = (center + radius_bins + 1).min(magnitudes.len());
        energy += magnitudes[start..end]
            .iter()
            .map(|value| value * value)
            .sum::<f32>();
        harmonic += f0_hz;
    }
    energy
}

fn rms_magnitude(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|value| value * value).sum::<f32>() / values.len() as f32).sqrt()
}

fn hann(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = std::f32::consts::TAU * index as f32 / (len - 1) as f32;
    0.5 - 0.5 * phase.cos()
}
