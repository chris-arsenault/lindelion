use std::ops::RangeInclusive;

use lindelion_midi::QuantizeSettings;
use lindelion_pitch_detect::{PitchContour, PitchFrame};

use crate::{
    AnalysisResult, AnalysisSettings, ScratchpadAudio, analysis::analyze_with_pitch_contour,
};

const SAMPLE_RATE: u32 = 48_000;
const HOP_SAMPLES: usize = 768;

#[test]
fn silence_and_breath_noise_do_not_create_phantom_notes() {
    let silence = analyze_fixture(vec![0.0; SAMPLE_RATE as usize], |_| None);
    let breath = analyze_fixture(noise(SAMPLE_RATE as usize, 0.03), |_| None);

    assert!(silence.detected_notes.is_empty());
    assert!(breath.detected_notes.is_empty());
    assert_analysis_is_finite(&silence);
    assert_analysis_is_finite(&breath);
}

#[test]
fn soft_vowel_and_hard_consonant_onsets_segment_expected_notes() {
    let first_end = samples_for_ms(520);
    let second_end = samples_for_ms(1_080);
    let mut audio = tone(440.0, first_end, 0.35, samples_for_ms(120));
    audio.extend(noise(samples_for_ms(24), 0.18));
    audio.extend(tone(
        493.88,
        second_end - audio.len(),
        0.45,
        samples_for_ms(8),
    ));

    let result = analyze_fixture(audio, |sample| {
        if sample < first_end {
            Some(440.0)
        } else {
            Some(493.88)
        }
    });

    assert_note_count(&result, 2..=3);
    assert_close_cents(result.detected_notes[0].pitch_hz, 440.0, 60.0);
    assert_has_onset_near(&result, first_end, samples_for_ms(100));
}

#[test]
fn vibrato_and_scoop_resolve_to_one_stable_note() {
    let len = SAMPLE_RATE as usize;
    let result = analyze_fixture(tone(440.0, len, 0.35, samples_for_ms(180)), |sample| {
        let t = sample as f32 / SAMPLE_RATE as f32;
        let scoop = (t / 0.2).clamp(0.0, 1.0);
        let base = 392.0 + (440.0 - 392.0) * scoop;
        let vibrato_cents = 18.0 * (std::f32::consts::TAU * 5.2 * t).sin();
        Some(base * 2.0_f32.powf(vibrato_cents / 1200.0))
    });

    assert_note_count(&result, 1..=1);
    assert_close_cents(result.detected_notes[0].pitch_hz, 440.0, 80.0);
}

#[test]
fn legato_pitch_jump_splits_without_energy_transient() {
    let split = samples_for_ms(500);
    let len = samples_for_ms(1_000);
    let result = analyze_fixture(tone(440.0, len, 0.35, samples_for_ms(30)), |sample| {
        Some(if sample < split { 440.0 } else { 659.25 })
    });

    assert_note_count(&result, 2..=2);
    assert_has_onset_near(&result, split, samples_for_ms(100));
    assert_close_cents(result.detected_notes[1].pitch_hz, 659.25, 70.0);
}

#[test]
fn repeated_same_pitch_articulation_survives_as_two_notes() {
    let first_len = samples_for_ms(430);
    let gap_len = samples_for_ms(90);
    let second_len = samples_for_ms(430);
    let mut audio = tone(440.0, first_len, 0.35, samples_for_ms(20));
    audio.extend(noise(gap_len, 0.02));
    audio.extend(tone(440.0, second_len, 0.35, samples_for_ms(12)));
    let restart = first_len + gap_len;

    let result = analyze_fixture(audio, |sample| {
        let in_gap = (first_len..restart).contains(&sample);
        (!in_gap).then_some(440.0)
    });

    assert_note_count(&result, 2..=2);
    assert_has_onset_near(&result, restart, samples_for_ms(100));
    for note in &result.detected_notes {
        assert_close_cents(note.pitch_hz, 440.0, 50.0);
    }
}

#[test]
fn clipped_low_and_high_range_inputs_remain_finite_and_quantized() {
    let split = samples_for_ms(500);
    let mut audio = tone(55.0, split, 1.4, samples_for_ms(12));
    audio.extend(tone(1_975.0, split, 1.4, samples_for_ms(12)));
    for sample in &mut audio {
        *sample = sample.clamp(-1.0, 1.0);
    }

    let result = analyze_fixture(audio, |sample| {
        Some(if sample < split { 55.0 } else { 1_975.0 })
    });

    assert_note_count(&result, 2..=2);
    assert_analysis_is_finite(&result);
    assert_close_cents(result.detected_notes[0].pitch_hz, 55.0, 70.0);
    assert_close_cents(result.detected_notes[1].pitch_hz, 1_975.0, 70.0);
    assert!(
        result
            .midi_clip
            .notes
            .iter()
            .all(|note| note.midi_note <= 127)
    );
}

