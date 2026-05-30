use crate::{math::finite_or, window};

mod artifact;
mod measurements;
mod pitch;

pub use artifact::{
    InterPeakFloorBand, fixed_analysis_region, folded_frequency_hz, inter_peak_floor_ratio,
    inter_peak_floor_ratio_in_band, reference_peak_frequencies, shifted_frequencies,
    windowed_dft_energy_at, zero_crossing_period_jitter,
};
pub use measurements::{
    AudioWindowMetrics, HarmonicDecayMeasurement, audio_window_metrics, first_index_above_abs,
    harmonic_decay_profile,
};
pub use pitch::{estimate_f0_autocorrelation_refined, estimate_f0_dft_peak};

pub fn peak_abs(buffer: &[f32]) -> f32 {
    buffer
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0, |peak, sample| peak.max(sample))
}

pub fn rms(buffer: &[f32]) -> f32 {
    if buffer.is_empty() {
        return 0.0;
    }

    (buffer.iter().map(|sample| sample * sample).sum::<f32>() / buffer.len() as f32).sqrt()
}

pub fn assert_all_finite(buffer: &[f32]) {
    assert!(
        buffer.iter().all(|sample| sample.is_finite()),
        "buffer contains NaN or infinity"
    );
}

pub fn sanitize_audio_in_place(samples: &mut [f32]) {
    for sample in samples {
        *sample = finite_or(*sample, 0.0);
    }
}

pub fn sanitize_audio_to_vec(audio: &[f32]) -> Vec<f32> {
    let mut audio = audio.to_vec();
    sanitize_audio_in_place(&mut audio);
    audio
}

pub fn append_sanitized_audio(buffer: &mut Vec<f32>, audio: &[f32]) {
    let start = buffer.len();
    buffer.extend_from_slice(audio);
    sanitize_audio_in_place(&mut buffer[start..]);
}

pub fn median_finite_positive(values: impl IntoIterator<Item = f32>) -> Option<f32> {
    let mut values = values
        .into_iter()
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values[values.len() / 2])
}

pub fn dft_magnitude_at(buffer: &[f32], sample_rate: f32, frequency_hz: f32) -> f32 {
    if buffer.is_empty() || sample_rate <= 0.0 || frequency_hz <= 0.0 {
        return 0.0;
    }

    let phase_step = -std::f32::consts::TAU * frequency_hz / sample_rate;
    let mut real = 0.0;
    let mut imag = 0.0;

    for (index, sample) in buffer.iter().copied().enumerate() {
        let phase = phase_step * index as f32;
        real += sample * phase.cos();
        imag += sample * phase.sin();
    }

    (real * real + imag * imag).sqrt() / buffer.len() as f32
}

pub fn windowed_dft_magnitude_at(buffer: &[f32], sample_rate: f32, frequency_hz: f32) -> f32 {
    if buffer.len() < 2 || sample_rate <= 0.0 || frequency_hz <= 0.0 {
        return 0.0;
    }

    let phase_step = -std::f64::consts::TAU * frequency_hz as f64 / sample_rate as f64;
    let mut real = 0.0;
    let mut imag = 0.0;
    let mut window_sum = 0.0;

    for (index, sample) in buffer.iter().copied().enumerate() {
        let window = window::hann_f64(index, buffer.len());
        let phase = phase_step * index as f64;
        real += sample as f64 * window * phase.cos();
        imag += sample as f64 * window * phase.sin();
        window_sum += window;
    }

    if window_sum > f64::EPSILON {
        ((real * real + imag * imag).sqrt() / window_sum) as f32
    } else {
        0.0
    }
}

pub fn spectral_centroid_hz(buffer: &[f32], sample_rate: f32) -> Option<f32> {
    if buffer.len() < 2 || sample_rate <= 0.0 || !sample_rate.is_finite() {
        return None;
    }

    let len = buffer.len();
    let nyquist_bin = len / 2;
    let mut weighted_sum = 0.0;
    let mut magnitude_sum = 0.0;

    for bin in 1..=nyquist_bin {
        let phase_step = -std::f32::consts::TAU * bin as f32 / len as f32;
        let mut real = 0.0;
        let mut imag = 0.0;
        for (index, sample) in buffer.iter().copied().enumerate() {
            let phase = phase_step * index as f32;
            real += sample * phase.cos();
            imag += sample * phase.sin();
        }

        let magnitude = (real * real + imag * imag).sqrt();
        if magnitude <= f32::EPSILON || !magnitude.is_finite() {
            continue;
        }

        let frequency_hz = bin as f32 * sample_rate / len as f32;
        weighted_sum += frequency_hz * magnitude;
        magnitude_sum += magnitude;
    }

    (magnitude_sum > f32::EPSILON).then_some(weighted_sum / magnitude_sum)
}

