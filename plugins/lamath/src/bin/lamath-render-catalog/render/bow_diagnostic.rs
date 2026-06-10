//! Bow diagnostic analysis (`--bow-diagnostic`).
#![allow(clippy::wildcard_imports)]

use super::tube_tap_analysis::{amplitude_db, format_db, power_ratio_db, ratio_db};
use super::*;

#[allow(dead_code)]
pub(crate) const BOW_DIAGNOSTIC_WINDOW_SECONDS: f32 = 0.68;

pub(crate) const BOW_SPECTRUM_BAND_COUNT: usize = 7;

pub(crate) const BOW_SPECTRUM_STEP_HZ: f32 = 80.0;

pub(crate) const BOW_SPECTRUM_BANDS: [(&str, f32, f32); BOW_SPECTRUM_BAND_COUNT] = [
    ("80-500", 80.0, 500.0),
    ("0.5-1k", 500.0, 1_000.0),
    ("1-2k", 1_000.0, 2_000.0),
    ("2-4k", 2_000.0, 4_000.0),
    ("4-8k", 4_000.0, 8_000.0),
    ("8-12k", 8_000.0, 12_000.0),
    ("12-16k", 12_000.0, 16_000.0),
];

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct BowDiagnosticReport {
    pub(crate) case_id: &'static str,
    pub(crate) frequency_hz: f32,
    pub(crate) smoothed_frequency_hz: f32,
    pub(crate) one_way_delay_samples: f32,
    pub(crate) start_seconds: f32,
    pub(crate) end_seconds: f32,
    pub(crate) output: BowDiagnosticRow,
    pub(crate) pickup_tap: BowDiagnosticRow,
    pub(crate) body_radiation: BowDiagnosticRow,
    pub(crate) weighted_pickup: BowDiagnosticRow,
    pub(crate) weighted_body: BowDiagnosticRow,
    pub(crate) pickup_weight: f32,
    pub(crate) body_weight: f32,
    pub(crate) mix_residual_rms: f32,
    pub(crate) model_output: BowDiagnosticRow,
    pub(crate) bow_force: BowDiagnosticRow,
    pub(crate) bow_wave_injection: BowDiagnosticRow,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct BowDiagnosticRow {
    pub(crate) name: &'static str,
    pub(crate) peak: f32,
    pub(crate) rms: f32,
    pub(crate) dominant_hz: f32,
    pub(crate) dominant_dbfs: f32,
    pub(crate) nearest_harmonic: usize,
    pub(crate) nearest_harmonic_cents: f32,
    pub(crate) h1_dbfs: f32,
    pub(crate) h1_relative_db: f32,
    pub(crate) h2_relative_db: f32,
    pub(crate) h3_relative_db: f32,
    pub(crate) h4_relative_db: f32,
    pub(crate) h8_relative_db: f32,
    pub(crate) h16_relative_db: f32,
    pub(crate) h24_relative_db: f32,
    pub(crate) high_peak_hz: f32,
    pub(crate) high_peak_dbfs: f32,
    pub(crate) high_peak_relative_db: f32,
    pub(crate) spectrum: BowSpectrumSummary,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BowSpectrumSummary {
    pub(crate) band_power_relative_db: [f32; BOW_SPECTRUM_BAND_COUNT],
    pub(crate) high_low_power_db: f32,
    pub(crate) centroid_hz: f32,
    pub(crate) rolloff95_hz: f32,
}

#[allow(dead_code)]
pub(crate) fn analyze_bow_diagnostic(
    case: &CatalogCase,
) -> Result<BowDiagnosticReport, RenderError> {
    let RenderTarget::Stringed(patch) = target_for_recipe(case.patch_recipe) else {
        return Err(RenderError::BowDiagnosticRequiresString { case_id: case.id });
    };
    let frequency_hz = single_pitch_frequency_hz(case)
        .map_err(|_| RenderError::BowDiagnosticRequiresSinglePitch { case_id: case.id })?;
    let target_frames = case.target_frames();
    let render = render_stringed_with_probe(patch, case.schedule.notes, target_frames);
    let (start, end) = bow_diagnostic_window(case, target_frames)?;
    let output = &render.left[start..end];
    let probes = &render.probes[start..end];
    let pickup_tap = probe_signal(probes, |probe| probe.pickup_tap);
    let body_radiation = probe_signal(probes, |probe| probe.body_radiated);
    let weighted_pickup = probe_signal(probes, |probe| probe.weighted_pickup);
    let weighted_body = probe_signal(probes, |probe| probe.weighted_body);
    let model_output = probe_signal(probes, |probe| probe.output);
    let mix_residual = mix_residual(&model_output, &weighted_pickup, &weighted_body);
    let bow_force = probe_signal(probes, |probe| probe.bow_force);
    let bow_wave_injection = probe_signal(probes, |probe| probe.bow_wave_correction);
    Ok(BowDiagnosticReport {
        case_id: case.id,
        frequency_hz,
        smoothed_frequency_hz: mean_probe(probes, |probe| probe.current_frequency_hz),
        one_way_delay_samples: mean_probe(probes, |probe| probe.one_way_delay_samples),
        start_seconds: start as f32 / CATALOG_SAMPLE_RATE as f32,
        end_seconds: end as f32 / CATALOG_SAMPLE_RATE as f32,
        output: analyze_bow_signal("final output", output, frequency_hz),
        pickup_tap: analyze_bow_signal("pickup tap", &pickup_tap, frequency_hz),
        body_radiation: analyze_bow_signal("body radiation", &body_radiation, frequency_hz),
        weighted_pickup: analyze_bow_signal("weighted pickup", &weighted_pickup, frequency_hz),
        weighted_body: analyze_bow_signal("weighted body", &weighted_body, frequency_hz),
        pickup_weight: mean_probe(probes, |probe| probe.pickup_weight),
        body_weight: mean_probe(probes, |probe| probe.body_weight),
        mix_residual_rms: rms(&mix_residual),
        model_output: analyze_bow_signal("model output", &model_output, frequency_hz),
        bow_force: analyze_bow_signal("bow force", &bow_force, frequency_hz),
        bow_wave_injection: analyze_bow_signal(
            "bow wave injection",
            &bow_wave_injection,
            frequency_hz,
        ),
    })
}

pub(crate) fn bow_diagnostic_window(
    case: &CatalogCase,
    target_frames: usize,
) -> Result<(usize, usize), RenderError> {
    let Some(first) = case.schedule.notes.first() else {
        return Err(RenderError::BowDiagnosticEmptyWindow { case_id: case.id });
    };
    let phrase_start_seconds = case
        .schedule
        .notes
        .iter()
        .map(|note| note.start_seconds)
        .fold(first.start_seconds, f32::min);
    let phrase_end_seconds = case
        .schedule
        .notes
        .iter()
        .map(|note| note.end_seconds)
        .fold(first.end_seconds, f32::max);
    let release_margin_seconds = 0.10;
    let window_end_seconds =
        (phrase_end_seconds - release_margin_seconds).max(phrase_start_seconds);
    let window_start_seconds =
        (window_end_seconds - BOW_DIAGNOSTIC_WINDOW_SECONDS).max(phrase_start_seconds + 0.25);
    let start = seconds_to_frame(window_start_seconds).min(target_frames);
    let end = seconds_to_frame(window_end_seconds).min(target_frames);
    if end.saturating_sub(start) < 2_048 {
        return Err(RenderError::BowDiagnosticEmptyWindow { case_id: case.id });
    }
    Ok((start, end))
}

pub(crate) fn analyze_bow_signal(
    name: &'static str,
    samples: &[f32],
    frequency_hz: f32,
) -> BowDiagnosticRow {
    let (dominant_hz, dominant_magnitude) =
        dominant_peak(samples, CATALOG_SAMPLE_RATE as f32, 80.0, 5_000.0);
    let (high_peak_hz, high_peak_magnitude) =
        dominant_peak(samples, CATALOG_SAMPLE_RATE as f32, 4_000.0, 12_000.0);
    let (nearest_harmonic, nearest_harmonic_cents) = nearest_harmonic(dominant_hz, frequency_hz);
    let h1 = windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, frequency_hz);
    let spectrum = bow_spectrum_summary(samples);
    BowDiagnosticRow {
        name,
        peak: peak_abs(samples),
        rms: rms(samples),
        dominant_hz,
        dominant_dbfs: amplitude_db(dominant_magnitude),
        nearest_harmonic,
        nearest_harmonic_cents,
        h1_dbfs: amplitude_db(h1),
        h1_relative_db: ratio_db(h1, dominant_magnitude),
        h2_relative_db: harmonic_relative_db(samples, frequency_hz, 2, dominant_magnitude),
        h3_relative_db: harmonic_relative_db(samples, frequency_hz, 3, dominant_magnitude),
        h4_relative_db: harmonic_relative_db(samples, frequency_hz, 4, dominant_magnitude),
        h8_relative_db: harmonic_relative_db(samples, frequency_hz, 8, dominant_magnitude),
        h16_relative_db: harmonic_relative_db(samples, frequency_hz, 16, dominant_magnitude),
        h24_relative_db: harmonic_relative_db(samples, frequency_hz, 24, dominant_magnitude),
        high_peak_hz,
        high_peak_dbfs: amplitude_db(high_peak_magnitude),
        high_peak_relative_db: ratio_db(high_peak_magnitude, dominant_magnitude),
        spectrum,
    }
}

pub(crate) fn harmonic_relative_db(
    samples: &[f32],
    frequency_hz: f32,
    harmonic: usize,
    reference: f32,
) -> f32 {
    ratio_db(
        windowed_dft_magnitude_at(
            samples,
            CATALOG_SAMPLE_RATE as f32,
            frequency_hz * harmonic as f32,
        ),
        reference,
    )
}

pub(crate) fn bow_spectrum_summary(samples: &[f32]) -> BowSpectrumSummary {
    let sample_rate = CATALOG_SAMPLE_RATE as f32;
    let high_limit = 16_000.0_f32.min(sample_rate * 0.49);
    let mut bands = [0.0; BOW_SPECTRUM_BAND_COUNT];
    let mut sampled_power = Vec::new();
    let mut total_power = 0.0;
    let mut weighted_power = 0.0;
    let mut frequency = 80.0;

    while frequency <= high_limit {
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, frequency);
        let power = magnitude * magnitude;
        total_power += power;
        weighted_power += frequency * power;
        sampled_power.push((frequency, power));
        for (index, (_, low_hz, high_hz)) in BOW_SPECTRUM_BANDS.iter().copied().enumerate() {
            if frequency >= low_hz && frequency < high_hz {
                bands[index] += power;
                break;
            }
        }
        frequency += BOW_SPECTRUM_STEP_HZ;
    }

    let mut cumulative_power = 0.0;
    let rolloff_target = total_power * 0.95;
    let mut rolloff95_hz = 0.0;
    for (frequency, power) in sampled_power {
        cumulative_power += power;
        if cumulative_power >= rolloff_target {
            rolloff95_hz = frequency;
            break;
        }
    }

    let low_mid_power = bands[..4].iter().sum::<f32>();
    let high_power = bands[4..].iter().sum::<f32>();
    BowSpectrumSummary {
        band_power_relative_db: std::array::from_fn(|index| {
            power_ratio_db(bands[index], total_power)
        }),
        high_low_power_db: power_ratio_db(high_power, low_mid_power),
        centroid_hz: if total_power > 1.0e-24 {
            weighted_power / total_power
        } else {
            0.0
        },
        rolloff95_hz,
    }
}

