use std::path::Path;

use lindelion_dsp_utils::analysis::{
    InterPeakFloorBand, assert_all_finite, fixed_analysis_region, gain_fitted_rms_difference,
    inter_peak_floor_ratio, inter_peak_floor_ratio_in_band, peak_abs, reference_peak_frequencies,
    rms, sampled_high_frequency_ratio, shifted_frequencies, windowed_dft_energy_at,
};
use lindelion_sample_library::decode_wav_mono;

use super::render_resample_pro;
use crate::PitchShiftRatios;

const WIND_REFERENCE_PEAKS_HZ: [f32; 8] = [
    466.0, 524.0, 554.0, 1_394.0, 1_577.0, 1_846.0, 2_078.0, 2_320.0,
];

#[test]
fn resample_pro_wind_fixture_pitch_offsets_stay_clean_before_linnod_output() {
    let (source, known_bad_one_cent, sample_rate) =
        split_wind_fixture_audio("crunch_pitch_shift_regression.wav");
    let (_, known_bad_seventeen_cent, seventeen_cent_sample_rate) =
        split_wind_fixture_audio("crunch_17c_pitch_shift_regression.wav");
    assert_eq!(sample_rate, seventeen_cent_sample_rate);

    let source_metrics = WindSourceMetrics::new(&source, sample_rate);
    let identity = render_resample_pro(&source, sample_rate, WIND_REFERENCE_PEAKS_HZ[0], 1.0);
    let identity_metrics =
        WindRenderMetrics::new(&identity, &source, sample_rate, &source_metrics.peaks_hz);

    assert_clean_identity_wind_render(&source_metrics, &identity_metrics);

    for case in [
        WindPitchCase::new("1c", 0.0, 1.0, Some(&known_bad_one_cent)),
        WindPitchCase::new("17c", 0.0, 17.0, Some(&known_bad_seventeen_cent)),
        WindPitchCase::new("50c", 0.0, 50.0, None),
        WindPitchCase::new("7st", 7.0, 0.0, None),
        WindPitchCase::new("12st", 12.0, 0.0, None),
    ] {
        assert_clean_wind_pitch_shift(
            &source,
            sample_rate,
            &source_metrics,
            &identity_metrics,
            case,
        );
    }
}

fn assert_clean_identity_wind_render(source: &WindSourceMetrics, identity: &WindRenderMetrics) {
    assert!(
        identity.fitted_error_ratio <= 0.08,
        "identity wind render should stay close to source after gain fitting; error={}",
        identity.fitted_error_ratio
    );
    assert!(
        identity.inter_peak_floor <= source.inter_peak_floor * 1.8 + 0.000_05,
        "identity wind render should not raise the inter-peak floor; source={}, identity={}",
        source.inter_peak_floor,
        identity.inter_peak_floor
    );
    assert!(
        identity.high_band_ratio <= source.high_band_ratio * 1.8 + 0.003,
        "identity wind render should not create high-band energy; source={}, identity={}",
        source.high_band_ratio,
        identity.high_band_ratio
    );
    assert_bounded_wind_level("identity", source, identity);
}

fn assert_clean_wind_pitch_shift(
    source: &[f32],
    sample_rate: u32,
    source_metrics: &WindSourceMetrics,
    identity_metrics: &WindRenderMetrics,
    case: WindPitchCase<'_>,
) {
    let pitch_ratio = case.pitch_ratio();
    let target_peaks = shifted_frequencies(&source_metrics.peaks_hz, pitch_ratio);
    let output = render_resample_pro(source, sample_rate, WIND_REFERENCE_PEAKS_HZ[0], pitch_ratio);
    let output_metrics = WindRenderMetrics::new(&output, source, sample_rate, &target_peaks);

    assert_all_finite(&output);
    assert_bounded_wind_level(case.name, source_metrics, &output_metrics);
    assert_clean_wind_floor(source_metrics, identity_metrics, &output_metrics, case);
    assert_clean_wind_high_band_floor(source_metrics, identity_metrics, &output_metrics, case);
    assert_shifted_anchor_partials_dominate_source_partials(
        &output,
        sample_rate,
        pitch_ratio,
        case,
    );

    if let Some(known_bad_shift) = case.known_bad_shift {
        assert_known_bad_wind_fixture_is_noisier(
            known_bad_shift,
            sample_rate,
            source_metrics,
            &target_peaks,
            &output_metrics,
            case,
        );
    }
}

