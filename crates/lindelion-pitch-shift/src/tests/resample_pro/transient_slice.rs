use lindelion_dsp_utils::{
    analysis::{assert_all_finite, max_adjacent_delta, peak_abs},
    db_to_gain,
};

use super::{analyze_resample_pro_cache, low_band_harmonic_stack};
use crate::{
    PitchShiftEngine, PitchShiftRatios, PitchShiftRenderConfig, PitchShiftSliceRenderRequest,
    PitchShiftSynthesisAlgorithm, ResidualMixPolicy,
};

#[test]
fn resample_pro_transient_has_low_preecho() {
    let sample_rate = 48_000;
    let impulse_position = 12_000usize;
    let pitch_ratio = PitchShiftRatios::from_semitones_cents(7.0, 0.0).pitch_ratio;
    let mut audio = vec![0.0; sample_rate as usize];
    audio[impulse_position] = 1.0;
    let cache = analyze_resample_pro_cache(&audio, sample_rate, 440.0, &[0, impulse_position]);
    let rendered = crate::resample_pro_render::render_region_pitch_shift_with_source(
        &audio,
        &cache,
        0,
        audio.len(),
        PitchShiftRatios {
            pitch_ratio,
            formant_ratio: None,
        },
    )
    .unwrap();
    let peak_index = peak_abs_index(&rendered).unwrap();
    let timing_error = peak_index.abs_diff(impulse_position);
    let pre_echo = energy(&rendered[peak_index.saturating_sub(512)..peak_index.saturating_sub(32)]);
    let attack = energy(&rendered[peak_index..(peak_index + 64).min(rendered.len())]);

    assert_all_finite(&rendered);
    assert!(
        timing_error <= 2,
        "pitch-shifted transient should stay sample-aligned; expected={impulse_position}, peak={peak_index}, error={timing_error}"
    );
    assert!(
        pre_echo <= attack * db_to_gain(-50.0).powi(2) + 1.0e-12,
        "pitch-shifted transient pre-echo should stay below -50 dB; pre_echo={pre_echo}, attack={attack}"
    );
    assert!(peak_abs(&rendered) > 0.1, "transient peak should survive");
}

#[test]
fn resample_pro_slice_render_with_guard_context_trims_clean_edges() {
    let sample_rate = 48_000;
    let source_f0_hz = 220.0;
    let slice_start = 12_000usize;
    let slice_end = 36_000usize;
    let ratios = PitchShiftRatios::from_semitones_cents(0.0, 50.0);
    let mut audio = low_band_harmonic_stack(source_f0_hz, sample_rate, sample_rate as usize);
    audio[slice_start] += 0.4;
    audio[slice_end] -= 0.4;
    let cache = analyze_resample_pro_cache(
        &audio,
        sample_rate,
        source_f0_hz,
        &[0, slice_start, slice_end],
    );
    let rendered_slice = PitchShiftEngine
        .render_slice(
            &audio,
            &cache,
            PitchShiftSliceRenderRequest {
                slice_index: 1,
                config: PitchShiftRenderConfig {
                    algorithm: PitchShiftSynthesisAlgorithm::ResampleStretch,
                    ratios,
                    residual_policy: ResidualMixPolicy::Muted,
                    ..PitchShiftRenderConfig::default()
                },
            },
        )
        .unwrap();
    let rendered_region = crate::resample_pro_render::render_region_pitch_shift_with_source(
        &audio,
        &cache,
        slice_start,
        slice_end,
        ratios,
    )
    .unwrap();
    let source_slice = &audio[slice_start..slice_end];

    assert_eq!(rendered_slice.len(), slice_end - slice_start);
    assert_eq!(rendered_slice, rendered_region);
    assert_all_finite(&rendered_slice);
    assert_guarded_edges_stay_bounded(&rendered_slice, source_slice);
}

fn assert_guarded_edges_stay_bounded(rendered: &[f32], source: &[f32]) {
    let edge_len = 512.min(rendered.len() / 4);
    let start_delta = max_adjacent_delta(&rendered[..edge_len]);
    let source_start_delta = max_adjacent_delta(&source[..edge_len]);
    let end_start = rendered.len().saturating_sub(edge_len);
    let end_delta = max_adjacent_delta(&rendered[end_start..]);
    let source_end_delta = max_adjacent_delta(&source[end_start..]);

    assert!(
        start_delta <= source_start_delta * 4.0 + 0.1,
        "guarded slice start should not acquire a new click; source_delta={source_start_delta}, rendered_delta={start_delta}"
    );
    assert!(
        end_delta <= source_end_delta * 4.0 + 0.1,
        "guarded slice end should not acquire a new click; source_delta={source_end_delta}, rendered_delta={end_delta}"
    );
}

fn energy(samples: &[f32]) -> f32 {
    samples.iter().map(|sample| sample * sample).sum()
}

fn peak_abs_index(samples: &[f32]) -> Option<usize> {
    samples
        .iter()
        .copied()
        .enumerate()
        .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
        .map(|(index, _)| index)
}
