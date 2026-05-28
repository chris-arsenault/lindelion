use lindelion_dsp_utils::analysis::estimate_frequency_zero_crossings;

use super::{RuntimeFixture, note_on, peak_abs};
use crate::{PitchOffset, TriggerMode, patch::PitchShiftAlgorithm};

#[test]
fn resample_stretch_pitch_shift_does_not_allocate() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.patch.slices[0].pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    fixture.prepare_current_patch();
    let events = [note_on(0, 36, 1.0)];
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];

    fixture.process_no_alloc(
        "linnod prepared resample-stretch pitch shift",
        &events,
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.prepared_resample_pro_variant_count(), 1);
    assert!(peak_abs(&left) > 0.000_01);
}

#[test]
fn resample_stretch_uses_prepared_buffer_without_note_path_render() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.patch.slices[0].pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    fixture.prepare_current_patch();
    let render_count = fixture.processor.prepared_resample_pro_render_count();
    let mut left = [0.0; 512];
    let mut right = [0.0; 512];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.prepared_resample_pro_variant_count(), 1);
    assert_eq!(
        fixture.processor.prepared_resample_pro_render_count(),
        render_count
    );
    assert!(peak_abs(&left) > 0.000_01);
}

#[test]
fn resample_stretch_slice_pitch_change_regenerates_prepared_buffer() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.patch.slices[0].pitch = PitchOffset {
        semitones: 0,
        cents: 1.0,
    };
    fixture.prepare_current_patch();
    assert_eq!(fixture.processor.prepared_resample_pro_variant_count(), 1);

    fixture.patch.slices[0].pitch = PitchOffset::from_frequency_ratio(2.0);
    fixture.prepare_current_patch();
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

    assert_eq!(fixture.processor.prepared_resample_pro_variant_count(), 1);
    assert_eq!(fixture.processor.prepared_resample_pro_render_count(), 1);
    assert!(
        (estimated_hz - 440.0).abs() < 3.0,
        "regenerated prepared buffer should use updated slice pitch; estimated_hz={estimated_hz}"
    );
}

#[test]
fn resample_stretch_missing_prepared_shift_is_silent_instead_of_unshifted_fallback() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.trigger_mode = TriggerMode::Chromatic;
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.prepare_current_patch();
    let mut left = [0.0; 512];
    let mut right = [0.0; 512];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 48, 1.0)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.prepared_resample_pro_variant_count(), 0);
    assert_eq!(peak_abs(&left), 0.0);
}
