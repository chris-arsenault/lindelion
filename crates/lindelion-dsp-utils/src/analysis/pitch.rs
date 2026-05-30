use super::{normalized_autocorrelation, strongest_rms_window, windowed_dft_magnitude_at};

/// Sub-cent, wide-range fundamental estimator built on a windowed DFT
/// magnitude-peak scan with parabolic interpolation.
///
/// Unlike [`super::estimate_f0_autocorrelation`], whose integer-lag
/// quantization and window drift cannot resolve a few cents or bracket a badly
/// mistuned model, this scans the magnitude spectrum across the whole
/// `[min_hz, max_hz]` range — wide enough to bracket at least an octave of
/// error — and refines the strongest peak by re-centering a parabola on three
/// magnitude probes. It reports whatever fundamental is actually present, never
/// locking to an assumed target.
pub fn estimate_f0_dft_peak(
    samples: &[f32],
    sample_rate: f32,
    min_hz: f32,
    max_hz: f32,
) -> Option<f32> {
    /// Parabolic re-centering passes; each halves the probe spacing.
    const REFINEMENT_PASSES: usize = 8;

    if samples.len() < 256 || sample_rate <= 0.0 || min_hz <= 0.0 || max_hz <= min_hz {
        return None;
    }
    let min_hz = min_hz.max(1.0);
    let max_hz = max_hz.min(sample_rate * 0.49);
    if max_hz <= min_hz {
        return None;
    }
    let window = strongest_rms_window(samples, 16_384.min(samples.len()));
    if window.len() < 256 {
        return None;
    }

    // Coarse scan: probe spacing is half the Hann main-lobe width (two DFT
    // bins), so the magnitude peak's lobe is always sampled near its top and
    // never stepped over, while a sidelobe can never outscore it.
    let coarse_step = 2.0 * sample_rate / window.len() as f32;
    let mut best_freq = min_hz;
    let mut best_magnitude = 0.0_f32;
    let mut frequency = min_hz;
    while frequency <= max_hz {
        let magnitude = windowed_dft_magnitude_at(window, sample_rate, frequency);
        if magnitude > best_magnitude {
            best_magnitude = magnitude;
            best_freq = frequency;
        }
        frequency += coarse_step;
    }
    if best_magnitude <= 0.0 {
        return None;
    }

    // Refine: fit a parabola to three magnitude probes, move to its vertex, and
    // halve the spacing each pass. For a pure tone the windowed lobe is
    // symmetric about the true frequency, so this converges to it.
    let mut center = best_freq;
    let mut spacing = coarse_step;
    for _ in 0..REFINEMENT_PASSES {
        let lower = windowed_dft_magnitude_at(window, sample_rate, center - spacing);
        let middle = windowed_dft_magnitude_at(window, sample_rate, center);
        let upper = windowed_dft_magnitude_at(window, sample_rate, center + spacing);
        let curvature = lower - 2.0 * middle + upper;
        if curvature.abs() <= f32::EPSILON {
            break;
        }
        let offset = (0.5 * (lower - upper) / curvature).clamp(-1.0, 1.0);
        center += offset * spacing;
        spacing *= 0.5;
    }

    center.is_finite().then(|| center.clamp(min_hz, max_hz))
}

