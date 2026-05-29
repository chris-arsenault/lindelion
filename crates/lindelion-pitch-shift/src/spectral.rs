use lindelion_dsp_utils::{math::finite_or, window};
use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::{
    PitchShiftAnalysisConfig, ResidualEnergyDescriptor, SpectralEnvelope, SpectralEnvelopePoint,
    SpectralPeak,
};

pub struct SpectralFrameAnalysis {
    pub start_sample: usize,
    pub end_sample: usize,
    pub rms: f32,
    pub harmonic_magnitudes: Vec<f32>,
    pub spectral_peaks: Vec<SpectralPeak>,
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
    let spectrum = frame_spectrum(audio, start_sample, frame_size, &*fft);
    let magnitudes = spectrum
        .iter()
        .map(|bin| bin.norm_sqr().sqrt())
        .collect::<Vec<_>>();
    let bin_hz = sample_rate as f32 / frame_size as f32;
    let residual = residual_descriptor(&magnitudes, bin_hz, f0_hz);
    SpectralFrameAnalysis {
        start_sample,
        end_sample,
        rms: frame_rms(audio, start_sample, end_sample),
        harmonic_magnitudes: harmonic_magnitudes(&magnitudes, bin_hz, f0_hz),
        spectral_peaks: spectral_peaks(&spectrum, &magnitudes, bin_hz, f0_hz),
        envelope: spectral_envelope(&magnitudes, bin_hz, f0_hz, config, &*fft),
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

fn frame_spectrum(
    audio: &[f32],
    start_sample: usize,
    frame_size: usize,
    fft: &dyn RealToComplex<f32>,
) -> Vec<realfft::num_complex::Complex32> {
    let mut frame = fft.make_input_vec();
    for (index, sample) in frame.iter_mut().enumerate() {
        let source = audio.get(start_sample + index).copied().unwrap_or(0.0);
        *sample = finite_or(source, 0.0) * window::hann(index, frame_size);
    }

    let mut spectrum = fft.make_output_vec();
    if fft.process(&mut frame, &mut spectrum).is_err() {
        return Vec::new();
    }
    spectrum
}

/// Iterative true-envelope budget. The estimator usually converges in well under this
/// many passes; the cap bounds the offline analysis cost (Röbel & Rodet, DAFx-05).
const TRUE_ENVELOPE_MAX_ITERS: usize = 16;
/// Convergence reached once the original spectrum rises no more than this above the
/// smoothed envelope anywhere (the envelope rides all peaks within tolerance).
const TRUE_ENVELOPE_THRESHOLD_DB: f32 = 2.0;
/// Magnitude floor before taking the log, so spectral zeros do not produce `-inf`.
const TRUE_ENVELOPE_LOG_FLOOR: f32 = 1.0e-9;

fn spectral_envelope(
    magnitudes: &[f32],
    bin_hz: f32,
    f0_hz: Option<f32>,
    config: PitchShiftAnalysisConfig,
    fft: &dyn RealToComplex<f32>,
) -> SpectralEnvelope {
    let point_count = config.envelope_points.min(magnitudes.len()).max(1);
    let nyquist_hz = bin_hz * magnitudes.len().saturating_sub(1) as f32;
    let lifter_order = true_envelope_lifter_order(magnitudes.len(), bin_hz, f0_hz);
    let envelope = true_envelope_magnitudes(magnitudes, lifter_order, fft);
    let last_bin = envelope.len().saturating_sub(1);
    let points = (0..point_count)
        .map(|index| {
            let frequency_hz = if point_count == 1 {
                0.0
            } else {
                nyquist_hz * index as f32 / (point_count - 1) as f32
            };
            let bin = (frequency_hz / bin_hz.max(f32::EPSILON)).round() as usize;
            SpectralEnvelopePoint {
                frequency_hz,
                magnitude: envelope.get(bin.min(last_bin)).copied().unwrap_or(0.0),
            }
        })
        .collect();
    SpectralEnvelope {
        harmonic_spacing_hz: f0_hz,
        points,
    }
}

/// Cepstral lifter order `P ≈ sr / (2·f0)`: high enough to resolve formant structure but
/// kept below the pitch rahmonic at `sr / f0`, so the envelope cannot latch onto
/// individual harmonics. Falls back to `n / 12` when f0 is unknown.
fn true_envelope_lifter_order(half_len: usize, bin_hz: f32, f0_hz: Option<f32>) -> usize {
    let n = half_len.saturating_sub(1) * 2;
    let sample_rate = bin_hz * n as f32;
    let order = match f0_hz.filter(|f0| f0.is_finite() && *f0 > 0.0) {
        Some(f0) => sample_rate / (2.0 * f0.max(bin_hz)),
        None => n as f32 / 12.0,
    };
    (order.round() as usize).clamp(2, half_len.saturating_sub(1).max(2))
}

/// Iterative true envelope (Imai/Röbel & Rodet): start from the log spectrum and
/// repeatedly take the max of the *original* spectrum and its Hamming-liftered cepstral
/// smoothing. The `max` pushes the envelope up to ride the spectral peaks rather than the
/// mean, and the Hamming lifter suppresses the Gibbs ringing a rectangular lifter leaves.
/// Returns the converged smooth envelope as linear magnitudes.
fn true_envelope_magnitudes(
    magnitudes: &[f32],
    lifter_order: usize,
    fft: &dyn RealToComplex<f32>,
) -> Vec<f32> {
    let half = magnitudes.len();
    let n = half.saturating_sub(1) * 2;
    if n < 4 || half < 3 {
        return magnitudes.to_vec();
    }
    let target: Vec<f32> = magnitudes
        .iter()
        .map(|m| m.max(TRUE_ENVELOPE_LOG_FLOOR).ln())
        .collect();
    let mut estimate = target.clone();
    let mut smoothed = vec![0.0_f32; half];
    let mut full = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let threshold = TRUE_ENVELOPE_THRESHOLD_DB * std::f32::consts::LN_10 / 20.0;

    for _ in 0..TRUE_ENVELOPE_MAX_ITERS {
        cepstral_smooth(
            &estimate,
            lifter_order,
            fft,
            &mut full,
            &mut spectrum,
            &mut smoothed,
        );
        let max_excess = (0..half)
            .map(|k| target[k] - smoothed[k])
            .fold(0.0_f32, f32::max);
        if max_excess <= threshold {
            break;
        }
        for k in 0..half {
            estimate[k] = target[k].max(smoothed[k]);
        }
    }
    smoothed.iter().map(|value| value.exp()).collect()
}

/// One cepstral smoothing pass: even-symmetric DFT of the log spectrum → real cepstrum →
/// Hamming lifter keeping `|quefrency| ≤ order` → DFT back to the smoothed log spectrum.
/// `full` (length `n`) and `spectrum` (length `n/2+1`) are reused FFT scratch buffers.
fn cepstral_smooth(
    log_half: &[f32],
    lifter_order: usize,
    fft: &dyn RealToComplex<f32>,
    full: &mut [f32],
    spectrum: &mut [Complex32],
    out: &mut [f32],
) {
    let half = log_half.len();
    let n = full.len();
    for (slot, &value) in full.iter_mut().zip(log_half.iter()) {
        *slot = value;
    }
    for k in 1..half.saturating_sub(1) {
        full[n - k] = log_half[k];
    }
    let _ = fft.process(full, spectrum);
    // Real cepstrum c[k] = Re(DFT(log))/n, even-symmetric in k ↔ n-k; apply the Hamming
    // lifter and rebuild the symmetric sequence in `full` for the inverse pass.
    let order = lifter_order.max(1);
    for k in 0..n {
        let quefrency = k.min(n - k);
        let coeff = if k < half {
            spectrum[k].re
        } else {
            spectrum[n - k].re
        } / n as f32;
        let weight = if quefrency <= order {
            0.54 + 0.46 * (std::f32::consts::PI * quefrency as f32 / order as f32).cos()
        } else {
            0.0
        };
        full[k] = coeff * weight;
    }
    let _ = fft.process(full, spectrum);
    for (slot, bin) in out.iter_mut().zip(spectrum.iter()) {
        *slot = bin.re;
    }
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

fn harmonic_magnitudes(magnitudes: &[f32], bin_hz: f32, f0_hz: Option<f32>) -> Vec<f32> {
    let Some(f0_hz) = f0_hz.filter(|f0| *f0 > 0.0 && bin_hz > 0.0) else {
        return Vec::new();
    };
    let nyquist_hz = bin_hz * magnitudes.len().saturating_sub(1) as f32;
    let radius_bins = (f0_hz * 0.05 / bin_hz).ceil().max(1.0) as usize;
    let mut values = Vec::new();
    let mut harmonic = f0_hz;
    while harmonic <= nyquist_hz {
        let center = (harmonic / bin_hz).round() as usize;
        let start = center.saturating_sub(radius_bins);
        let end = (center + radius_bins + 1).min(magnitudes.len());
        values.push(rms_magnitude(&magnitudes[start..end]));
        harmonic += f0_hz;
    }
    values
}

fn spectral_peaks(
    spectrum: &[realfft::num_complex::Complex32],
    magnitudes: &[f32],
    bin_hz: f32,
    f0_hz: Option<f32>,
) -> Vec<SpectralPeak> {
    if magnitudes.len() < 3 || bin_hz <= 0.0 {
        return Vec::new();
    }
    let peak_floor = magnitudes.iter().copied().fold(0.0, f32::max) * 0.01;
    let mut peaks = (1..magnitudes.len() - 1)
        .filter(|index| {
            magnitudes[*index] > peak_floor
                && magnitudes[*index] >= magnitudes[index - 1]
                && magnitudes[*index] > magnitudes[index + 1]
        })
        .map(|index| SpectralPeak {
            frequency_hz: snapped_peak_frequency(
                interpolated_peak_bin(magnitudes, index) * bin_hz,
                f0_hz,
                bin_hz,
            ),
            magnitude: magnitudes[index],
            phase_radians: spectrum
                .get(index)
                .map(|bin| bin.im.atan2(bin.re))
                .unwrap_or(0.0),
        })
        .collect::<Vec<_>>();
    peaks.sort_by(|left, right| right.magnitude.total_cmp(&left.magnitude));
    peaks.truncate(96);
    peaks.sort_by(|left, right| left.frequency_hz.total_cmp(&right.frequency_hz));
    peaks
}

fn snapped_peak_frequency(frequency_hz: f32, f0_hz: Option<f32>, tolerance_hz: f32) -> f32 {
    let Some(f0_hz) = f0_hz.filter(|f0| *f0 > 0.0 && f0.is_finite()) else {
        return frequency_hz;
    };
    let harmonic = (frequency_hz / f0_hz).round().max(1.0);
    let snapped = harmonic * f0_hz;
    if (snapped - frequency_hz).abs() <= tolerance_hz.max(1.0) {
        snapped
    } else {
        frequency_hz
    }
}

fn interpolated_peak_bin(magnitudes: &[f32], index: usize) -> f32 {
    if index == 0 || index + 1 >= magnitudes.len() {
        return index as f32;
    }
    let left = magnitudes[index - 1];
    let center = magnitudes[index];
    let right = magnitudes[index + 1];
    let curvature = left - 2.0 * center + right;
    if curvature.abs() <= f32::EPSILON {
        return index as f32;
    }
    let offset = (0.5 * (left - right) / curvature).clamp(-0.5, 0.5);
    index as f32 + offset
}

fn frame_rms(audio: &[f32], start_sample: usize, end_sample: usize) -> f32 {
    if start_sample >= end_sample || start_sample >= audio.len() {
        return 0.0;
    }
    let end_sample = end_sample.min(audio.len());
    let energy = audio[start_sample..end_sample]
        .iter()
        .copied()
        .map(|sample| sample * sample)
        .sum::<f32>();
    (energy / (end_sample - start_sample) as f32).sqrt()
}

fn rms_magnitude(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|value| value * value).sum::<f32>() / values.len() as f32).sqrt()
}
