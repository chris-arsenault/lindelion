//! Tube render-tap analysis (`--tube-tap-analysis`).
#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) const TAP_RATIO_HARMONIC_COUNT: usize = 8;

pub(crate) const TAP_FLOOR_BAND_COUNT: usize = 5;

pub(crate) const TAP_RATIO_HARMONICS: [usize; TAP_RATIO_HARMONIC_COUNT] =
    [2, 3, 4, 5, 7, 9, 11, 13];

pub(crate) const TAP_FLOOR_BANDS: [(&str, f32, f32); TAP_FLOOR_BAND_COUNT] = [
    ("floor_0.2_0.8k", 200.0, 800.0),
    ("floor_0.8_1.2k", 800.0, 1_200.0),
    ("floor_1.2_2k", 1_200.0, 2_000.0),
    ("floor_2_6k", 2_000.0, 6_000.0),
    ("floor_6_12k", 6_000.0, 12_000.0),
];

pub(crate) const TAP_ANALYSIS_MAX_SECONDS: f32 = 1.5;

#[derive(Debug, Clone)]
pub(crate) struct TubeTapAnalysisReport {
    pub(crate) case_id: &'static str,
    pub(crate) frequency_hz: f32,
    pub(crate) start_seconds: f32,
    pub(crate) end_seconds: f32,
    pub(crate) rows: Vec<TubeTapAnalysisRow>,
}

#[derive(Debug, Clone)]
pub(crate) struct TubeTapAnalysisRow {
    pub(crate) name: &'static str,
    pub(crate) rms_dbfs: f32,
    pub(crate) h1_dbfs: f32,
    pub(crate) harmonic_ratios_db: [f32; TAP_RATIO_HARMONIC_COUNT],
    pub(crate) h15_h25_db: f32,
    pub(crate) h27_h39_db: f32,
    pub(crate) floors_dbfs: [f32; TAP_FLOOR_BAND_COUNT],
}

pub(crate) fn analyze_tube_taps(case: &CatalogCase) -> Result<TubeTapAnalysisReport, RenderError> {
    let RenderTarget::Tube(patch) = target_for_recipe(case.patch_recipe) else {
        return Err(RenderError::TubeTapAnalysisRequiresTube { case_id: case.id });
    };
    let frequency_hz = single_pitch_frequency_hz(case)?;
    let target_frames = case.target_frames();
    let (_, _, tap_traces) = render_tube_with_taps(patch, case.schedule.notes, target_frames);
    let (start, end) = tap_analysis_window(case, target_frames)?;
    let rows = TUBE_RENDER_TAP_NAMES
        .iter()
        .copied()
        .zip(tap_traces.iter())
        .map(|(name, trace)| analyze_tap_trace(name, &trace[start..end], frequency_hz))
        .collect();
    Ok(TubeTapAnalysisReport {
        case_id: case.id,
        frequency_hz,
        start_seconds: start as f32 / CATALOG_SAMPLE_RATE as f32,
        end_seconds: end as f32 / CATALOG_SAMPLE_RATE as f32,
        rows,
    })
}

pub(crate) fn tap_analysis_window(
    case: &CatalogCase,
    target_frames: usize,
) -> Result<(usize, usize), RenderError> {
    let Some(first) = case.schedule.notes.first() else {
        return Err(RenderError::TubeTapAnalysisEmptyWindow { case_id: case.id });
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
    let duration = (phrase_end_seconds - phrase_start_seconds).max(0.0);
    if duration <= f32::EPSILON || target_frames == 0 {
        return Err(RenderError::TubeTapAnalysisEmptyWindow { case_id: case.id });
    }

    let trim_seconds = (duration * 0.18).clamp(0.20, 0.90);
    let trimmed_start = phrase_start_seconds + trim_seconds.min(duration * 0.35);
    let trimmed_end = phrase_end_seconds - trim_seconds.min(duration * 0.35);
    let (start_seconds, end_seconds) = if trimmed_end > trimmed_start {
        let max_duration = TAP_ANALYSIS_MAX_SECONDS.min(trimmed_end - trimmed_start);
        let center = (trimmed_start + trimmed_end) * 0.5;
        (center - max_duration * 0.5, center + max_duration * 0.5)
    } else {
        (phrase_start_seconds, phrase_end_seconds)
    };

    let start = seconds_to_frame(start_seconds).min(target_frames);
    let end = seconds_to_frame(end_seconds).min(target_frames);
    if end.saturating_sub(start) < 1024 {
        return Err(RenderError::TubeTapAnalysisEmptyWindow { case_id: case.id });
    }
    Ok((start, end))
}

pub(crate) fn seconds_to_frame(seconds: f32) -> usize {
    (seconds.max(0.0) * CATALOG_SAMPLE_RATE as f32).round() as usize
}

pub(crate) fn analyze_tap_trace(
    name: &'static str,
    samples: &[f32],
    frequency_hz: f32,
) -> TubeTapAnalysisRow {
    let h1 = windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, frequency_hz);
    let harmonic_ratios_db = std::array::from_fn(|index| {
        let harmonic = TAP_RATIO_HARMONICS[index] as f32;
        let magnitude =
            windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, frequency_hz * harmonic);
        ratio_db(magnitude, h1)
    });
    let floors_dbfs = std::array::from_fn(|index| {
        let (_, start_hz, end_hz) = TAP_FLOOR_BANDS[index];
        inter_harmonic_floor_dbfs(samples, start_hz, end_hz, frequency_hz)
    });
    TubeTapAnalysisRow {
        name,
        rms_dbfs: amplitude_db(rms(samples)),
        h1_dbfs: amplitude_db(h1),
        harmonic_ratios_db,
        h15_h25_db: harmonic_average_ratio_db(samples, frequency_hz, 15, 25, h1),
        h27_h39_db: harmonic_average_ratio_db(samples, frequency_hz, 27, 39, h1),
        floors_dbfs,
    }
}

