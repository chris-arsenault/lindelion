use super::*;
use crate::PitchMappedRegion;

#[test]
fn chromatic_mode_resolves_selected_pad_and_pitch_delta() {
    let mut patch = LinnodPatch {
        trigger_mode: TriggerMode::Chromatic,
        active_chromatic_pad: PadId(2),
        pad_map: vec![PadAssignment {
            pad: PadId(2),
            slice_index: 7,
            midi_note: 64,
            choke_group: None,
        }],
        ..LinnodPatch::default()
    };
    patch.normalize_layout();

    let resolved = resolve_note_trigger(&patch, 76).unwrap();

    assert_eq!(resolved.slice_index, 7);
    assert_eq!(resolved.chromatic_semitones, 12.0);
}

#[test]
fn mode_routes_only_the_detected_key_without_pitch_coercion() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.trigger_mode = TriggerMode::PitchMap;
    fixture.patch.slices[0].pitch = PitchOffset {
        semitones: 12,
        cents: 50.0,
    };
    fixture.analysis.pitch_map = vec![mapped_a3(1_200, 3_600, 20.0)];

    let trigger =
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 57, 48_000.0, 1.0).unwrap();

    assert_eq!(trigger.source_start_sample, 1_200);
    assert_eq!(trigger.source_end_sample, 3_600);
    assert_eq!(trigger.ratios, PitchShiftRatios::identity());
    assert!(
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 58, 48_000.0, 1.0).is_none()
    );
}

#[test]
fn auto_tune_corrects_the_detected_deviation_to_the_mapped_key() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.trigger_mode = TriggerMode::PitchMap;
    fixture.patch.auto_tune.enabled = true;
    fixture.analysis.pitch_map = vec![mapped_a3(1_200, 3_600, 20.0)];

    let trigger =
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 57, 48_000.0, 1.0).unwrap();

    assert!(
        (fixture.analysis.pitch_map[0].detected_f0_hz * trigger.ratios.pitch_ratio - 220.0).abs()
            < 0.01
    );
    assert!((trigger.ratios.pitch_ratio - 2.0_f32.powf(-20.0 / 1_200.0)).abs() < 0.000_01);
}

#[test]
fn auto_tune_does_not_correct_outside_the_mapping_tolerance() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.trigger_mode = TriggerMode::PitchMap;
    fixture.patch.auto_tune.enabled = true;
    fixture.analysis.pitch_map = vec![mapped_a3(0, 4_800, 26.0)];

    let trigger =
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 57, 48_000.0, 1.0).unwrap();

    assert_eq!(trigger.ratios, PitchShiftRatios::identity());
}

#[test]
fn corrected_note_trigger_does_not_allocate() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.trigger_mode = TriggerMode::PitchMap;
    fixture.patch.auto_tune.enabled = true;
    fixture.analysis.pitch_map = vec![mapped_a3(0, 4_800, 20.0)];
    fixture.prepare_current_patch();
    let events = [note_on(0, 57, 1.0)];
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];

    fixture.process_no_alloc(
        "linnod pitch-mapped note trigger",
        &events,
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 1);
    assert!(peak_abs(&left) > 0.000_01);
}

#[test]
fn resample_stretch_prepares_the_corrected_region_off_thread() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.trigger_mode = TriggerMode::PitchMap;
    fixture.patch.auto_tune.enabled = true;
    fixture.patch.engine.pitch_shift_algorithm = PitchShiftAlgorithm::ResampleStretch;
    fixture.analysis.pitch_map = vec![mapped_a3(0, 4_800, -20.0)];

    fixture.prepare_current_patch();

    assert_eq!(fixture.processor.prepared_resample_pro_variant_count(), 1);
    assert_eq!(fixture.processor.prepared_resample_pro_render_count(), 1);
}

fn mapped_a3(start_sample: usize, end_sample: usize, cents_deviation: f32) -> PitchMappedRegion {
    PitchMappedRegion {
        midi_note: 57,
        start_sample,
        end_sample,
        detected_f0_hz: 220.0 * 2.0_f32.powf(cents_deviation / 1_200.0),
        cents_deviation,
        mean_confidence: 0.95,
    }
}
