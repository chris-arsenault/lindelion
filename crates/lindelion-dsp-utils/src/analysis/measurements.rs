use std::ops::Range;

use super::{peak_abs, rms, spectral_centroid_hz, windowed_dft_magnitude_at};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioWindowMetrics {
    pub sample_count: usize,
    pub peak_abs: f32,
    pub rms: f32,
    pub dc_offset: f32,
    pub spectral_centroid_hz: Option<f32>,
}

impl AudioWindowMetrics {
    pub fn dc_offset_abs(self) -> f32 {
        self.dc_offset.abs()
    }
}

pub fn audio_window_metrics(buffer: &[f32], sample_rate: f32) -> AudioWindowMetrics {
    let dc_offset = if buffer.is_empty() {
        0.0
    } else {
        buffer.iter().copied().sum::<f32>() / buffer.len() as f32
    };

    AudioWindowMetrics {
        sample_count: buffer.len(),
        peak_abs: peak_abs(buffer),
        rms: rms(buffer),
        dc_offset,
        spectral_centroid_hz: spectral_centroid_hz(buffer, sample_rate),
    }
}

pub fn first_index_above_abs(samples: &[f32], threshold: f32) -> Option<usize> {
    if !threshold.is_finite() || threshold < 0.0 {
        return None;
    }
    samples.iter().position(|sample| sample.abs() > threshold)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HarmonicDecayMeasurement {
    pub harmonic: usize,
    pub frequency_hz: f32,
    pub early_magnitude: f32,
    pub late_magnitude: f32,
    pub late_to_early_ratio: f32,
}

pub fn harmonic_decay_profile(
    samples: &[f32],
    sample_rate: f32,
    fundamental_hz: f32,
    harmonic_count: usize,
    early_range: Range<usize>,
    late_range: Range<usize>,
) -> Vec<HarmonicDecayMeasurement> {
    if sample_rate <= 0.0 || fundamental_hz <= 0.0 || harmonic_count == 0 {
        return Vec::new();
    }

    let early = clamped_range(samples, early_range);
    let late = clamped_range(samples, late_range);
    if early.len() < 2 || late.len() < 2 {
        return Vec::new();
    }

    let mut profile = Vec::with_capacity(harmonic_count);
    for harmonic in 1..=harmonic_count {
        let frequency_hz = fundamental_hz * harmonic as f32;
        if frequency_hz >= sample_rate * 0.45 {
            break;
        }
        let early_magnitude = windowed_dft_magnitude_at(early, sample_rate, frequency_hz);
        let late_magnitude = windowed_dft_magnitude_at(late, sample_rate, frequency_hz);
        profile.push(HarmonicDecayMeasurement {
            harmonic,
            frequency_hz,
            early_magnitude,
            late_magnitude,
            late_to_early_ratio: late_magnitude / early_magnitude.max(1.0e-12),
        });
    }
    profile
}

fn clamped_range(samples: &[f32], range: Range<usize>) -> &[f32] {
    let start = range.start.min(samples.len());
    let end = range.end.min(samples.len());
    if start >= end {
        &samples[0..0]
    } else {
        &samples[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_window_metrics_reports_level_centroid_and_dc() {
        let sample_rate = 48_000.0;
        let audio = sine_wave(1_000.0, sample_rate, 2_048)
            .into_iter()
            .map(|sample| sample * 0.5 + 0.1)
            .collect::<Vec<_>>();

        let metrics = audio_window_metrics(&audio, sample_rate);

        assert_eq!(metrics.sample_count, audio.len());
        assert!(metrics.peak_abs > 0.5);
        assert!(metrics.rms > 0.3);
        assert!((metrics.dc_offset - 0.1).abs() < 0.01);
        assert!(metrics.spectral_centroid_hz.unwrap() > 500.0);
    }

    #[test]
    fn first_index_above_abs_finds_threshold_crossing() {
        assert_eq!(first_index_above_abs(&[0.0, -0.1, 0.25], 0.2), Some(2));
        assert_eq!(first_index_above_abs(&[0.0, 0.1], 0.2), None);
        assert_eq!(first_index_above_abs(&[1.0], f32::NAN), None);
    }

    #[test]
    fn harmonic_decay_profile_tracks_partial_level_change() {
        let sample_rate = 48_000.0;
        let mut audio = Vec::new();
        for index in 0..8_192 {
            let t = index as f32 / sample_rate;
            let envelope = if index < 4_096 { 1.0 } else { 0.25 };
            let fundamental = (std::f32::consts::TAU * 220.0 * t).sin();
            let second = (std::f32::consts::TAU * 440.0 * t).sin() * 0.5;
            audio.push((fundamental + second) * envelope);
        }

        let profile =
            harmonic_decay_profile(&audio, sample_rate, 220.0, 3, 512..3_584, 4_608..7_680);

        assert_eq!(profile.len(), 3);
        assert_eq!(profile[0].harmonic, 1);
        assert!(profile[0].late_to_early_ratio < 0.35);
        assert!(profile[1].late_to_early_ratio < 0.35);
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }
}