pub(crate) fn harmonic_average_ratio_db(
    samples: &[f32],
    frequency_hz: f32,
    first: usize,
    last: usize,
    h1: f32,
) -> f32 {
    let nyquist = CATALOG_SAMPLE_RATE as f32 * 0.5;
    let mut power = 0.0;
    let mut count = 0;
    for harmonic in first..=last {
        let probe_hz = frequency_hz * harmonic as f32;
        if probe_hz >= nyquist {
            continue;
        }
        let magnitude = windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, probe_hz);
        power += magnitude * magnitude;
        count += 1;
    }
    if count == 0 {
        return f32::NEG_INFINITY;
    }
    ratio_db((power / count as f32).sqrt(), h1)
}

pub(crate) fn inter_harmonic_floor_dbfs(
    samples: &[f32],
    start_hz: f32,
    end_hz: f32,
    frequency_hz: f32,
) -> f32 {
    let nyquist = CATALOG_SAMPLE_RATE as f32 * 0.5;
    let end_hz = end_hz.min(nyquist * 0.98);
    if end_hz <= start_hz {
        return f32::NEG_INFINITY;
    }
    let step_hz = (frequency_hz * 0.73).clamp(80.0, 180.0);
    let reject_radius = frequency_hz * 0.18;
    let mut probe_hz = start_hz + step_hz * 0.5;
    let mut power = 0.0;
    let mut count = 0;
    while probe_hz < end_hz {
        let nearest_harmonic = (probe_hz / frequency_hz).round().max(1.0) * frequency_hz;
        if (probe_hz - nearest_harmonic).abs() > reject_radius {
            let magnitude =
                windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, probe_hz);
            power += magnitude * magnitude;
            count += 1;
        }
        probe_hz += step_hz;
    }
    if count == 0 {
        return f32::NEG_INFINITY;
    }
    amplitude_db((power / count as f32).sqrt())
}

pub(crate) fn ratio_db(magnitude: f32, reference: f32) -> f32 {
    amplitude_db(magnitude / reference.max(1.0e-12))
}

pub(crate) fn power_ratio_db(power: f32, reference: f32) -> f32 {
    if power.is_finite() && power > 0.0 && reference.is_finite() && reference > 0.0 {
        10.0 * (power / reference.max(1.0e-24)).log10()
    } else {
        f32::NEG_INFINITY
    }
}

pub(crate) fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude.is_finite() && amplitude > 0.0 {
        20.0 * amplitude.max(1.0e-12).log10()
    } else {
        f32::NEG_INFINITY
    }
}

pub(crate) fn format_db(value: f32) -> String {
    if value.is_finite() {
        format!("{value:>7.1}")
    } else {
        "   -inf".to_string()
    }
}

impl TubeTapAnalysisReport {
    pub(crate) fn summary(&self) -> String {
        let mut output = String::new();
        output.push_str("Lamath Tube tap analysis\n");
        output.push_str(&format!("case: {}\n", self.case_id));
        output.push_str(&format!("f0: {:.2} Hz\n", self.frequency_hz));
        output.push_str(&format!(
            "window: {:.3}-{:.3} s\n",
            self.start_seconds, self.end_seconds
        ));
        output.push_str(
            "tap                           rms     h1     h2     h3     h4     h5     h7     h9    h11    h13  h15-25 h27-39  fl0.2  fl0.8  fl1.2    fl2    fl6\n",
        );
        for row in &self.rows {
            output.push_str(&format!(
                "{:<28} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                row.name,
                format_db(row.rms_dbfs),
                format_db(row.h1_dbfs),
                format_db(row.harmonic_ratios_db[0]),
                format_db(row.harmonic_ratios_db[1]),
                format_db(row.harmonic_ratios_db[2]),
                format_db(row.harmonic_ratios_db[3]),
                format_db(row.harmonic_ratios_db[4]),
                format_db(row.harmonic_ratios_db[5]),
                format_db(row.harmonic_ratios_db[6]),
                format_db(row.harmonic_ratios_db[7]),
                format_db(row.h15_h25_db),
                format_db(row.h27_h39_db),
                format_db(row.floors_dbfs[0]),
                format_db(row.floors_dbfs[1]),
                format_db(row.floors_dbfs[2]),
                format_db(row.floors_dbfs[3]),
                format_db(row.floors_dbfs[4]),
            ));
        }
        output.push_str("harmonic columns after h1 are dB relative to that tap's h1; floor columns are inter-harmonic dBFS bands.\n");
        output
    }
}
