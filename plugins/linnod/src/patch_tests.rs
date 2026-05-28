use super::*;
use lindelion_midi::Scale;
use lindelion_onset_detect::{AlgorithmParams, DEFAULT_MANUAL_GRID_OFFSET_MS, DetectionAlgorithm};

#[test]
fn default_patch_has_sixteen_slices_and_pad_assignments() {
    let patch = LinnodPatch::default();

    assert_eq!(patch.slices.len(), SLICE_COUNT);
    assert_eq!(patch.pad_map.len(), SLICE_COUNT);
    assert_eq!(patch.pad_map[0].midi_note, 36);
    assert_eq!(patch.pad_map[15].midi_note, 51);
    assert_eq!(patch.playback.mode, PlaybackMode::OneShot);
    assert_eq!(
        patch.engine.pitch_shift_algorithm,
        PitchShiftAlgorithm::SpectralPeak
    );
    assert!(!patch.slices[0].use_playback_override);
}

#[test]
fn patch_uses_shared_midi_tuning_types() {
    let mut patch = LinnodPatch::default();
    patch.tuning.scale = Scale::Dorian;
    patch.tuning.root = RootNote::D;

    assert_eq!(patch.tuning.scale.intervals(), vec![0, 2, 3, 5, 7, 9, 10]);
    assert_eq!(patch.tuning.root.pitch_class(), 2);
}

#[test]
fn slice_edit_sanitizes_gain_and_envelope_state() {
    let mut patch = LinnodPatch::default();

    assert!(patch.apply_slice_edit(0, SliceEdit::GainDb(f32::INFINITY)));
    assert!(patch.apply_slice_edit(
        0,
        SliceEdit::Envelope(EnvelopeConfig {
            attack_ms: -1.0,
            decay_ms: 2.0,
            sustain: 2.0,
            release_ms: f32::NAN,
        }),
    ));

    let slice = patch.slice(0).unwrap();
    assert_eq!(slice.gain_db, 0.0);
    assert_eq!(slice.envelope.attack_ms, 0.0);
    assert_eq!(slice.envelope.decay_ms, 2.0);
    assert_eq!(slice.envelope.sustain, 1.0);
    assert_eq!(slice.envelope.release_ms, 0.0);
}

#[test]
fn slice_edit_updates_playback_override_and_rejects_invalid_index() {
    let mut patch = LinnodPatch::default();

    assert!(patch.apply_slice_edit(0, SliceEdit::PlaybackOverride(true)));
    assert!(!patch.apply_slice_edit(SLICE_COUNT, SliceEdit::Reverse(true)));

    let slice = patch.slice(0).unwrap();
    assert!(slice.use_playback_override);
}

#[test]
fn playback_edit_and_effective_config_use_global_then_slice_override() {
    let mut patch = LinnodPatch::default();

    assert!(patch.apply_playback_edit(PlaybackEdit::Mode(PlaybackMode::Continue)));
    assert!(
        patch.apply_playback_edit(PlaybackEdit::Envelope(EnvelopeConfig {
            attack_ms: 5.0,
            decay_ms: 10.0,
            sustain: 0.5,
            release_ms: 120.0,
        }))
    );

    assert_eq!(
        patch.effective_playback_config(0),
        PlaybackConfig {
            mode: PlaybackMode::Continue,
            envelope: EnvelopeConfig {
                attack_ms: 5.0,
                decay_ms: 10.0,
                sustain: 0.5,
                release_ms: 120.0,
            },
        }
    );

    patch.apply_slice_edit(0, SliceEdit::PlaybackOverride(true));
    patch.apply_slice_edit(0, SliceEdit::PlaybackMode(PlaybackMode::Looped));
    patch.apply_slice_edit(
        0,
        SliceEdit::Envelope(EnvelopeConfig {
            attack_ms: 1.0,
            decay_ms: 2.0,
            sustain: 0.75,
            release_ms: 3.0,
        }),
    );

    assert_eq!(
        patch.effective_playback_config(0).mode,
        PlaybackMode::Looped
    );
    assert_eq!(patch.effective_playback_config(0).envelope.attack_ms, 1.0);
}

#[test]
fn engine_edit_sets_pitch_shift_algorithm() {
    let mut patch = LinnodPatch::default();

    assert!(patch.apply_engine_edit(EngineEdit::PitchShiftAlgorithm(
        PitchShiftAlgorithm::TimeStretch,
    )));

    assert_eq!(
        patch.engine.pitch_shift_algorithm,
        PitchShiftAlgorithm::TimeStretch
    );

    assert!(patch.apply_engine_edit(EngineEdit::PitchShiftAlgorithm(
        PitchShiftAlgorithm::ResampleStretch,
    )));

    assert_eq!(
        patch.engine.pitch_shift_algorithm,
        PitchShiftAlgorithm::ResampleStretch
    );
}

