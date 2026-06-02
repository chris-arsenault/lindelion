use std::ops::Range;

use super::{peak_abs, rms, spectral_centroid_hz, windowed_dft_magnitude_at};

/// Estimate the T60 (time to decay 60 dB) of a single partial at `frequency_hz`,
/// by tracking its windowed magnitude over time and least-squares-fitting the
/// magnitude-in-dB against time. Returns `None` when the partial is not decaying
/// (slope ≥ 0 — e.g. a steady tone or a held resonator that does not yet ring
/// down), the input is too short for a fit, or the inputs are not finite.
///
/// Mirrors [`harmonic_decay_profile`]'s sub-window magnitude tracking
/// (`windowed_dft_magnitude_at`), extended from a two-point ratio to a fitted
/// decay rate.
pub fn partial_t60_seconds(
    samples: &[f32],
    sample_rate: f32,
    frequency_hz: f32,
    window_len: usize,
    hop: usize,
) -> Option<f32> {
    if sample_rate <= 0.0 || frequency_hz <= 0.0 || window_len < 2 || hop == 0 {
        return None;
    }
    if samples.len() < window_len + hop {
        return None;
    }

    // (time_seconds, magnitude_db) per window.
    let mut times = Vec::new();
    let mut levels_db = Vec::new();
    let mut start = 0;
    while start + window_len <= samples.len() {
        let window = &samples[start..start + window_len];
        let magnitude = windowed_dft_magnitude_at(window, sample_rate, frequency_hz);
        if magnitude > 0.0 {
            let center = start as f32 + window_len as f32 * 0.5;
            times.push(center / sample_rate);
            levels_db.push(20.0 * magnitude.log10());
        }
        start += hop;
    }

    if times.len() < 4 {
        return None;
    }

    // Reject partials that do not measurably decay (steady tones, or held
    // resonators that do not yet ring down): a fit over near-flat levels yields a
    // meaningless near-infinite T60. Require a real top-to-bottom drop first.
    const MIN_DECAY_SPAN_DB: f32 = 3.0;
    if levels_db[0] - levels_db[levels_db.len() - 1] < MIN_DECAY_SPAN_DB {
        return None;
    }

    let slope = least_squares_slope(&times, &levels_db)?;
    if slope >= 0.0 || !slope.is_finite() {
        return None;
    }
    let t60 = -60.0 / slope;
    if t60.is_finite() && t60 > 0.0 {
        Some(t60)
    } else {
        None
    }
}

/// Slope of the least-squares line `y = slope·x + intercept`. Returns `None` when
/// the points are degenerate (fewer than two, or zero spread in `x`).
fn least_squares_slope(xs: &[f32], ys: &[f32]) -> Option<f32> {
    let n = xs.len();
    if n < 2 || ys.len() != n {
        return None;
    }
    let n_f = n as f32;
    let sum_x: f32 = xs.iter().copied().sum();
    let sum_y: f32 = ys.iter().copied().sum();
    let sum_xx: f32 = xs.iter().map(|x| x * x).sum();
    let sum_xy: f32 = xs.iter().zip(ys).map(|(x, y)| x * y).sum();
    let denominator = n_f * sum_xx - sum_x * sum_x;
    if denominator.abs() < f32::EPSILON {
        return None;
    }
    Some((n_f * sum_xy - sum_x * sum_y) / denominator)
}

/// For each harmonic `n` in `1..=harmonic_count`, find the actual spectral peak
/// near the ideal partial `n·fundamental_hz` and return the ratio
/// `measured_hz / (n·fundamental_hz)`. A ratio above 1 means the partial is
/// stretched sharp (string stiffness / inharmonicity); ≈ 1 means harmonic.
///
/// The peak is located by a coarse magnitude grid search across
/// `[center·(1 − search_fraction), center·(1 + search_fraction)]`, reusing
/// `windowed_dft_magnitude_at` as the per-frequency probe. Harmonics at or above
/// `0.45·sample_rate` are dropped.
pub fn inharmonicity_ratios(
    samples: &[f32],
    sample_rate: f32,
    fundamental_hz: f32,
    harmonic_count: usize,
    search_fraction: f32,
) -> Vec<f32> {
    if sample_rate <= 0.0
        || fundamental_hz <= 0.0
        || harmonic_count == 0
        || !(0.0..1.0).contains(&search_fraction)
        || samples.len() < 2
    {
        return Vec::new();
    }

    let mut ratios = Vec::with_capacity(harmonic_count);
    for harmonic in 1..=harmonic_count {
        let center = fundamental_hz * harmonic as f32;
        if center >= sample_rate * 0.45 {
            break;
        }
        let span = center * search_fraction;
        if let Some(peak_hz) = spectral_peak_hz_near(samples, sample_rate, center, span) {
            ratios.push(peak_hz / center);
        }
    }
    ratios
}

