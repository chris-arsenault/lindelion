use lindelion_onset_detect::{MarkerKind, SliceMarker};
use lindelion_pitch_detect::{PitchContour, PitchFrame};

use super::*;

#[test]
fn source_cache_key_is_deterministic_and_source_derived() {
    let analyzer = PitchShiftAnalyzer::default();
    let audio = sine_wave(220.0, 48_000, 4_800);
    let contour = pitch_contour(48_000, &[Some(220.0), Some(220.0), Some(220.0)]);
    let markers = markers(&[0, 2_400]);

    let first = analyzer
        .analyze(&audio, 48_000, &contour, &markers)
        .unwrap();
    let second = analyzer
        .analyze(&audio, 48_000, &contour, &markers)
        .unwrap();
    let mut edited_audio = audio.clone();
    edited_audio[128] *= 0.25;
    let edited = analyzer
        .analyze(&edited_audio, 48_000, &contour, &markers)
        .unwrap();

    assert_eq!(first.key, second.key);
    assert_ne!(first.key.audio_hash, edited.key.audio_hash);
    assert_eq!(first.key.marker_hash, edited.key.marker_hash);
}

#[test]
fn analysis_derives_voicing_segments_and_slice_summaries() {
    let analyzer = PitchShiftAnalyzer::default();
    let mut audio = sine_wave(220.0, 48_000, 4_800);
    audio.extend(vec![0.0; 4_800]);
    let contour = pitch_contour(48_000, &[Some(220.0), Some(220.0), None, None, Some(330.0)]);
    let markers = markers(&[0, 4_800]);

    let cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers)
        .unwrap();

    assert_eq!(cache.slice_summaries.len(), 2);
    assert_eq!(cache.slice_summaries[0].detected_f0_hz, Some(220.0));
    assert!(cache.slice_summaries[0].voiced_ratio > 0.5);
    assert!(cache.slice_summaries[1].voiced_ratio < 0.5);
    assert_eq!(cache.voicing_segments[0].kind, VoicingKind::Voiced);
    assert_eq!(cache.voicing_segments[1].kind, VoicingKind::Unvoiced);
    assert!(
        cache
            .frames
            .iter()
            .all(|frame| frame.residual.aperiodic_ratio.is_finite())
    );
}

#[test]
fn pitch_adaptive_envelope_tracks_source_formant_peak() {
    let analyzer = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 96,
        ..PitchShiftAnalysisConfig::default()
    });
    let audio = harmonic_stack_with_formant(110.0, 1_000.0, 48_000, 8_192);
    let contour = pitch_contour(48_000, &[Some(110.0), Some(110.0), Some(110.0)]);
    let cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers(&[0]))
        .unwrap();

    let peak = cache.frames[1]
        .spectral_envelope
        .points
        .iter()
        .max_by(|left, right| left.magnitude.total_cmp(&right.magnitude))
        .unwrap();

    assert!((peak.frequency_hz - 1_000.0).abs() < 300.0);
    assert_eq!(
        cache.frames[1].spectral_envelope.harmonic_spacing_hz,
        Some(110.0)
    );
}

#[test]
fn invalid_inputs_are_rejected_before_cache_creation() {
    let analyzer = PitchShiftAnalyzer::default();
    let contour = pitch_contour(48_000, &[Some(220.0)]);

    assert_eq!(
        analyzer.analyze(&[], 48_000, &contour, &[]),
        Err(PitchShiftAnalysisError::EmptySource)
    );
    assert_eq!(
        analyzer.analyze(&[0.0], 0, &contour, &[]),
        Err(PitchShiftAnalysisError::InvalidSampleRate)
    );
    assert_eq!(
        analyzer.analyze(&[0.0], 48_000, &pitch_contour(48_000, &[]), &[]),
        Err(PitchShiftAnalysisError::EmptyPitchContour)
    );
}