#[test]
fn detection_edit_mutates_persistent_detection_config() {
    let mut patch = LinnodPatch::default();

    assert!(patch.apply_detection_edit(DetectionEdit::Algorithm(DetectionAlgorithm::ComplexFlux)));
    assert!(patch.apply_detection_edit(DetectionEdit::GroupDelayWeight(2.5)));
    assert!(patch.apply_detection_edit(DetectionEdit::LookbackFrames(6)));
    assert!(patch.apply_detection_edit(DetectionEdit::MinSliceMs(f32::NAN)));

    assert_eq!(patch.detection.algorithm, DetectionAlgorithm::ComplexFlux);
    assert_eq!(
        patch.detection.params,
        AlgorithmParams::ComplexFlux {
            lookback_frames: 6,
            group_delay_weight: 2.5,
        }
    );
    assert_eq!(
        patch.detection.min_slice_ms,
        lindelion_onset_detect::DEFAULT_MIN_SLICE_MS
    );

    assert!(patch.apply_detection_edit(DetectionEdit::ManualGridDivisions(99)));
    assert_eq!(
        patch.detection.params,
        AlgorithmParams::ManualGrid {
            divisions: SLICE_COUNT,
            offset_ms: DEFAULT_MANUAL_GRID_OFFSET_MS,
        }
    );
}

#[test]
fn normalize_layout_clamps_loaded_patch_shape() {
    let mut patch = LinnodPatch {
        slices: vec![SliceParams::default_for_index(1)],
        active_chromatic_pad: PadId(99),
        pad_map: vec![PadAssignment {
            pad: PadId(99),
            slice_index: 999,
            midi_note: 10,
            choke_group: Some(ChokeGroupId(99)),
        }],
        ..LinnodPatch::default()
    };

    patch.normalize_layout();

    assert_eq!(patch.slices.len(), SLICE_COUNT);
    assert_eq!(patch.active_chromatic_pad, PadId(16));
    assert_eq!(patch.pad_map[15].slice_index, 15);
    assert_eq!(patch.pad_map[15].midi_note, 10);
    assert_eq!(patch.pad_map[15].choke_group, Some(ChokeGroupId(16)));
}

#[test]
fn pad_assignment_helpers_validate_and_resolve_selected_slice() {
    let mut patch = LinnodPatch {
        active_chromatic_pad: PadId(2),
        pad_map: vec![
            PadAssignment {
                pad: PadId(2),
                slice_index: 7,
                midi_note: 64,
                choke_group: Some(ChokeGroupId(4)),
            },
            PadAssignment {
                pad: PadId(99),
                slice_index: 99,
                midi_note: 12,
                choke_group: None,
            },
        ],
        ..LinnodPatch::default()
    };

    patch.normalize_layout();

    assert_eq!(patch.selected_slice_index(), Some(7));
    assert_eq!(patch.pad_map[15].slice_index, 15);
    assert_eq!(slice_index_for_pad(&[], PadId(3)), Some(2));
    assert_eq!(
        pad_assignment_for_note(&patch.pad_map, 64)
            .unwrap()
            .choke_group,
        Some(ChokeGroupId(4))
    );
}

#[test]
fn pad_edit_updates_persistent_choke_group() {
    let mut patch = LinnodPatch::default();

    assert!(patch.apply_pad_edit(PadId(3), PadEdit::ChokeGroup(Some(ChokeGroupId(2))),));
    assert!(patch.apply_pad_edit(PadId(4), PadEdit::ChokeGroup(Some(ChokeGroupId(99))),));
    assert!(patch.apply_pad_edit(PadId(4), PadEdit::ChokeGroup(None)));

    assert_eq!(patch.pad_map[2].choke_group, Some(ChokeGroupId(2)));
    assert_eq!(patch.pad_map[3].choke_group, None);
}

#[test]
fn slice_region_helpers_use_shared_marker_domain() {
    let patch = LinnodPatch {
        markers: vec![
            SliceMarker {
                position_samples: 2_000,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 500,
                kind: lindelion_onset_detect::MarkerKind::User,
            },
        ],
        ..LinnodPatch::default()
    };

    let selected = patch.slice_region(1, 3_000).unwrap();

    assert_eq!(selected.start_sample, 500);
    assert_eq!(selected.duration_samples(), 1_500);
    assert_eq!(patch.slice_index_at_source_sample(3_000, 2_500), Some(2));
}