fn analyze_fixture(
    audio: Vec<f32>,
    pitch_at_sample: impl Fn(usize) -> Option<f32>,
) -> AnalysisResult {
    let contour = pitch_contour(audio.len(), pitch_at_sample);
    analyze_with_pitch_contour(
        &ScratchpadAudio::new(SAMPLE_RATE, audio),
        AnalysisSettings {
            confidence_threshold: 0.5,
            onset_sensitivity: 0.55,
            min_note_ms: 70.0,
        },
        &QuantizeSettings::default(),
        contour,
    )
}

fn pitch_contour(len: usize, pitch_at_sample: impl Fn(usize) -> Option<f32>) -> PitchContour {
    let frame_count = (len / HOP_SAMPLES).max(1);
    let frames = (0..frame_count)
        .map(|frame_index| {
            let source_sample_position = frame_index * HOP_SAMPLES;
            let f0_hz = pitch_at_sample(source_sample_position);
            PitchFrame {
                frame_index,
                source_sample_position,
                timestamp_seconds: source_sample_position as f32 / SAMPLE_RATE as f32,
                f0_hz,
                raw_f0_hz: f0_hz.unwrap_or(0.0),
                confidence: if f0_hz.is_some() { 0.95 } else { 0.1 },
                voiced: f0_hz.is_some(),
                rms: if f0_hz.is_some() { 0.25 } else { 0.02 },
            }
        })
        .collect();
    PitchContour {
        source_sample_rate: SAMPLE_RATE,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames,
    }
}

fn tone(frequency_hz: f32, len: usize, amplitude: f32, attack_samples: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let ramp = if attack_samples == 0 {
                1.0
            } else {
                (index as f32 / attack_samples as f32).min(1.0)
            };
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / SAMPLE_RATE as f32;
            phase.sin() * amplitude * ramp
        })
        .collect()
}

fn noise(len: usize, amplitude: f32) -> Vec<f32> {
    let mut state = 0x4d595df4_u32;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let normalized = state as f32 / u32::MAX as f32;
            (normalized * 2.0 - 1.0) * amplitude
        })
        .collect()
}

fn assert_note_count(result: &AnalysisResult, range: RangeInclusive<usize>) {
    assert!(
        range.contains(&result.detected_notes.len()),
        "expected note count in {range:?}, got {}: {:?}",
        result.detected_notes.len(),
        note_summaries(result)
    );
}

fn note_summaries(result: &AnalysisResult) -> Vec<(usize, usize, u16, u16)> {
    result
        .detected_notes
        .iter()
        .map(|note| {
            (
                note.start_sample,
                note.end_sample,
                note.pitch_hz.round() as u16,
                (note.peak_rms * 1_000.0).round() as u16,
            )
        })
        .collect()
}

fn assert_has_onset_near(result: &AnalysisResult, expected: usize, tolerance: usize) {
    assert!(
        result
            .detected_notes
            .iter()
            .skip(1)
            .any(|note| note.start_sample.abs_diff(expected) <= tolerance),
        "expected onset near {expected}, got {:?}",
        result
            .detected_notes
            .iter()
            .map(|note| note.start_sample)
            .collect::<Vec<_>>()
    );
}

fn assert_close_cents(actual_hz: f32, expected_hz: f32, tolerance_cents: f32) {
    let cents = 1200.0 * (actual_hz / expected_hz).log2().abs();
    assert!(
        cents <= tolerance_cents,
        "expected {actual_hz} Hz within {tolerance_cents} cents of {expected_hz} Hz, got {cents}"
    );
}

fn assert_analysis_is_finite(result: &AnalysisResult) {
    assert!(result.pitch_contour.frames.iter().all(|frame| {
        frame.raw_f0_hz.is_finite() && frame.confidence.is_finite() && frame.rms.is_finite()
    }));
    assert!(result.detected_notes.iter().all(|note| {
        note.pitch_hz.is_finite() && note.peak_rms.is_finite() && note.mean_rms.is_finite()
    }));
    assert!(
        result
            .midi_clip
            .notes
            .iter()
            .all(|note| note.duration_ticks > 0)
    );
}

fn samples_for_ms(ms: usize) -> usize {
    (SAMPLE_RATE as usize * ms) / 1_000
}