pub fn estimate_frequency_zero_crossings(buffer: &[f32], sample_rate: f32) -> Option<f32> {
    if buffer.len() < 3 || sample_rate <= 0.0 {
        return None;
    }

    let mut crossings = Vec::new();
    for index in 1..buffer.len() {
        let previous = buffer[index - 1];
        let current = buffer[index];
        if previous <= 0.0 && current > 0.0 {
            let denominator = current - previous;
            let fraction = if denominator.abs() > f32::EPSILON {
                -previous / denominator
            } else {
                0.0
            };
            crossings.push(index as f32 - 1.0 + fraction);
        }
    }

    if crossings.len() < 2 {
        return None;
    }

    let periods = crossings
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|period| *period > 0.0)
        .collect::<Vec<_>>();

    if periods.is_empty() {
        return None;
    }

    let average_period = periods.iter().sum::<f32>() / periods.len() as f32;
    Some(sample_rate / average_period)
}

pub fn estimate_f0_autocorrelation(
    samples: &[f32],
    sample_rate: f32,
    min_hz: f32,
    max_hz: f32,
) -> Option<f32> {
    if samples.len() < 256 || sample_rate <= 0.0 || min_hz <= 0.0 || max_hz <= min_hz {
        return None;
    }
    let window = strongest_rms_window(samples, 8_192.min(samples.len()));
    if window.len() < 256 {
        return None;
    }
    let mean = window.iter().copied().sum::<f32>() / window.len() as f32;
    let centered = window
        .iter()
        .map(|sample| sample - mean)
        .collect::<Vec<_>>();
    let min_lag = (sample_rate / max_hz).floor().max(1.0) as usize;
    let max_lag = ((sample_rate / min_hz).ceil() as usize).min(centered.len() / 2);
    if min_lag >= max_lag {
        return None;
    }

    let mut best_lag = 0usize;
    let mut best_score = 0.0f32;
    for lag in min_lag..=max_lag {
        let score = normalized_autocorrelation(&centered, lag);
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    (best_score > 0.1 && best_lag > 0).then_some(sample_rate / best_lag as f32)
}

pub fn fitted_sine_rms_error(samples: &[f32], sample_rate: f32, frequency_hz: f32) -> f32 {
    if samples.is_empty() || sample_rate <= 0.0 || frequency_hz <= 0.0 {
        return 0.0;
    }

    let mut sin_sample_sum = 0.0;
    let mut cos_sample_sum = 0.0;
    let mut sin_sin_sum = 0.0;
    let mut cos_cos_sum = 0.0;
    let mut sin_cos_sum = 0.0;
    for (index, sample) in samples.iter().copied().enumerate() {
        let phase = std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate;
        let sin = phase.sin();
        let cos = phase.cos();
        sin_sample_sum += sin * sample;
        cos_sample_sum += cos * sample;
        sin_sin_sum += sin * sin;
        cos_cos_sum += cos * cos;
        sin_cos_sum += sin * cos;
    }

    let determinant = sin_sin_sum * cos_cos_sum - sin_cos_sum * sin_cos_sum;
    if determinant.abs() <= f32::EPSILON {
        return f32::INFINITY;
    }
    let sin_gain = (sin_sample_sum * cos_cos_sum - cos_sample_sum * sin_cos_sum) / determinant;
    let cos_gain = (cos_sample_sum * sin_sin_sum - sin_sample_sum * sin_cos_sum) / determinant;

    let error_sum = samples
        .iter()
        .copied()
        .enumerate()
        .map(|(index, sample)| {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate;
            let fitted = sin_gain * phase.sin() + cos_gain * phase.cos();
            let error = sample - fitted;
            error * error
        })
        .sum::<f32>();
    (error_sum / samples.len() as f32).sqrt()
}

pub fn rms_difference(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    if len == 0 {
        return 0.0;
    }
    let error_sum = left
        .iter()
        .zip(right.iter())
        .take(len)
        .map(|(left, right)| {
            let error = left - right;
            error * error
        })
        .sum::<f32>();
    (error_sum / len as f32).sqrt()
}

pub fn gain_fitted_rms_difference(reference: &[f32], rendered: &[f32]) -> f32 {
    let len = reference.len().min(rendered.len());
    if len == 0 {
        return 0.0;
    }
    let reference = &reference[..len];
    let rendered = &rendered[..len];
    let reference_energy = reference
        .iter()
        .copied()
        .map(|sample| sample * sample)
        .sum::<f32>();
    if reference_energy <= f32::EPSILON {
        return rms(rendered);
    }
    let gain = reference
        .iter()
        .zip(rendered.iter())
        .map(|(reference, rendered)| reference * rendered)
        .sum::<f32>()
        / reference_energy;
    let error_sum = reference
        .iter()
        .zip(rendered.iter())
        .map(|(reference, rendered)| {
            let error = rendered - reference * gain;
            error * error
        })
        .sum::<f32>();
    (error_sum / len as f32).sqrt()
}

pub fn max_adjacent_delta(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|window| (window[1] - window[0]).abs())
        .fold(0.0, f32::max)
}