pub(crate) fn dominant_peak(
    samples: &[f32],
    sample_rate: f32,
    low_hz: f32,
    high_hz: f32,
) -> (f32, f32) {
    let mut best_hz = low_hz;
    let mut best_magnitude = 0.0;
    let mut probe_hz = low_hz;
    while probe_hz <= high_hz {
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, probe_hz);
        if magnitude > best_magnitude {
            best_hz = probe_hz;
            best_magnitude = magnitude;
        }
        probe_hz += 10.0;
    }

    let refine_start = (best_hz - 12.0).max(low_hz);
    let refine_end = (best_hz + 12.0).min(high_hz);
    probe_hz = refine_start;
    while probe_hz <= refine_end {
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, probe_hz);
        if magnitude > best_magnitude {
            best_hz = probe_hz;
            best_magnitude = magnitude;
        }
        probe_hz += 1.0;
    }
    (best_hz, best_magnitude)
}

pub(crate) fn nearest_harmonic(frequency_hz: f32, fundamental_hz: f32) -> (usize, f32) {
    if frequency_hz <= 0.0 || fundamental_hz <= 0.0 {
        return (0, 0.0);
    }
    let harmonic = (frequency_hz / fundamental_hz).round().max(1.0) as usize;
    let harmonic_hz = fundamental_hz * harmonic as f32;
    let cents = 1_200.0 * (frequency_hz / harmonic_hz).log2();
    (harmonic, cents)
}

