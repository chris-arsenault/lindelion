use super::{constant_pitch_contour, harmonic_stack_with_formant, markers};
use crate::{
    PitchShiftAnalysisConfig, PitchShiftAnalyzer, PitchShiftEngine, PitchShiftRatios,
    PitchShiftRegionSampleRequest, PitchShiftRenderConfig, PitchShiftSynthesisAlgorithm,
    ResidualMixPolicy,
};

#[test]
fn harmonic_fallback_phase_tracks_absolute_position() {
    // The harmonic fallback must derive its phase from the absolute sample position
    // (the phase offset), not the region-relative source offset — otherwise adjacent
    // slices reset phase and click at the seam. Rendering the same source offset at
    // two different phase offsets must therefore produce different output.
    let sample_rate = 48_000;
    let audio = harmonic_stack_with_formant(150.0, 1_200.0, sample_rate, 9_600);
    let contour = constant_pitch_contour(sample_rate, 150.0, audio.len());
    let cache = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    })
    .analyze(&audio, sample_rate, &contour, &markers(&[0]))
    .unwrap();

    let config = PitchShiftRenderConfig {
        algorithm: PitchShiftSynthesisAlgorithm::Auto,
        ratios: PitchShiftRatios {
            pitch_ratio: 1.5,
            formant_ratio: Some(1.0),
        },
        residual_policy: ResidualMixPolicy::Muted,
        ..PitchShiftRenderConfig::default()
    };
    let render = |phase_offset: f32| {
        PitchShiftEngine
            .render_region_sample(
                &audio,
                &cache,
                PitchShiftRegionSampleRequest {
                    start_sample: 0,
                    end_sample: audio.len(),
                    offset_samples: 1_000.0,
                    phase_offset_samples: Some(phase_offset),
                    config,
                },
            )
            .unwrap()
    };
    // ~quarter period of the 225 Hz shifted tone; the fallback must respond to it.
    let a = render(1_000.0);
    let b = render(1_053.0);
    assert!(
        (a - b).abs() > 1e-3,
        "harmonic fallback ignored the phase offset (region-relative phase): a={a}, b={b}"
    );
}
