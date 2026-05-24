use crate::math::finite_or;

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
}