pub(crate) fn probe_signal(
    probes: &[StringModelProbe],
    mut value: impl FnMut(&StringModelProbe) -> f32,
) -> Vec<f32> {
    probes.iter().map(&mut value).collect()
}

pub(crate) fn mean_probe(
    probes: &[StringModelProbe],
    mut value: impl FnMut(&StringModelProbe) -> f32,
) -> f32 {
    if probes.is_empty() {
        return 0.0;
    }
    probes.iter().map(&mut value).sum::<f32>() / probes.len() as f32
}

pub(crate) fn mix_residual(output: &[f32], pickup: &[f32], body: &[f32]) -> Vec<f32> {
    output
        .iter()
        .copied()
        .zip(pickup.iter().copied())
        .zip(body.iter().copied())
        .map(|((output, pickup), body)| output - pickup - body)
        .collect()
}

#[allow(dead_code)]
impl BowDiagnosticReport {
    pub(crate) fn summary(&self) -> String {
        let mut output = String::new();
        output.push_str("Lamath bow physical diagnostic\n");
        output.push_str(&format!("case: {}\n", self.case_id));
        output.push_str(&format!(
            "window: {:.3}-{:.3} s\n",
            self.start_seconds, self.end_seconds
        ));
        output.push_str(&format!(
            "target: {:.2} Hz; smoothed string: {:.2} Hz; one-way delay: {:.2} samples\n\n",
            self.frequency_hz, self.smoothed_frequency_hz, self.one_way_delay_samples
        ));

        output.push_str("speaker sanity\n");
        output.push_str(&format!(
            "  peak {:.4} ({})  rms {:.4} ({})  dominant {}\n",
            self.output.peak,
            format_db(amplitude_db(self.output.peak)).trim(),
            self.output.rms,
            format_db(amplitude_db(self.output.rms)).trim(),
            self.output.peak_description()
        ));
        output.push_str(&format!("  flags: {}\n\n", self.safety_flags()));

        output.push_str("internal spectra\n");
        output.push_str(&format_bow_row(&self.pickup_tap));
        output.push_str(&format_bow_row(&self.body_radiation));
        output.push_str(&format_bow_row(&self.weighted_pickup));
        output.push_str(&format_bow_row(&self.weighted_body));
        output.push_str(&format_bow_row(&self.model_output));
        output.push_str(&format_bow_row(&self.bow_force));
        output.push_str(&format_bow_row(&self.bow_wave_injection));
        output.push_str(&format_bow_row(&self.output));
        output.push_str("\nfull-spectrum sampled power\n");
        output.push_str(&format_bow_spectrum_header());
        output.push_str(&format_bow_spectrum_row(&self.pickup_tap));
        output.push_str(&format_bow_spectrum_row(&self.weighted_body));
        output.push_str(&format_bow_spectrum_row(&self.model_output));
        output.push_str(&format_bow_spectrum_row(&self.bow_wave_injection));
        output.push_str(&format_bow_spectrum_row(&self.output));
        output.push_str(&format!(
            "\nmean pickup/body weights: {:.3}/{:.3}; raw body/pickup rms: {} dB; weighted body/pickup rms: {} dB; bow-wave/model-output rms: {} dB; mix residual rms: {}\n",
            self.pickup_weight,
            self.body_weight,
            format_db(ratio_db(self.body_radiation.rms, self.pickup_tap.rms)).trim(),
            format_db(ratio_db(self.weighted_body.rms, self.weighted_pickup.rms)).trim(),
            format_db(ratio_db(self.bow_wave_injection.rms, self.model_output.rms)).trim(),
            format_db(amplitude_db(self.mix_residual_rms)).trim(),
        ));
        output.push_str("physical read: ");
        output.push_str(self.physical_read());
        output.push('\n');
        output
    }

