use std::path::Path;

use lindelion_dsp_utils::analysis::{
    assert_all_finite, estimate_f0_autocorrelation, gain_fitted_rms_difference,
    sampled_high_frequency_ratio,
};
use lindelion_onset_detect::{MarkerKind, SliceMarker};
use lindelion_sample_library::decode_wav_mono;

use super::{note_on, source_analysis_from_samples};
use crate::{LinnodPatch, PitchOffset};

#[test]
fn pad_mode_one_cent_sax_fixture_stays_clean() {
    let (samples, sample_rate, source_f0_hz) = sax_fixture_audio();
    let analysis = source_analysis_from_samples(
        samples.clone(),
        sample_rate,
        markers(&[0]),
        source_f0_hz,
        "sax_test.wav",
    );
    let mut patch = LinnodPatch::default();
    patch.slices[0].pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    let mut processor = super::super::LinnodProcessor::new(sample_rate as f32);
    processor.prepare_source_analysis(&patch, &analysis);
    let mut left = vec![0.0; samples.len()];
    let mut right = vec![0.0; samples.len()];

    processor.process(
        &patch,
        Some(&analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    let source_steady = &samples[1_024..samples.len() - 1_024];
    let output_steady = &left[1_024..left.len() - 1_024];
    let fitted_error = gain_fitted_rms_difference(source_steady, output_steady);
    let source_high_ratio =
        sampled_high_frequency_ratio(source_steady, sample_rate as f32, 6_000.0, 100.0);
    let output_high_ratio =
        sampled_high_frequency_ratio(output_steady, sample_rate as f32, 6_000.0, 100.0);

    assert!(
        fitted_error < 0.16,
        "1-cent sax pad output should stay close after gain fitting; error={fitted_error}"
    );
    assert!(
        output_high_ratio < source_high_ratio * 1.6 + 0.005,
        "1-cent sax pad output added high-frequency energy; source={source_high_ratio}, output={output_high_ratio}"
    );
    assert_all_finite(&left);
    assert_all_finite(&right);
}

fn sax_fixture_audio() -> (Vec<f32>, u32, f32) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio/sax_test.wav")
        .canonicalize()
        .unwrap();
    let decoded = decode_wav_mono(&path).unwrap();
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.channels, 2);
    let source_f0_hz =
        estimate_f0_autocorrelation(&decoded.samples, decoded.sample_rate as f32, 80.0, 1_000.0)
            .expect("sax fixture should have a measurable fundamental");
    (decoded.samples, decoded.sample_rate, source_f0_hz)
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
