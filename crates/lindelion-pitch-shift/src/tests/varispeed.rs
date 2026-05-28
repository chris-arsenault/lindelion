use super::{
    constant_pitch_contour, fitted_sine_rms_error, high_frequency_artifact_ratio, markers,
    sine_wave,
};
use crate::{
    PitchShiftAnalysisConfig, PitchShiftAnalyzer, PitchShiftEngine, PitchShiftRatios,
    PitchShiftRenderConfig, PitchShiftSliceRenderRequest, PitchShiftSynthesisAlgorithm,
    ResidualMixPolicy,
};

#[test]
fn varispeed_synthesis_shifts_sine_without_algorithmic_artifacts() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let audio = sine_wave(source_f0_hz, sample_rate, sample_rate as usize);
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    let source_cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(&audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();

    for (semitones, cents) in [(0.0, 1.0), (0.0, 50.0), (7.0, 0.0), (12.0, 0.0)] {
        let pitch_ratio = PitchShiftRatios::from_semitones_cents(semitones, cents).pitch_ratio;
        let rendered = PitchShiftEngine
            .render_slice(
                &audio,
                &source_cache,
                PitchShiftSliceRenderRequest {
                    slice_index: 0,
                    config: PitchShiftRenderConfig {
                        algorithm: PitchShiftSynthesisAlgorithm::Varispeed,
                        ratios: PitchShiftRatios {
                            pitch_ratio,
                            formant_ratio: None,
                        },
                        residual_policy: ResidualMixPolicy::Muted,
                        ..PitchShiftRenderConfig::default()
                    },
                },
            )
            .unwrap();
        let target_hz = source_f0_hz * pitch_ratio;
        let valid_len = (rendered.len() as f32 / pitch_ratio).floor() as usize;
        let steady_end = valid_len.min(rendered.len()).saturating_sub(4_096);
        let steady = &rendered[4_096..steady_end];
        let estimated_hz = lindelion_dsp_utils::analysis::estimate_frequency_zero_crossings(
            steady,
            sample_rate as f32,
        )
        .unwrap();
        let fitted_error = fitted_sine_rms_error(steady, sample_rate as f32, target_hz);
        let high_artifact_ratio =
            high_frequency_artifact_ratio(steady, sample_rate as f32, target_hz);

        assert!(
            (estimated_hz - target_hz).abs() < 0.5,
            "expected {target_hz:.3} Hz from {semitones:+.0} st {cents:+.0} c, got {estimated_hz:.3} Hz"
        );
        assert!(
            fitted_error < 0.004,
            "varispeed sine shift {semitones:+.0} st {cents:+.0} c should stay clean; fitted_error={fitted_error}"
        );
        assert!(
            high_artifact_ratio < 0.002,
            "varispeed sine shift {semitones:+.0} st {cents:+.0} c added high-frequency artifact ratio={high_artifact_ratio}"
        );
    }
}
