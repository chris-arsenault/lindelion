use std::path::Path;

use lindelion_dsp_utils::analysis::{
    assert_all_finite, fixed_analysis_region, inter_peak_floor_ratio, reference_peak_frequencies,
    rms, shifted_frequencies,
};
use lindelion_onset_detect::DetectionConfig;
use lindelion_onset_detect::{MarkerKind, SliceMarker};
use lindelion_sample_library::{OwnedMonoAudioBuffer, SampleMetadata, decode_wav_mono};

use super::note_on;
use crate::analysis::LinnodSourceAnalyzer;
use crate::{LinnodPatch, PitchOffset};

const CRUNCH_PEAKS_HZ: [f32; 8] = [
    466.0, 524.0, 554.0, 1_394.0, 1_577.0, 1_846.0, 2_078.0, 2_320.0,
];

#[test]
fn pad_mode_pitch_offsets_keep_crunch_output_clean() {
    let (source, known_bad_one_cent, sample_rate) =
        split_fixture_audio("crunch_pitch_shift_regression.wav");
    let (_, known_bad_seventeen_cent, _) =
        split_fixture_audio("crunch_17c_pitch_shift_regression.wav");
    let source_metrics = CrunchMetrics::new(&source, sample_rate);
    for case in [
        PitchCase::new("1c", 0, 1.0, Some(&known_bad_one_cent)),
        PitchCase::new("17c", 0, 17.0, Some(&known_bad_seventeen_cent)),
        PitchCase::new("7st", 7, 0.0, None),
        PitchCase::new("12st", 12, 0.0, None),
    ] {
        assert_clean_pitch_offset(&source, sample_rate, &source_metrics, case);
    }
}

fn assert_clean_pitch_offset(
    source: &[f32],
    sample_rate: u32,
    source_metrics: &CrunchMetrics,
    case: PitchCase<'_>,
) {
    let pitch = PitchOffset {
        semitones: case.semitones,
        cents: case.cents,
    };
    let output = render_pad_pitch_offset(source, sample_rate, pitch);
    let output_steady = steady_region(&output);
    let output_floor = inter_peak_floor_ratio(
        output_steady,
        sample_rate as f32,
        &shifted_frequencies(&source_metrics.peaks_hz, pitch.ratio()),
    );
    let output_rms = rms(output_steady);
    assert!(
        output_rms > source_metrics.rms * 0.15,
        "{} output collapsed in level; source_rms={}, output_rms={output_rms}",
        case.name,
        source_metrics.rms
    );
    assert!(
        output_floor < source_metrics.floor * 2.0 + 0.000_05,
        "{} pad pitch should not raise the inter-peak floor; source={}, output={output_floor}",
        case.name,
        source_metrics.floor
    );
    if let Some(known_bad_shift) = case.known_bad_shift {
        assert_known_bad_is_noisier(
            known_bad_shift,
            sample_rate,
            source_metrics,
            case,
            output_floor,
        );
    }
    assert_all_finite(&output);
}

fn assert_known_bad_is_noisier(
    known_bad_shift: &[f32],
    sample_rate: u32,
    source_metrics: &CrunchMetrics,
    case: PitchCase<'_>,
    output_floor: f32,
) {
    let known_bad_floor = inter_peak_floor_ratio(
        steady_region(known_bad_shift),
        sample_rate as f32,
        &shifted_frequencies(
            &source_metrics.peaks_hz,
            PitchOffset {
                semitones: case.semitones,
                cents: case.cents,
            }
            .ratio(),
        ),
    );
    assert!(
        known_bad_floor > source_metrics.floor * 2.5,
        "{} fixture should expose the broken render; source={}, known_bad={known_bad_floor}",
        case.name,
        source_metrics.floor
    );
    assert!(
        output_floor < known_bad_floor * 0.6,
        "{} pad pitch should be materially cleaner than the recorded bad half; output={output_floor}, known_bad={known_bad_floor}",
        case.name
    );
}

struct CrunchMetrics {
    peaks_hz: Vec<f32>,
    floor: f32,
    rms: f32,
}

impl CrunchMetrics {
    fn new(source: &[f32], sample_rate: u32) -> Self {
        let source_steady = steady_region(source);
        let peaks_hz = reference_peak_frequencies(
            source_steady,
            sample_rate as f32,
            100.0,
            6_000.0,
            25.0,
            &CRUNCH_PEAKS_HZ,
        );
        let floor = inter_peak_floor_ratio(source_steady, sample_rate as f32, &peaks_hz);
        let rms = rms(source_steady);
        Self {
            peaks_hz,
            floor,
            rms,
        }
    }
}

fn render_pad_pitch_offset(source: &[f32], sample_rate: u32, pitch: PitchOffset) -> Vec<f32> {
    let analysis = source_analysis_from_detected_pitch(source.to_vec(), sample_rate);
    let mut patch = LinnodPatch::default();
    patch.slices[0].pitch = pitch;
    let mut processor = super::super::LinnodProcessor::new(sample_rate as f32);
    processor.prepare_source_analysis(&patch, &analysis);
    let mut left = vec![0.0; source.len()];
    let mut right = vec![0.0; source.len()];

    processor.process(
        &patch,
        Some(&analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );
    assert_all_finite(&right);
    left
}

fn split_fixture_audio(filename: &str) -> (Vec<f32>, Vec<f32>, u32) {
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

#[derive(Clone, Copy)]
struct PitchCase<'a> {
    name: &'static str,
    semitones: i32,
    cents: f32,
    known_bad_shift: Option<&'a [f32]>,
}

impl<'a> PitchCase<'a> {
    const fn new(
        name: &'static str,
        semitones: i32,
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
}

fn markers(positions: &[usize]) -> Vec<SliceMarker> {
    positions
        .iter()
        .copied()
        .map(|position_samples| SliceMarker {
            position_samples,
            kind: MarkerKind::Auto,
        })
        .collect()
}

fn source_analysis_from_detected_pitch(
    samples: Vec<f32>,
    sample_rate: u32,
) -> crate::analysis::SourceAnalysis {
    let detection = DetectionConfig {
        min_slice_ms: 5_000.0,
        ..DetectionConfig::default()
    };
    LinnodSourceAnalyzer::default()
        .analyze(
            SampleMetadata {
                reference: lindelion_sample_library::SampleReference::new(
                    "hash",
                    "Samples/crunch_pitch_shift_regression.wav",
                ),
                filename: "crunch_pitch_shift_regression.wav".to_string(),
                duration_ms: ((samples.len() as f32 / sample_rate as f32) * 1_000.0).round() as u64,
                sample_rate,
                channels: 1,
                rms_db: None,
                peak_db: None,
                waveform_preview: lindelion_sample_library::SampleWaveformPreview {
                    points: Vec::new(),
                },
            },
            OwnedMonoAudioBuffer::new(samples, sample_rate),
            detection,
            &markers(&[0]),
        )
        .unwrap()
}

fn steady_region(samples: &[f32]) -> &[f32] {
    fixed_analysis_region(samples, 2_048, 8_192)
}
