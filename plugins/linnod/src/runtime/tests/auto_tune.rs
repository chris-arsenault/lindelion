use super::*;

#[test]
fn pad_mode_auto_tune_applies_nearest_chromatic_whole_cent_shift() {
    let mut fixture = fixture_with_frequency(443.0, 443.0);
    fixture.patch.auto_tune.enabled = true;
    let trigger =
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 36, 48_000.0, 1.0).unwrap();

    let detected_f0_hz = fixture
        .analysis
        .pitch_shift_cache
        .slice_summary(trigger.slice_index)
        .and_then(|summary| summary.detected_f0_hz)
        .unwrap();
    assert!((detected_f0_hz * trigger.ratios.pitch_ratio - 439.94).abs() < 0.01);

    let mut left = [0.0; 4_096];
    let mut right = [0.0; 4_096];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    let estimated_hz = estimate_frequency_zero_crossings(&left[512..], 48_000.0).unwrap();
    assert!(
        (estimated_hz - 440.0).abs() < 3.0,
        "expected chromatic auto-tuned output near 440 Hz, got {estimated_hz:.2} Hz"
    );
}

#[test]
fn slice_auto_tune_override_supersedes_global_config() {
    let mut fixture = fixture_with_frequency(443.0, 443.0);
    fixture.patch.auto_tune.enabled = true;
    fixture.patch.slices[0].use_auto_tune_override = true;
    fixture.patch.slices[0].auto_tune_enabled = false;

    let trigger =
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 36, 48_000.0, 1.0).unwrap();

    assert!((trigger.ratios.pitch_ratio - 1.0).abs() < 0.000_01);
}

#[test]
fn auto_tune_note_trigger_does_not_allocate() {
    let mut fixture = fixture_with_frequency(443.0, 443.0);
    fixture.patch.auto_tune.enabled = true;
    let events = [note_on(0, 36, 1.0)];
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];

    fixture.process_no_alloc(
        "linnod auto-tune note trigger",
        &events,
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 1);
}

fn fixture_with_frequency(frequency_hz: f32, f0_hz: f32) -> RuntimeFixture {
    let sample_rate = 48_000;
    let samples = sine_wave(frequency_hz, sample_rate, 4_800);
    let analysis = source_analysis_from_samples(
        samples,
        sample_rate,
        vec![SliceMarker {
            position_samples: 0,
            kind: MarkerKind::Auto,
        }],
        f0_hz,
        "tuned_source.wav",
    );
    let patch = LinnodPatch::default();
    let mut processor = LinnodProcessor::new(48_000.0);
    processor.prepare_source_analysis(&patch, &analysis);
    RuntimeFixture {
        processor,
        patch,
        analysis,
    }
}
