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
fn resample_stretch_compat_shifts_sine_cleanly() {
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
                        algorithm: PitchShiftSynthesisAlgorithm::ResampleStretch,
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
        let steady = &rendered[4_096..rendered.len() - 4_096];
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
            fitted_error < 0.01,
            "resample-stretch sine shift {semitones:+.0} st {cents:+.0} c should stay clean; fitted_error={fitted_error}"
        );
        assert!(
            high_artifact_ratio < 0.002,
            "resample-stretch sine shift {semitones:+.0} st {cents:+.0} c added high-frequency artifact ratio={high_artifact_ratio}"
        );
    }
}

#[test]
fn resample_stretch_slice_render_is_guarded_resample_pro_region() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let audio = sine_wave(source_f0_hz, sample_rate, sample_rate as usize);
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    let source_cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(&audio, sample_rate, &contour, &markers(&[0, 12_000]))
    .unwrap();
    let ratios = PitchShiftRatios::from_semitones_cents(0.0, 50.0);
    let rendered_slice = PitchShiftEngine
        .render_slice(
            &audio,
            &source_cache,
            PitchShiftSliceRenderRequest {
                slice_index: 1,
                config: PitchShiftRenderConfig {
                    algorithm: PitchShiftSynthesisAlgorithm::ResampleStretch,
                    ratios,
                    residual_policy: ResidualMixPolicy::Preserve,
                    ..PitchShiftRenderConfig::default()
                },
            },
        )
        .unwrap();
    let slice = source_cache.slice_summary(1).unwrap();
    let rendered_pro_region = crate::resample_pro_render::render_region_pitch_shift_with_source(
        &audio,
        &source_cache,
        slice.start_sample,
        slice.end_sample,
        ratios,
    )
    .unwrap();

    assert_eq!(
        rendered_slice, rendered_pro_region,
        "ResampleStretch must remain a guarded Resample Pro compatibility path, not spectral-peak plus residual synthesis"
    );
}

#[test]
fn resample_stretch_point_render_is_resample_pro_sample() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let audio = sine_wave(source_f0_hz, sample_rate, sample_rate as usize / 2);
    let contour = constant_pitch_contour(sample_rate, source_f0_hz, audio.len());
    let source_cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(&audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();
    let ratios = PitchShiftRatios::from_semitones_cents(7.0, 0.0);
    for offset in [4_096.0, 8_192.0, 12_288.0] {
        let sample = PitchShiftEngine
            .render_slice_sample(
                &audio,
                &source_cache,
                crate::PitchShiftSliceSampleRequest {
                    slice_index: 0,
                    offset_samples: offset,
                    config: PitchShiftRenderConfig {
                        algorithm: PitchShiftSynthesisAlgorithm::ResampleStretch,
                        ratios,
                        ..PitchShiftRenderConfig::default()
                    },
                },
            )
            .unwrap();
        let center = offset.floor().max(0.0) as usize;
        let start = center.saturating_sub(2);
        let end = center
            .saturating_add(3)
            .min(source_cache.source_len_samples);
        let rendered_region = crate::resample_pro_render::render_region_pitch_shift_with_source(
            &audio,
            &source_cache,
            start,
            end,
            ratios,
        )
        .unwrap();
        let expected = lindelion_dsp_utils::interpolation::cubic_f64(
            &rendered_region,
            offset as f64 - start as f64,
        );
        assert_eq!(sample, expected);
    }
}