fn assert_bounded_wind_level(
    case_name: &str,
    source: &WindSourceMetrics,
    output: &WindRenderMetrics,
) {
    assert!(
        output.rms > source.rms * 0.15,
        "{case_name} wind output collapsed in level; source={}, output={}",
        source.rms,
        output.rms
    );
    assert!(
        output.peak <= source.peak * 1.75 + 0.02,
        "{case_name} wind output peak should stay bounded without limiting; source_peak={}, output_peak={}",
        source.peak,
        output.peak
    );
}

fn assert_clean_wind_floor(
    source: &WindSourceMetrics,
    identity: &WindRenderMetrics,
    output: &WindRenderMetrics,
    case: WindPitchCase<'_>,
) {
    let identity_relative_limit = identity.inter_peak_floor * case.floor_multiplier() + 0.000_05;
    let source_relative_limit = source.inter_peak_floor * case.floor_multiplier() * 1.25 + 0.000_05;
    let floor_limit = identity_relative_limit.max(source_relative_limit);
    assert!(
        output.inter_peak_floor <= floor_limit,
        "{} Resample Pro wind pitch should not raise the inter-peak floor; source={}, identity={}, output={}, limit={floor_limit}",
        case.name,
        source.inter_peak_floor,
        identity.inter_peak_floor,
        output.inter_peak_floor
    );
}

fn assert_clean_wind_high_band_floor(
    source: &WindSourceMetrics,
    identity: &WindRenderMetrics,
    output: &WindRenderMetrics,
    case: WindPitchCase<'_>,
) {
    let high_band_floor_limit = identity
        .high_band_floor_ratio
        .max(source.high_band_floor_ratio)
        * case.high_band_floor_multiplier()
        + case.high_band_margin();
    assert!(
        output.high_band_floor_ratio <= high_band_floor_limit,
        "{} Resample Pro wind pitch should not add broad high-band residue; source_floor={}, identity_floor={}, output_floor={}, source_energy={}, output_energy={}, limit={high_band_floor_limit}",
        case.name,
        source.high_band_floor_ratio,
        identity.high_band_floor_ratio,
        output.high_band_floor_ratio,
        source.high_band_ratio,
        output.high_band_ratio
    );
}

fn assert_shifted_anchor_partials_dominate_source_partials(
    output: &[f32],
    sample_rate: u32,
    pitch_ratio: f32,
    case: WindPitchCase<'_>,
) {
    if case.semitones.abs() < 1.0 {
        return;
    }

    let steady = wind_steady_region(output);
    let source_stack = peak_stack_energy(steady, sample_rate as f32, &WIND_REFERENCE_PEAKS_HZ[..3]);
    let target_peaks = shifted_frequencies(&WIND_REFERENCE_PEAKS_HZ[..3], pitch_ratio);
    let target_stack = peak_stack_energy(steady, sample_rate as f32, &target_peaks);
    assert!(
        target_stack > source_stack * 1.35,
        "{} Resample Pro wind pitch should move the anchor partials to the target ratio; source_stack={}, target_stack={target_stack}",
        case.name,
        source_stack
    );
}

fn assert_known_bad_wind_fixture_is_noisier(
    known_bad_shift: &[f32],
    sample_rate: u32,
    source: &WindSourceMetrics,
    target_peaks: &[f32],
    output: &WindRenderMetrics,
    case: WindPitchCase<'_>,
) {
    let known_bad =
        WindRenderMetrics::new(known_bad_shift, known_bad_shift, sample_rate, target_peaks);
    assert!(
        known_bad.inter_peak_floor > source.inter_peak_floor * 2.5,
        "{} wind fixture should expose the broken render; source={}, known_bad={}",
        case.name,
        source.inter_peak_floor,
        known_bad.inter_peak_floor
    );
    assert!(
        output.inter_peak_floor < known_bad.inter_peak_floor * 0.6,
        "{} Resample Pro wind pitch should be materially cleaner than the recorded bad half; output={}, known_bad={}",
        case.name,
        output.inter_peak_floor,
        known_bad.inter_peak_floor
    );
}

struct WindSourceMetrics {
    peaks_hz: Vec<f32>,
    inter_peak_floor: f32,
    high_band_floor_ratio: f32,
    high_band_ratio: f32,
    rms: f32,
    peak: f32,
}