#[test]
fn synthesis_shifts_pitch_while_preserving_envelope_peak() {
    let analyzer = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 96,
        ..PitchShiftAnalysisConfig::default()
    });
    let audio = harmonic_stack_with_formant(110.0, 1_000.0, 48_000, 12_000);
    let contour = pitch_contour(
        48_000,
        &[
            Some(110.0),
            Some(110.0),
            Some(110.0),
            Some(110.0),
            Some(110.0),
        ],
    );
    let source_cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers(&[0]))
        .unwrap();
    let rendered = PitchShiftEngine
        .render_slice(
            &audio,
            &source_cache,
            PitchShiftSliceRenderRequest {
                slice_index: 0,
                config: PitchShiftRenderConfig {
                    ratios: PitchShiftRatios {
                        pitch_ratio: 2.0,
                        formant_ratio: None,
                    },
                    residual_policy: ResidualMixPolicy::Muted,
                    ..PitchShiftRenderConfig::default()
                },
            },
        )
        .unwrap();

    let shifted_f0 = lindelion_dsp_utils::analysis::dft_magnitude_at(&rendered, 48_000.0, 220.0);
    let original_f0 = lindelion_dsp_utils::analysis::dft_magnitude_at(&rendered, 48_000.0, 110.0);
    assert!(shifted_f0 > original_f0 * 4.0);

    let output_contour = pitch_contour(
        48_000,
        &[
            Some(220.0),
            Some(220.0),
            Some(220.0),
            Some(220.0),
            Some(220.0),
        ],
    );
    let output_cache = analyzer
        .analyze(&rendered, 48_000, &output_contour, &markers(&[0]))
        .unwrap();
    let peak = envelope_peak_hz(&output_cache.frames[2].spectral_envelope);

    assert!((peak - 1_000.0).abs() < 350.0, "peak was {peak}");
}

#[test]
fn synthesis_shifts_sine_to_expected_output_frequency() {
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

    for (semitones, expected_hz) in [(-12.0, 110.0), (7.0, 329.63), (12.0, 440.0)] {
        let pitch_ratio = PitchShiftRatios::from_semitones_cents(semitones, 0.0).pitch_ratio;
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

        let estimated_hz = lindelion_dsp_utils::analysis::estimate_frequency_zero_crossings(
            &rendered[2_048..],
            sample_rate as f32,
        )
        .unwrap();
        assert!(
            (estimated_hz - expected_hz).abs() < 2.0,
            "expected {expected_hz:.2} Hz from {semitones:+.0} st, got {estimated_hz:.2} Hz"
        );
    }
}

#[test]
fn formant_ratio_moves_envelope_when_requested() {
    let analyzer = PitchShiftAnalyzer::new(PitchShiftAnalysisConfig {
        frame_size: 4096,
        envelope_points: 128,
        ..PitchShiftAnalysisConfig::default()
    });
    let audio = harmonic_stack_with_formant(110.0, 900.0, 48_000, 12_000);
    let contour = pitch_contour(
        48_000,
        &[
            Some(110.0),
            Some(110.0),
            Some(110.0),
            Some(110.0),
            Some(110.0),
        ],
    );
    let source_cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers(&[0]))
        .unwrap();
    let rendered = PitchShiftEngine
        .render_slice(
            &audio,
            &source_cache,
            PitchShiftSliceRenderRequest {
                slice_index: 0,
                config: PitchShiftRenderConfig {
                    ratios: PitchShiftRatios {
                        pitch_ratio: 2.0,
                        formant_ratio: Some(2.0),
                    },
                    residual_policy: ResidualMixPolicy::Muted,
                    ..PitchShiftRenderConfig::default()
                },
            },
        )
        .unwrap();
    let output_contour = pitch_contour(
        48_000,
        &[
            Some(220.0),
            Some(220.0),
            Some(220.0),
            Some(220.0),
            Some(220.0),
        ],
    );
    let output_cache = analyzer
        .analyze(&rendered, 48_000, &output_contour, &markers(&[0]))
        .unwrap();
    let peak = envelope_peak_hz(&output_cache.frames[2].spectral_envelope);

    assert!((peak - 1_800.0).abs() < 500.0, "peak was {peak}");
}

#[test]
fn synthesis_preserves_unvoiced_regions_by_policy() {
    let analyzer = PitchShiftAnalyzer::default();
    let audio = vec![0.0, 0.25, -0.1, 0.4, -0.2, 0.0];
    let contour = pitch_contour(48_000, &[None]);
    let cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers(&[0]))
        .unwrap();

    let rendered = PitchShiftEngine
        .render_slice(
            &audio,
            &cache,
            PitchShiftSliceRenderRequest::new(0, PitchShiftRatios::identity()),
        )
        .unwrap();

    assert_eq!(rendered, audio);
}