    fn safety_flags(&self) -> &'static str {
        if self.output.peak > 0.98 {
            "peak near full scale"
        } else if self.output.nearest_harmonic >= 4 && self.output.h1_relative_db < -18.0 {
            "dominant upper harmonic, weak fundamental"
        } else {
            "ok"
        }
    }

    // The output spectrum is the arbiter of contact health, not the force
    // spectrum: healthy Helmholtz force is corner-shaped (sharp stick/slip
    // transitions), so its strongest single FFT bin is often a high partial
    // even when the string sings its fundamental.
    fn physical_read(&self) -> &'static str {
        let output_high =
            self.output.nearest_harmonic >= 4 || self.model_output.nearest_harmonic >= 4;
        let contact_high =
            self.bow_force.nearest_harmonic >= 4 || self.bow_wave_injection.nearest_harmonic >= 4;
        if output_high && contact_high {
            "rendered tone and contact force both sit on upper partials; the contact is likely not establishing Helmholtz motion — check the stick/slip regime (Schelleng overpressure ratio) before touching body/output balance."
        } else if output_high {
            "the contact tracks the fundamental but the rendered tone selects upper partials; look at bridge/body/output weighting."
        } else {
            "rendered tone is fundamental-led; a high-harmonic-dominant contact force alongside this is the healthy corner-shaped Helmholtz force, not a defect."
        }
    }
}

