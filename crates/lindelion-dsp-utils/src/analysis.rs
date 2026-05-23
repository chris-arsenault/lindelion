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
}