#[test]
fn synthesis_sample_api_matches_full_slice_render() {
    let analyzer = PitchShiftAnalyzer::default();
    let audio = sine_wave(220.0, 48_000, 4_800);
    let contour = pitch_contour(48_000, &[Some(220.0), Some(220.0), Some(220.0)]);
    let cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers(&[0]))
        .unwrap();
    let ratios = PitchShiftRatios {
        pitch_ratio: 1.25,
        formant_ratio: None,
    };
    let rendered = PitchShiftEngine
        .render_slice(&audio, &cache, PitchShiftSliceRenderRequest::new(0, ratios))
        .unwrap();

    for offset in [0, 32, 256, 1_024] {
        let sample = PitchShiftEngine
            .render_slice_sample(
                &audio,
                &cache,
                PitchShiftSliceSampleRequest::new(0, offset as f32, ratios),
            )
            .unwrap();
        assert!((sample - rendered[offset]).abs() < 0.000_001);
    }
}

#[test]
fn synthesis_region_sample_api_can_extend_beyond_slice_boundary() {
    let analyzer = PitchShiftAnalyzer::default();
    let audio = sine_wave(220.0, 48_000, 4_800);
    let contour = pitch_contour(48_000, &[Some(220.0), Some(220.0), Some(220.0)]);
    let cache = analyzer
        .analyze(&audio, 48_000, &contour, &markers(&[0, 1_200]))
        .unwrap();

    let sample = PitchShiftEngine
        .render_region_sample(
            &audio,
            &cache,
            PitchShiftRegionSampleRequest::new(
                0,
                audio.len(),
                1_500.0,
                PitchShiftRatios::identity(),
            ),
        )
        .unwrap();
    let slice_sample = PitchShiftEngine
        .render_slice_sample(
            &audio,
            &cache,
            PitchShiftSliceSampleRequest::new(0, 1_500.0, PitchShiftRatios::identity()),
        )
        .unwrap();

    assert_eq!(sample, audio[1_500]);
    assert_eq!(slice_sample, 0.0);
}

fn pitch_contour(sample_rate: u32, f0_values: &[Option<f32>]) -> PitchContour {
    PitchContour {
        source_sample_rate: sample_rate,
        analysis_sample_rate: sample_rate,
        hop_size: 1_200,
        frames: f0_values
            .iter()
            .copied()
            .enumerate()
            .map(|(index, f0_hz)| pitch_frame(index, index * 2_400, f0_hz, sample_rate))
            .collect(),
    }
}

fn constant_pitch_contour(sample_rate: u32, f0_hz: f32, len: usize) -> PitchContour {
    PitchContour {
        source_sample_rate: sample_rate,
        analysis_sample_rate: sample_rate,
        hop_size: 1_200,
        frames: (0..len)
            .step_by(1_200)
            .enumerate()
            .map(|(index, source_sample_position)| {
                pitch_frame(index, source_sample_position, Some(f0_hz), sample_rate)
            })
            .collect(),
    }
}

fn pitch_frame(
    frame_index: usize,
    source_sample_position: usize,
    f0_hz: Option<f32>,
    sample_rate: u32,
) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / sample_rate as f32,
        f0_hz,
        raw_f0_hz: f0_hz.unwrap_or(0.0),
        confidence: if f0_hz.is_some() { 0.95 } else { 0.1 },
        voiced: f0_hz.is_some(),
        rms: if f0_hz.is_some() { 0.2 } else { 0.0 },
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

fn sine_wave(frequency_hz: f32, sample_rate: u32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate as f32).sin() * 0.5
        })
        .collect()
}

fn harmonic_stack_with_formant(
    f0_hz: f32,
    formant_hz: f32,
    sample_rate: u32,
    len: usize,
) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (1..24)
                .map(|harmonic| {
                    let frequency = f0_hz * harmonic as f32;
                    let formant_gain = gaussian(frequency, formant_hz, 180.0);
                    let phase =
                        std::f32::consts::TAU * frequency * index as f32 / sample_rate as f32;
                    phase.sin() * formant_gain
                })
                .sum::<f32>()
                * 0.15
        })
        .collect()
}

fn gaussian(value: f32, center: f32, width: f32) -> f32 {
    let normalized = (value - center) / width;
    (-0.5 * normalized * normalized).exp()
}

fn envelope_peak_hz(envelope: &SpectralEnvelope) -> f32 {
    envelope
        .points
        .iter()
        .max_by(|left, right| left.magnitude.total_cmp(&right.magnitude))
        .map(|point| point.frequency_hz)
        .unwrap_or(0.0)
}