impl WindSourceMetrics {
    fn new(source: &[f32], sample_rate: u32) -> Self {
        let steady = wind_steady_region(source);
        let peaks_hz = reference_peak_frequencies(
            steady,
            sample_rate as f32,
            100.0,
            6_000.0,
            25.0,
            &WIND_REFERENCE_PEAKS_HZ,
        );
        Self {
            inter_peak_floor: inter_peak_floor_ratio(steady, sample_rate as f32, &peaks_hz),
            high_band_floor_ratio: high_band_floor_ratio(steady, sample_rate, &peaks_hz),
            high_band_ratio: sampled_high_frequency_ratio(
                steady,
                sample_rate as f32,
                6_000.0,
                100.0,
            ),
            rms: rms(steady),
            peak: peak_abs(source),
            peaks_hz,
        }
    }
}

struct WindRenderMetrics {
    inter_peak_floor: f32,
    high_band_floor_ratio: f32,
    high_band_ratio: f32,
    fitted_error_ratio: f32,
    rms: f32,
    peak: f32,
}

impl WindRenderMetrics {
    fn new(rendered: &[f32], source: &[f32], sample_rate: u32, target_peaks_hz: &[f32]) -> Self {
        let rendered_steady = wind_steady_region(rendered);
        let source_steady = wind_steady_region(source);
        Self {
            inter_peak_floor: inter_peak_floor_ratio(
                rendered_steady,
                sample_rate as f32,
                target_peaks_hz,
            ),
            high_band_floor_ratio: high_band_floor_ratio(
                rendered_steady,
                sample_rate,
                target_peaks_hz,
            ),
            high_band_ratio: sampled_high_frequency_ratio(
                rendered_steady,
                sample_rate as f32,
                6_000.0,
                100.0,
            ),
            fitted_error_ratio: gain_fitted_rms_difference(source_steady, rendered_steady)
                / rms(source_steady).max(1.0e-9),
            rms: rms(rendered_steady),
            peak: peak_abs(rendered),
        }
    }
}

#[derive(Clone, Copy)]
struct WindPitchCase<'a> {
    name: &'static str,
    semitones: f32,
    cents: f32,
    known_bad_shift: Option<&'a [f32]>,
}

impl<'a> WindPitchCase<'a> {
    const fn new(
        name: &'static str,
        semitones: f32,
        cents: f32,
        known_bad_shift: Option<&'a [f32]>,
    ) -> Self {
        Self {
            name,
            semitones,
            cents,
            known_bad_shift,
        }
    }

    fn pitch_ratio(self) -> f32 {
        PitchShiftRatios::from_semitones_cents(self.semitones, self.cents).pitch_ratio
    }

    fn floor_multiplier(self) -> f32 {
        if self.semitones.abs() < f32::EPSILON && self.cents.abs() <= 17.0 {
            2.0
        } else if self.semitones.abs() < f32::EPSILON {
            2.4
        } else {
            3.5
        }
    }

    fn high_band_floor_multiplier(self) -> f32 {
        if self.semitones.abs() < f32::EPSILON {
            2.0
        } else if self.semitones <= 7.0 {
            4.0
        } else {
            8.0
        }
    }

    fn high_band_margin(self) -> f32 {
        if self.semitones.abs() < f32::EPSILON {
            0.004
        } else if self.semitones <= 7.0 {
            0.02
        } else {
            0.05
        }
    }
}

fn split_wind_fixture_audio(filename: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(filename)
        .canonicalize()
        .unwrap();
    let decoded = decode_wav_mono(&path).unwrap();
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.channels, 2);
    let half = decoded.samples.len() / 2;
    assert!(half > 16_384);
    let fixture_len = half.min(48_000);
    let source = decoded.samples[..fixture_len].to_vec();
    let known_bad_shift = decoded.samples[half..half + fixture_len].to_vec();
    (source, known_bad_shift, decoded.sample_rate)
}

fn wind_steady_region(samples: &[f32]) -> &[f32] {
    fixed_analysis_region(samples, 2_048, 8_192)
}

fn high_band_floor_ratio(samples: &[f32], sample_rate: u32, peaks_hz: &[f32]) -> f32 {
    inter_peak_floor_ratio_in_band(
        samples,
        sample_rate as f32,
        peaks_hz,
        InterPeakFloorBand::new(6_000.0, sample_rate as f32 * 0.45, 50.0, 35.0),
    )
}

fn peak_stack_energy(samples: &[f32], sample_rate: f32, peaks_hz: &[f32]) -> f32 {
    peaks_hz
        .iter()
        .copied()
        .filter(|frequency_hz| *frequency_hz < sample_rate * 0.45)
        .map(|frequency_hz| windowed_dft_energy_at(samples, sample_rate, frequency_hz))
        .sum()
}