/// Coarse spectral-peak frequency within `[center − span, center + span]` by a
/// fixed-resolution magnitude grid search.
fn spectral_peak_hz_near(
    samples: &[f32],
    sample_rate: f32,
    center_hz: f32,
    span_hz: f32,
) -> Option<f32> {
    const PROBES: usize = 161;
    let low = (center_hz - span_hz).max(1.0);
    let high = (center_hz + span_hz).min(sample_rate * 0.49);
    if high <= low {
        return None;
    }
    let mut best_hz = low;
    let mut best_magnitude = -1.0;
    for probe in 0..PROBES {
        let frequency = low + (high - low) * probe as f32 / (PROBES - 1) as f32;
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, frequency);
        if magnitude > best_magnitude {
            best_magnitude = magnitude;
            best_hz = frequency;
        }
    }
    if best_magnitude > 0.0 {
        Some(best_hz)
    } else {
        None
    }
}

/// Spectral centroid (`spectral_centroid_hz`) measured in successive windows of
/// `window_len` samples advanced by `hop`, so the spectral darkening of a decaying
/// tone is visible as a falling trajectory. Silent/unmeasurable windows are
/// skipped. Empty when the input is shorter than one window.
pub fn spectral_centroid_trajectory(
    samples: &[f32],
    sample_rate: f32,
    window_len: usize,
    hop: usize,
) -> Vec<f32> {
    if sample_rate <= 0.0 || window_len < 2 || hop == 0 || samples.len() < window_len {
        return Vec::new();
    }
    let mut trajectory = Vec::new();
    let mut start = 0;
    while start + window_len <= samples.len() {
        if let Some(centroid) =
            spectral_centroid_hz(&samples[start..start + window_len], sample_rate)
        {
            trajectory.push(centroid);
        }
        start += hop;
    }
    trajectory
}