pub fn high_frequency_artifact_ratio(samples: &[f32], sample_rate: f32, target_hz: f32) -> f32 {
    artifact_frequency_ratio(
        samples,
        sample_rate,
        target_hz,
        &[4_000.0, 6_000.0, 8_000.0, 12_000.0, 16_000.0],
    )
}

pub fn artifact_frequency_ratio(
    samples: &[f32],
    sample_rate: f32,
    target_hz: f32,
    artifact_frequencies_hz: &[f32],
) -> f32 {
    let fundamental = dft_magnitude_at(samples, sample_rate, target_hz).max(1.0e-9);
    let strongest_artifact = artifact_frequencies_hz
        .iter()
        .copied()
        .map(|frequency_hz| dft_magnitude_at(samples, sample_rate, frequency_hz))
        .fold(0.0, f32::max);

    strongest_artifact / fundamental
}

pub fn sampled_high_frequency_ratio(
    samples: &[f32],
    sample_rate: f32,
    high_start_hz: f32,
    step_hz: f32,
) -> f32 {
    if samples.is_empty() || sample_rate <= 0.0 || high_start_hz <= 0.0 || step_hz <= 0.0 {
        return 0.0;
    }
    let nyquist = sample_rate * 0.5;
    let mut total = 0.0;
    let mut high = 0.0;
    let mut frequency_hz = step_hz;
    while frequency_hz < nyquist {
        let magnitude = dft_magnitude_at(samples, sample_rate, frequency_hz);
        let energy = magnitude * magnitude;
        total += energy;
        if frequency_hz >= high_start_hz {
            high += energy;
        }
        frequency_hz += step_hz;
    }

    high / total.max(1.0e-12)
}

fn strongest_rms_window(samples: &[f32], window_len: usize) -> &[f32] {
    if samples.len() <= window_len {
        return samples;
    }
    let hop = (window_len / 4).max(1);
    let mut best_start = 0usize;
    let mut best_rms = 0.0f32;
    for start in (0..=samples.len() - window_len).step_by(hop) {
        let rms = rms(&samples[start..start + window_len]);
        if rms > best_rms {
            best_rms = rms;
            best_start = start;
        }
    }
    &samples[best_start..best_start + window_len]
}

