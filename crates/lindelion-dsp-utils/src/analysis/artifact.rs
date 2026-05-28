#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterPeakFloorBand {
    pub min_hz: f32,
    pub max_hz: f32,
    pub step_hz: f32,
    pub exclusion_radius_hz: f32,
}

impl InterPeakFloorBand {
    pub const fn new(min_hz: f32, max_hz: f32, step_hz: f32, exclusion_radius_hz: f32) -> Self {
        Self {
            min_hz,
            max_hz,
            step_hz,
            exclusion_radius_hz,
        }
    }
}

pub fn windowed_dft_energy_at(samples: &[f32], sample_rate: f32, frequency_hz: f32) -> f32 {
    if samples.len() < 2 || sample_rate <= 0.0 || frequency_hz <= 0.0 {
        return 0.0;
    }

    let phase_step = -std::f32::consts::TAU * frequency_hz / sample_rate;
    let len = samples.len() as f32;
    let mut real = 0.0;
    let mut imag = 0.0;
    for (index, sample) in samples.iter().copied().enumerate() {
        let window = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (len - 1.0)).cos();
        let phase = phase_step * index as f32;
        real += sample * window * phase.cos();
        imag += sample * window * phase.sin();
    }
    real * real + imag * imag
}

pub fn folded_frequency_hz(frequency_hz: f32, sample_rate: f32) -> f32 {
    if !frequency_hz.is_finite() || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return 0.0;
    }
    let mut folded = frequency_hz.rem_euclid(sample_rate);
    if folded > sample_rate * 0.5 {
        folded = sample_rate - folded;
    }
    folded
}

pub fn zero_crossing_period_jitter(samples: &[f32]) -> f32 {
    let crossings = upward_zero_crossings(samples);
    let periods = crossings
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect::<Vec<_>>();
    if periods.len() < 2 {
        return f32::INFINITY;
    }
    let mean = periods.iter().sum::<f32>() / periods.len() as f32;
    let variance = periods
        .iter()
        .map(|period| {
            let error = period - mean;
            error * error
        })
        .sum::<f32>()
        / periods.len() as f32;
    variance.sqrt() / mean.max(f32::EPSILON)
}

pub fn fixed_analysis_region(samples: &[f32], trim: usize, len: usize) -> &[f32] {
    if samples.is_empty() || len == 0 {
        return &samples[..0];
    }

    let trim = trim.min(samples.len() / 4);
    let available_end = samples.len().saturating_sub(trim);
    let end = (trim + len).min(available_end);
    &samples[trim..end]
}

pub fn reference_peak_frequencies(
    samples: &[f32],
    sample_rate: f32,
    frequency_min_hz: f32,
    frequency_max_hz: f32,
    step_hz: f32,
    required_peaks_hz: &[f32],
) -> Vec<f32> {
    if cannot_scan_peaks(
        samples,
        sample_rate,
        frequency_min_hz,
        frequency_max_hz,
        step_hz,
    ) {
        return usable_required_peaks(required_peaks_hz, sample_rate).collect();
    }

    let mut bins = Vec::new();
    let mut frequency_hz = frequency_min_hz;
    let max_frequency_hz = frequency_max_hz.min(sample_rate * 0.45);
    while frequency_hz <= max_frequency_hz {
        bins.push((
            frequency_hz,
            windowed_dft_energy_at(samples, sample_rate, frequency_hz),
        ));
        frequency_hz += step_hz;
    }

    let peak_floor = bins.iter().map(|(_, energy)| *energy).fold(0.0, f32::max) * 0.002;
    let mut peaks = bins
        .windows(3)
        .filter_map(|window| {
            let [left, center, right] = window else {
                return None;
            };
            (center.1 > peak_floor && center.1 >= left.1 && center.1 > right.1).then_some(*center)
        })
        .collect::<Vec<_>>();
    peaks.sort_by(|left, right| right.1.total_cmp(&left.1));
    peaks.truncate(48);
    peaks.extend(
        usable_required_peaks(required_peaks_hz, sample_rate).map(|frequency_hz| {
            (
                frequency_hz,
                windowed_dft_energy_at(samples, sample_rate, frequency_hz),
            )
        }),
    );
    peaks.sort_by(|left, right| left.0.total_cmp(&right.0));
    peaks.dedup_by(|left, right| (left.0 - right.0).abs() < step_hz * 0.5);
    peaks
        .into_iter()
        .map(|(frequency_hz, _)| frequency_hz)
        .collect()
}

pub fn shifted_frequencies(frequencies_hz: &[f32], ratio: f32) -> Vec<f32> {
    frequencies_hz
        .iter()
        .copied()
        .filter(|frequency_hz| frequency_hz.is_finite())
        .map(|frequency_hz| frequency_hz * ratio)
        .filter(|frequency_hz| frequency_hz.is_finite() && *frequency_hz > 0.0)
        .collect()
}