/// Ratio of onset energy to steady-state energy: `rms(attack) / rms(sustain)`,
/// where the attack window is `[0, attack_window_s)` and the sustain window is
/// `[sustain_start_s, sustain_start_s + sustain_window_s)`. A percussive tone
/// gives a ratio well above 1; a steady sustained tone gives ≈ 1. Returns `None`
/// when either window falls outside the buffer.
pub fn attack_sustain_ratio(
    samples: &[f32],
    sample_rate: f32,
    attack_window_s: f32,
    sustain_start_s: f32,
    sustain_window_s: f32,
) -> Option<f32> {
    if sample_rate <= 0.0
        || !attack_window_s.is_finite()
        || attack_window_s <= 0.0
        || !sustain_window_s.is_finite()
        || sustain_window_s <= 0.0
        || sustain_start_s < 0.0
    {
        return None;
    }
    let attack_len = (attack_window_s * sample_rate) as usize;
    let sustain_start = (sustain_start_s * sample_rate) as usize;
    let sustain_len = (sustain_window_s * sample_rate) as usize;
    if attack_len < 2 || sustain_len < 2 {
        return None;
    }
    let sustain_end = sustain_start + sustain_len;
    if attack_len > samples.len() || sustain_end > samples.len() {
        return None;
    }
    let attack_rms = rms(&samples[..attack_len]);
    let sustain_rms = rms(&samples[sustain_start..sustain_end]);
    Some(attack_rms / sustain_rms.max(1.0e-12))
}

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

    #[test]
    fn partial_t60_seconds_recovers_known_decay_rate() {
        let sample_rate = 48_000.0;
        let frequency = 220.0;
        let t60_target = 1.5;
        // amplitude(t) = exp(-t · ln(1000) / T60) decays exactly 60 dB over T60.
        let decay = (1_000.0_f32).ln() / t60_target;
        let len = (sample_rate * 2.0) as usize;
        let decaying: Vec<f32> = (0..len)
            .map(|index| {
                let t = index as f32 / sample_rate;
                (-decay * t).exp() * (std::f32::consts::TAU * frequency * t).sin()
            })
            .collect();

        let measured =
            partial_t60_seconds(&decaying, sample_rate, frequency, 4_096, 2_048).unwrap();
        assert!(
            (measured - t60_target).abs() / t60_target < 0.15,
            "measured T60 {measured} should be within 15% of {t60_target}"
        );

        // A steady (non-decaying) tone is not a decay and must report None.
        let steady = sine_wave(frequency, sample_rate, len);
        assert!(partial_t60_seconds(&steady, sample_rate, frequency, 4_096, 2_048).is_none());
    }

    #[cfg_attr(
        not(feature = "integration-tests"),
        ignore = "see make test-integration"
    )]
    #[test]
    fn inharmonicity_ratios_detects_partial_stretch() {
        let sample_rate = 48_000.0;
        let fundamental = 220.0;
        let stiffness = 3.0e-4; // piano-like B coefficient
        let partials = 5;
        let len = (sample_rate * 1.0) as usize;
        let stiff: Vec<f32> = (0..len)
            .map(|index| {
                let t = index as f32 / sample_rate;
                (1..=partials)
                    .map(|n| {
                        let stretch = (1.0 + stiffness * (n * n) as f32).sqrt();
                        let frequency = fundamental * n as f32 * stretch;
                        (std::f32::consts::TAU * frequency * t).sin()
                    })
                    .sum()
            })
            .collect();

        let ratios = inharmonicity_ratios(&stiff, sample_rate, fundamental, partials, 0.05);
        assert_eq!(ratios.len(), partials);
        assert!(ratios.iter().all(|r| r.is_finite()));
        assert!(
            (ratios[0] - 1.0).abs() < 0.01,
            "fundamental ratio {}",
            ratios[0]
        );
        // f_5/(5·f0) = sqrt(1 + B·25) = 1.00374 for B = 3e-4; assert measurably
        // above harmonic (≈ 1.0) but below the true stretch.
        assert!(
            ratios[partials - 1] > 1.003,
            "top partial should be measurably stretched: {}",
            ratios[partials - 1]
        );
        for pair in ratios.windows(2) {
            assert!(
                pair[1] >= pair[0] - 0.01,
                "ratios should not fall: {ratios:?}"
            );
        }

        // A perfectly harmonic signal stays at ≈ 1.0 for every partial.
        let harmonic: Vec<f32> = (0..len)
            .map(|index| {
                let t = index as f32 / sample_rate;
                (1..=partials)
                    .map(|n| (std::f32::consts::TAU * fundamental * n as f32 * t).sin())
                    .sum()
            })
            .collect();
        let harmonic_ratios =
            inharmonicity_ratios(&harmonic, sample_rate, fundamental, partials, 0.05);
        assert!(
            harmonic_ratios.iter().all(|r| (r - 1.0).abs() < 0.01),
            "harmonic signal ratios should be ≈ 1: {harmonic_ratios:?}"
        );
    }

    #[cfg_attr(
        not(feature = "integration-tests"),
        ignore = "see make test-integration"
    )]
    #[test]
    fn spectral_centroid_trajectory_falls_as_tone_darkens() {
        let sample_rate = 48_000.0;
        let len = (sample_rate * 1.0) as usize;
        // `spectral_centroid_hz` uses an unwindowed (rectangular) DFT, so choose
        // bin-aligned frequencies whose periods divide both the window (4096) and
        // the hop (2048): 187.5 Hz (256-sample period) and 750 Hz (64-sample
        // period). This eliminates spectral leakage so the centroid cleanly
        // tracks the fading high partial. The 750 Hz partial fades to silent, so
        // the magnitude-weighted centroid falls from ~468 Hz toward 187.5 Hz.
        let darkening: Vec<f32> = (0..len)
            .map(|index| {
                let t = index as f32 / sample_rate;
                let high_gain = 1.0 - index as f32 / len as f32;
                (std::f32::consts::TAU * 187.5 * t).sin()
                    + (std::f32::consts::TAU * 750.0 * t).sin() * high_gain
            })
            .collect();

        let trajectory = spectral_centroid_trajectory(&darkening, sample_rate, 4_096, 2_048);
        assert!(trajectory.len() >= 4);
        assert!(trajectory.iter().all(|c| c.is_finite()));
        assert!(
            trajectory.first().unwrap() > trajectory.last().unwrap(),
            "centroid should fall as the tone darkens: {trajectory:?}"
        );
    }

    #[test]
    fn attack_sustain_ratio_separates_percussive_from_sustained() {
        let sample_rate = 48_000.0;
        let len = (sample_rate * 1.5) as usize;
        let frequency = 220.0;

        // Percussive: loud onset decaying toward the sustain window.
        let decay = (1_000.0_f32).ln() / 0.5;
        let percussive: Vec<f32> = (0..len)
            .map(|index| {
                let t = index as f32 / sample_rate;
                (-decay * t).exp() * (std::f32::consts::TAU * frequency * t).sin()
            })
            .collect();
        let percussive_ratio =
            attack_sustain_ratio(&percussive, sample_rate, 0.03, 1.0, 0.2).unwrap();
        assert!(
            percussive_ratio > 1.5,
            "percussive attack/sustain ratio should be large: {percussive_ratio}"
        );

        // Sustained: flat amplitude → ratio ≈ 1.
        let sustained = sine_wave(frequency, sample_rate, len);
        let sustained_ratio =
            attack_sustain_ratio(&sustained, sample_rate, 0.03, 1.0, 0.2).unwrap();
        assert!(
            (sustained_ratio - 1.0).abs() < 0.2,
            "sustained attack/sustain ratio should be ≈ 1: {sustained_ratio}"
        );
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }
}