fn normalized_autocorrelation(samples: &[f32], lag: usize) -> f32 {
    if lag == 0 || lag >= samples.len() {
        return 0.0;
    }
    let len = samples.len() - lag;
    let mut dot = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for index in 0..len {
        let left = samples[index];
        let right = samples[index + lag];
        dot += left * right;
        left_energy += left * left;
        right_energy += right * right;
    }
    let denominator = (left_energy * right_energy).sqrt();
    if denominator > f32::EPSILON {
        dot / denominator
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sine_frequency() {
        let sample_rate = 48_000.0;
        let frequency = 440.0;
        let buffer = (0..4096)
            .map(|index| (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin())
            .collect::<Vec<_>>();

        let estimate = estimate_frequency_zero_crossings(&buffer, sample_rate).unwrap();
        assert!((estimate - frequency).abs() < 1.0);
    }

    #[test]
    fn autocorrelation_estimates_f0_from_strongest_window() {
        let sample_rate = 48_000.0;
        let mut buffer = vec![0.0; 2_048];
        buffer.extend(sine_wave(220.0, sample_rate, 8_192));
        buffer.extend(vec![0.0; 2_048]);

        let estimate = estimate_f0_autocorrelation(&buffer, sample_rate, 80.0, 1_000.0).unwrap();

        assert!((estimate - 220.0).abs() < 2.0, "estimate={estimate}");
    }

    #[test]
    fn dft_bin_is_larger_at_matching_frequency() {
        let sample_rate = 48_000.0;
        let buffer = (0..4096)
            .map(|index| (std::f32::consts::TAU * 1_000.0 * index as f32 / sample_rate).sin())
            .collect::<Vec<_>>();

        assert!(
            dft_magnitude_at(&buffer, sample_rate, 1_000.0)
                > dft_magnitude_at(&buffer, sample_rate, 4_000.0) * 20.0
        );
    }

    #[test]
    fn windowed_dft_reduces_near_frequency_sidelobes() {
        let sample_rate = 48_000.0;
        let target_hz = 226.44649;
        let probe_hz = 220.0;
        let buffer = sine_wave(target_hz, sample_rate, 48_000 - 8_192);
        let raw_ratio = dft_magnitude_at(&buffer, sample_rate, probe_hz)
            / dft_magnitude_at(&buffer, sample_rate, target_hz).max(1.0e-9);
        let windowed_ratio = windowed_dft_magnitude_at(&buffer, sample_rate, probe_hz)
            / windowed_dft_magnitude_at(&buffer, sample_rate, target_hz).max(1.0e-9);

        assert!(
            raw_ratio > 0.03,
            "test fixture should expose raw DFT sidelobe; raw_ratio={raw_ratio}"
        );
        assert!(
            windowed_ratio < 0.01,
            "windowed DFT should suppress near-frequency sidelobe; windowed_ratio={windowed_ratio}"
        );
    }

    #[test]
    fn spectral_centroid_tracks_brightness_order() {
        let sample_rate = 48_000.0;
        let low = sine_wave(sample_rate * 10.0 / 1024.0, sample_rate, 1024);
        let high = sine_wave(sample_rate * 85.0 / 1024.0, sample_rate, 1024);

        assert!(
            spectral_centroid_hz(&high, sample_rate).unwrap()
                > spectral_centroid_hz(&low, sample_rate).unwrap() * 4.0
        );
    }

    #[test]
    fn sine_fit_and_difference_metrics_track_distortion() {
        let sample_rate = 48_000.0;
        let clean = sine_wave(440.0, sample_rate, 4_096);
        let distorted = clean
            .iter()
            .copied()
            .enumerate()
            .map(|(index, sample)| {
                sample + 0.05 * (std::f32::consts::TAU * 5_000.0 * index as f32 / sample_rate).sin()
            })
            .collect::<Vec<_>>();

        assert!(fitted_sine_rms_error(&clean, sample_rate, 440.0) < 0.000_01);
        assert!(fitted_sine_rms_error(&distorted, sample_rate, 440.0) > 0.02);
        assert!(rms_difference(&clean, &distorted) > 0.02);
        assert!(gain_fitted_rms_difference(&clean, &scaled(&clean, 0.5)) < 0.000_01);
        assert!(max_adjacent_delta(&[0.0, 0.25, -0.5, 0.0]) == 0.75);
    }

    #[test]
    fn artifact_ratios_track_high_frequency_energy() {
        let sample_rate = 48_000.0;
        let clean = sine_wave(440.0, sample_rate, 4_096);
        let bright = clean
            .iter()
            .copied()
            .enumerate()
            .map(|(index, sample)| {
                sample + 0.08 * (std::f32::consts::TAU * 8_000.0 * index as f32 / sample_rate).sin()
            })
            .collect::<Vec<_>>();

        assert!(
            high_frequency_artifact_ratio(&bright, sample_rate, 440.0)
                > high_frequency_artifact_ratio(&clean, sample_rate, 440.0) * 10.0
        );
        assert!(
            sampled_high_frequency_ratio(&bright, sample_rate, 6_000.0, 100.0)
                > sampled_high_frequency_ratio(&clean, sample_rate, 6_000.0, 100.0)
        );
    }

    #[test]
    fn audio_sanitizers_replace_non_finite_samples() {
        let mut samples = [1.0, f32::NAN, f32::INFINITY, -0.5];

        sanitize_audio_in_place(&mut samples);

        assert_eq!(samples, [1.0, 0.0, 0.0, -0.5]);
        assert_eq!(
            sanitize_audio_to_vec(&[f32::NEG_INFINITY, 0.25]),
            vec![0.0, 0.25]
        );

        let mut appended = vec![0.5];
        append_sanitized_audio(&mut appended, &[f32::INFINITY, -0.25]);
        assert_eq!(appended, vec![0.5, 0.0, -0.25]);
    }

    #[test]
    fn median_finite_positive_ignores_invalid_values() {
        assert_eq!(
            median_finite_positive([f32::NAN, -1.0, 440.0, 220.0, 880.0]),
            Some(440.0)
        );
        assert_eq!(median_finite_positive([0.0, f32::INFINITY]), None);
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }

    fn scaled(samples: &[f32], gain: f32) -> Vec<f32> {
        samples.iter().map(|sample| sample * gain).collect()
    }
}
