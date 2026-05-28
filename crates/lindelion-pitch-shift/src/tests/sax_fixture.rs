use std::path::Path;

use lindelion_dsp_utils::analysis::{
    estimate_f0_autocorrelation, gain_fitted_rms_difference, sampled_high_frequency_ratio,
};
use lindelion_sample_library::decode_wav_mono;

use super::{constant_pitch_contour, markers};
use crate::{
    PitchShiftAnalysisConfig, PitchShiftAnalyzer, PitchShiftEngine, PitchShiftRatios,
    PitchShiftRenderConfig, PitchShiftSliceRenderRequest, ResidualMixPolicy,
};

#[test]
fn synthesis_one_cent_sax_fixture_stays_close_and_low_artifact() {
    let (audio, sample_rate) = sax_fixture_audio();
    let source_f0_hz = estimate_f0_autocorrelation(&audio, sample_rate as f32, 80.0, 1_000.0)
        .expect("sax fixture should have a measurable fundamental");
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    let source_cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(&audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(0.0, 1.0).pitch_ratio;
    let rendered = PitchShiftEngine
        .render_slice(
            &audio,
            &source_cache,
            PitchShiftSliceRenderRequest {
                slice_index: 0,
                config: PitchShiftRenderConfig {
                    ratios: PitchShiftRatios {
                        pitch_ratio,
                        formant_ratio: Some(pitch_ratio),
                    },
                    residual_policy: ResidualMixPolicy::Muted,
                    ..PitchShiftRenderConfig::default()
                },
            },
        )
        .unwrap();
    let source_steady = &audio[1_024..audio.len() - 1_024];
    let rendered_steady = &rendered[1_024..rendered.len() - 1_024];
    let source_centroid =
        lindelion_dsp_utils::analysis::spectral_centroid_hz(source_steady, sample_rate as f32)
            .unwrap();
    let rendered_centroid =
        lindelion_dsp_utils::analysis::spectral_centroid_hz(rendered_steady, sample_rate as f32)
            .unwrap();
    let source_high_ratio =
        sampled_high_frequency_ratio(source_steady, sample_rate as f32, 6_000.0, 100.0);
    let rendered_high_ratio =
        sampled_high_frequency_ratio(rendered_steady, sample_rate as f32, 6_000.0, 100.0);
    let fitted_error = gain_fitted_rms_difference(source_steady, rendered_steady);

    assert!(
        fitted_error < 0.15,
        "1-cent sax shift should stay close to source after gain fitting; error={fitted_error}"
    );
    assert!(
        rendered_centroid < source_centroid * 1.10 + 100.0,
        "1-cent sax shift should not brighten substantially; source={source_centroid}, rendered={rendered_centroid}"
    );
    assert!(
        rendered_high_ratio < source_high_ratio * 1.5 + 0.002,
        "1-cent sax shift added high-frequency energy; source={source_high_ratio}, rendered={rendered_high_ratio}"
    );
}

#[test]
fn synthesis_semitone_sax_fixture_hits_expected_frequency_ratio() {
    let (audio, sample_rate) = sax_fixture_audio();
    let source_f0_hz = estimate_f0_autocorrelation(&audio, sample_rate as f32, 80.0, 1_000.0)
        .expect("sax fixture should have a measurable fundamental");
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    let source_cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(&audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(1.0, 0.0).pitch_ratio;
    let rendered = PitchShiftEngine
        .render_slice(
            &audio,
            &source_cache,
            PitchShiftSliceRenderRequest {
                slice_index: 0,
                config: PitchShiftRenderConfig {
                    ratios: PitchShiftRatios {
                        pitch_ratio,
                        formant_ratio: Some(pitch_ratio),
                    },
                    residual_policy: ResidualMixPolicy::Muted,
                    ..PitchShiftRenderConfig::default()
                },
            },
        )
        .unwrap();
    let rendered_f0_hz = estimate_f0_autocorrelation(&rendered, sample_rate as f32, 80.0, 1_200.0)
        .expect("rendered sax fixture should have a measurable fundamental");
    let measured_ratio = rendered_f0_hz / source_f0_hz;
    let cents_error = 1200.0 * (measured_ratio / pitch_ratio).log2();
    let source_high_ratio =
        sampled_high_frequency_ratio(&audio, sample_rate as f32, 6_000.0, 100.0);
    let rendered_high_ratio =
        sampled_high_frequency_ratio(&rendered, sample_rate as f32, 6_000.0, 100.0);

    assert!(
        cents_error.abs() < 25.0,
        "expected +1 semitone sax shift, measured ratio {measured_ratio:.6} ({cents_error:.2} cents error)"
    );
    assert!(
        rendered_high_ratio < source_high_ratio * 2.0 + 0.004,
        "semitone sax shift added high-frequency energy; source={source_high_ratio}, rendered={rendered_high_ratio}"
    );
}

fn sax_fixture_audio() -> (Vec<f32>, u32) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio/sax_test.wav")
        .canonicalize()
        .unwrap();
    let decoded = decode_wav_mono(&path).unwrap();
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.channels, 2);
    assert!(decoded.samples.len() > 8_192);
    (decoded.samples, decoded.sample_rate)
}