pub fn inter_peak_floor_ratio(samples: &[f32], sample_rate: f32, peaks_hz: &[f32]) -> f32 {
    inter_peak_floor_ratio_in_band(
        samples,
        sample_rate,
        peaks_hz,
        InterPeakFloorBand::new(100.0, (sample_rate * 0.45).min(6_000.0), 25.0, 20.0),
    )
}

pub fn inter_peak_floor_ratio_in_band(
    samples: &[f32],
    sample_rate: f32,
    peaks_hz: &[f32],
    band: InterPeakFloorBand,
) -> f32 {
    if samples.len() < 2 || sample_rate <= 0.0 || peaks_hz.is_empty() {
        return 0.0;
    }
    if band.min_hz <= 0.0 || band.max_hz <= band.min_hz || band.step_hz <= 0.0 {
        return 0.0;
    }

    let usable_peaks = peaks_hz
        .iter()
        .copied()
        .filter(|frequency_hz| *frequency_hz > 50.0 && *frequency_hz < sample_rate * 0.45)
        .collect::<Vec<_>>();
    if usable_peaks.is_empty() {
        return 0.0;
    }

    let peak_energy = peak_neighborhood_energy(samples, sample_rate, &usable_peaks);
    let median_floor = median_inter_peak_energy(samples, sample_rate, &usable_peaks, band);
    let mean_peak = peak_energy / (usable_peaks.len() * 5) as f32;
    median_floor / mean_peak.max(1.0e-12)
}

fn cannot_scan_peaks(
    samples: &[f32],
    sample_rate: f32,
    frequency_min_hz: f32,
    frequency_max_hz: f32,
    step_hz: f32,
) -> bool {
    samples.len() < 2
        || sample_rate <= 0.0
        || frequency_min_hz <= 0.0
        || frequency_max_hz <= frequency_min_hz
        || step_hz <= 0.0
}

fn usable_required_peaks(peaks_hz: &[f32], sample_rate: f32) -> impl Iterator<Item = f32> + '_ {
    peaks_hz.iter().copied().filter(move |frequency_hz| {
        frequency_hz.is_finite() && *frequency_hz > 0.0 && *frequency_hz < sample_rate * 0.45
    })
}

fn peak_neighborhood_energy(samples: &[f32], sample_rate: f32, usable_peaks: &[f32]) -> f32 {
    let mut peak_energy = 0.0;
    for center_hz in usable_peaks.iter().copied() {
        for offset_hz in [-8.0, -4.0, 0.0, 4.0, 8.0] {
            peak_energy += windowed_dft_energy_at(samples, sample_rate, center_hz + offset_hz);
        }
    }
    peak_energy
}

fn median_inter_peak_energy(
    samples: &[f32],
    sample_rate: f32,
    usable_peaks: &[f32],
    band: InterPeakFloorBand,
) -> f32 {
    let mut floor_values = Vec::new();
    let mut frequency_hz = band.min_hz;
    let floor_max_hz = band.max_hz.min(sample_rate * 0.45);
    while frequency_hz < floor_max_hz {
        if usable_peaks
            .iter()
            .all(|peak_hz| (frequency_hz - peak_hz).abs() >= band.exclusion_radius_hz)
        {
            floor_values.push(windowed_dft_energy_at(samples, sample_rate, frequency_hz));
        }
        frequency_hz += band.step_hz;
    }

    floor_values.sort_by(f32::total_cmp);
    floor_values
        .get(floor_values.len() / 2)
        .copied()
        .unwrap_or(0.0)
}

fn upward_zero_crossings(samples: &[f32]) -> Vec<f32> {
    samples
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            let previous = pair[0];
            let current = pair[1];
            if previous <= 0.0 && current > 0.0 {
                let fraction = -previous / (current - previous).max(f32::EPSILON);
                Some(index as f32 + fraction)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_peak_floor_ratio_tracks_broadband_residue() {
        let sample_rate = 48_000.0;
        let clean = sine_wave(440.0, sample_rate, 8_192);
        let noisy = clean
            .iter()
            .copied()
            .enumerate()
            .map(|(index, sample)| {
                let hash = ((index * 1_103_515_245 + 12_345) & 0xffff) as f32 / 65_535.0;
                sample + (hash * 2.0 - 1.0) * 0.025
            })
            .collect::<Vec<_>>();
        let peaks = reference_peak_frequencies(&clean, sample_rate, 100.0, 2_000.0, 25.0, &[]);
        let shifted = shifted_frequencies(&peaks, 2.0_f32.powf(1.0 / 12.0));

        assert!(
            peaks
                .iter()
                .any(|frequency_hz| { (*frequency_hz - 440.0).abs() <= 12.5 })
        );
        assert_eq!(fixed_analysis_region(&clean, 1_024, 2_048).len(), 2_048);
        assert!(
            inter_peak_floor_ratio(&noisy, sample_rate, &peaks)
                > inter_peak_floor_ratio(&clean, sample_rate, &peaks) * 10.0
        );
        assert!(
            shifted
                .first()
                .is_some_and(|frequency_hz| *frequency_hz > peaks[0])
        );
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }
}