#[allow(dead_code)]
impl BowDiagnosticRow {
    fn peak_description(&self) -> String {
        format!(
            "{:.1} Hz ~= H{} ({:+.0} cents), {} dBFS",
            self.dominant_hz,
            self.nearest_harmonic,
            self.nearest_harmonic_cents,
            format_db(self.dominant_dbfs).trim()
        )
    }
}

#[allow(dead_code)]
pub(crate) fn format_bow_row(row: &BowDiagnosticRow) -> String {
    format!(
        "  {:<18} rms {} dBFS  dom {:>7.1} Hz H{:<2} {:+5.0}c {} dBFS  H1 {} dBFS ({:>7}) H2 {:>7} H3 {:>7} H4 {:>7} H8 {:>7} H16 {:>7} H24 {:>7} >4k {:>7.1}Hz {:>7} dBFS ({:>7})\n",
        row.name,
        format_db(amplitude_db(row.rms)).trim(),
        row.dominant_hz,
        row.nearest_harmonic,
        row.nearest_harmonic_cents,
        format_db(row.dominant_dbfs).trim(),
        format_db(row.h1_dbfs).trim(),
        format_db(row.h1_relative_db).trim(),
        format_db(row.h2_relative_db).trim(),
        format_db(row.h3_relative_db).trim(),
        format_db(row.h4_relative_db).trim(),
        format_db(row.h8_relative_db).trim(),
        format_db(row.h16_relative_db).trim(),
        format_db(row.h24_relative_db).trim(),
        row.high_peak_hz,
        format_db(row.high_peak_dbfs).trim(),
        format_db(row.high_peak_relative_db).trim(),
    )
}

pub(crate) fn format_bow_spectrum_header() -> String {
    let mut output = String::from("  signal             ");
    for (label, _, _) in BOW_SPECTRUM_BANDS {
        output.push_str(&format!(" {label:>8}"));
    }
    output.push_str("   hi/lo centroid roll95\n");
    output
}