/// Sub-cent fundamental estimator measuring waveform *periodicity* via
/// parabolic-interpolated normalized autocorrelation.
///
/// [`super::estimate_f0_autocorrelation`] quantizes to integer lags; this
/// refines the autocorrelation peak to a sub-sample lag, removing that
/// quantization. Because it tracks the period rather than the strongest
/// spectral lobe, it stays accurate on spectrally complex, harmonically rich,
/// or filter-coloured tones (e.g. a bore's strike response) where a
/// magnitude-peak scan is pulled by the spectral envelope.
///
/// Pass a sub-octave `[min_hz, max_hz]` bracket (e.g. ±a fourth around the
/// expected pitch): a span of a full octave or more lets a period multiple or
/// sub-multiple lag tie with and outscore the true period.
pub fn estimate_f0_autocorrelation_refined(
    samples: &[f32],
    sample_rate: f32,
    min_hz: f32,
    max_hz: f32,
) -> Option<f32> {
    if samples.len() < 256 || sample_rate <= 0.0 || min_hz <= 0.0 || max_hz <= min_hz {
        return None;
    }
    let window = strongest_rms_window(samples, 16_384.min(samples.len()));
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
    if min_lag + 1 >= max_lag {
        return None;
    }

    let mut best_lag = 0usize;
    let mut best_score = 0.0_f32;
    for lag in min_lag..=max_lag {
        let score = normalized_autocorrelation(&centered, lag);
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }
    if best_score <= 0.1 || best_lag <= min_lag || best_lag >= max_lag {
        return (best_score > 0.1 && best_lag > 0).then(|| sample_rate / best_lag as f32);
    }

    // Parabolic sub-sample refinement of the autocorrelation peak: the period
    // lobe is symmetric about the true lag, so the vertex recovers it.
    let lower = normalized_autocorrelation(&centered, best_lag - 1);
    let upper = normalized_autocorrelation(&centered, best_lag + 1);
    let curvature = lower - 2.0 * best_score + upper;
    let refined_lag = if curvature.abs() > f32::EPSILON {
        best_lag as f32 + (0.5 * (lower - upper) / curvature).clamp(-0.5, 0.5)
    } else {
        best_lag as f32
    };

    (refined_lag > 0.0).then(|| sample_rate / refined_lag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dft_peak_estimator_resolves_sub_cent_across_matrix() {
        let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
        let frequencies = [30.0, 110.0, 440.0, 1_500.0, 4_000.0];

        for sample_rate in sample_rates {
            for frequency in frequencies {
                let buffer = sine_wave(frequency, sample_rate, (sample_rate * 0.6) as usize);
                let estimate =
                    estimate_f0_dft_peak(&buffer, sample_rate, frequency * 0.5, frequency * 2.0)
                        .unwrap_or_else(|| {
                            panic!("no estimate at {frequency} Hz / {sample_rate} Hz")
                        });
                let cents = crate::math::cents_between(frequency, estimate);
                assert!(
                    cents < 1.0,
                    "frequency={frequency} sample_rate={sample_rate} estimate={estimate} cents={cents}"
                );
            }
        }
    }

    #[test]
    fn dft_peak_estimator_locates_detuned_tone_without_locking_to_target() {
        let sample_rate = 48_000.0;
        let target = 440.0;
        // A tone an octave below the nominal target, mirroring a model that
        // resonates an octave flat. A target-locking estimator would report the
        // target; the magnitude-peak scan must report the tone that is present.
        let detuned = target * 0.5;
        let buffer = sine_wave(detuned, sample_rate, 24_000);

        let estimate =
            estimate_f0_dft_peak(&buffer, sample_rate, target * 0.25, target * 1.5).unwrap();

        assert!(
            crate::math::cents_between(detuned, estimate) < 1.0,
            "should find the detuned tone; estimate={estimate}"
        );
        assert!(
            crate::math::cents_between(target, estimate) > 100.0,
            "should not lock to the target; estimate={estimate}"
        );
    }

    #[test]
    fn refined_autocorrelation_resolves_sub_cent_across_matrix() {
        let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
        let frequencies = [30.0, 110.0, 440.0, 1_500.0, 4_000.0];

        for sample_rate in sample_rates {
            for frequency in frequencies {
                let buffer = sine_wave(frequency, sample_rate, (sample_rate * 0.6) as usize);
                // Sub-octave bracket so a period multiple/sub-multiple lag cannot
                // tie with and outscore the true period.
                let estimate = estimate_f0_autocorrelation_refined(
                    &buffer,
                    sample_rate,
                    frequency * 0.75,
                    frequency * 1.5,
                )
                .unwrap_or_else(|| panic!("no estimate at {frequency} Hz / {sample_rate} Hz"));
                let cents = crate::math::cents_between(frequency, estimate);
                assert!(
                    cents < 1.0,
                    "frequency={frequency} sample_rate={sample_rate} estimate={estimate} cents={cents}"
                );
            }
        }
    }

    fn sine_wave(frequency_hz: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin())
            .collect()
    }
}
