use lindelion_plugin_shell::vst3::{PluginMessage, PluginMessageDecodeError, PluginMessageType};
use vst3::Steinberg::Vst::{IConnectionPointTrait, IMessage};
use vst3::Steinberg::*;

use super::{
    LinnodMessageKind, LinnodPluginMessage, LinnodStatusPayload, LinnodVst3Controller,
    LinnodVst3Processor,
    controller::{normalized_parameter_value, parameter_index},
    messages::{
        LinnodDetectionEditMessage, LinnodMarkerEditMessage, LinnodPadEditMessage,
        LinnodPlaybackEditMessage, LinnodSliceEditMessage, LinnodSourceSlicePayload,
        LinnodSourceSummaryPayload, LinnodTelemetryPayload, LinnodWaveformPointPayload,
    },
};
use crate::{
    ChokeGroupId, LinnodPatch, PadId, SourceAnalysisStatus, parameters::MASTER_GAIN_PARAMETER_ID,
    patch_io,
};

#[test]
fn plugin_messages_roundtrip_typed_payloads() {
    assert_message_roundtrip(LinnodPluginMessage::patch_update(b"patch".to_vec()));
    assert_message_roundtrip(LinnodPluginMessage::SourceLoadRequest);
    assert_message_roundtrip(LinnodPluginMessage::source_ingest_request(
        b"/tmp/source.wav".to_vec(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::MarkerEdit(
        LinnodMarkerEditMessage::AddUser {
            position_samples: 128,
        }
        .encode(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::SliceEdit(
        LinnodSliceEditMessage::GainDb {
            slice_index: 2,
            gain_db: -3.0,
        }
        .encode(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::PadEdit(
        LinnodPadEditMessage::ChokeGroup {
            pad: PadId(3),
            group: Some(ChokeGroupId(2)),
        }
        .encode(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::PlaybackEdit(
        LinnodPlaybackEditMessage::Mode(crate::PlaybackMode::Continue).encode(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::DetectionEdit(
        LinnodDetectionEditMessage::Algorithm(
            lindelion_onset_detect::DetectionAlgorithm::ComplexFlux,
        )
        .encode(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::StatusResponse(status_payload()));
    assert_message_roundtrip(LinnodPluginMessage::SourceSummaryRequest);
    assert_message_roundtrip(LinnodPluginMessage::SourceSummaryResponse(
        source_summary_payload().encode().unwrap(),
    ));
    assert_message_roundtrip(LinnodPluginMessage::TelemetryResponse(
        LinnodTelemetryPayload {
            left_peak: 0.25,
            right_peak: 0.5,
            active_voices: 2.0,
        }
        .encode(),
    ));
}

#[test]
fn malformed_empty_message_payloads_do_not_panic() {
    let processor = LinnodVst3Processor::new();
    let message = PluginMessage::with_payload(
        LinnodMessageKind::StatusRequest.id(),
        b"unexpected".to_vec(),
    )
    .to_com_ptr::<IMessage>()
    .unwrap();

    let decoded = unsafe { LinnodPluginMessage::decode(message.as_ptr()) };
    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(decoded, Err(PluginMessageDecodeError::MalformedPayload));
    assert_eq!(result, kResultFalse);
}

#[test]
fn unknown_message_ids_are_ignored_safely() {
    let processor = LinnodVst3Processor::new();
    let message = PluginMessage::with_payload("lindelion.linnod.future", Vec::new())
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { LinnodPluginMessage::decode(message.as_ptr()) };
    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(decoded, Ok(None));
    assert_eq!(result, kNotImplemented);
}

#[test]
fn controller_applies_source_summary_and_preserves_it_across_patch_updates() {
    let controller = LinnodVst3Controller::new();
    let mut patch = LinnodPatch {
        source_sample: Some(lindelion_sample_library::SampleReference::new(
            "hash",
            "Samples/source.wav",
        )),
        ..LinnodPatch::default()
    };
    patch.markers = vec![lindelion_onset_detect::SliceMarker {
        position_samples: 0,
        kind: lindelion_onset_detect::MarkerKind::Auto,
    }];
    notify_controller(
        &controller,
        LinnodPluginMessage::patch_update(patch_payload(&patch)),
    );
    notify_controller(
        &controller,
        LinnodPluginMessage::SourceSummaryResponse(source_summary_payload().encode().unwrap()),
    );

    assert_eq!(controller.summary.borrow().source_label, "source.wav");
    assert_eq!(controller.summary.borrow().waveform.len(), 2);
    assert_eq!(controller.summary.borrow().slices[0].end_sample, 4_800);
    assert_eq!(
        controller.summary.borrow().slices[0].detected_midi_note,
        Some(57.0)
    );
    assert_eq!(
        controller.summary.borrow().slices[0].nearest_midi_note,
        Some(57)
    );
    assert_eq!(
        controller.summary.borrow().slices[0].root_target_f0_hz,
        Some(220.0)
    );

    patch.trigger_mode = crate::patch::TriggerMode::Chromatic;
    notify_controller(
        &controller,
        LinnodPluginMessage::patch_update(patch_payload(&patch)),
    );

    assert_eq!(controller.summary.borrow().source_label, "source.wav");
    assert_eq!(controller.summary.borrow().waveform.len(), 2);
    assert_eq!(
        controller.summary.borrow().trigger_mode,
        lindelion_ui::linnod_vizia::LinnodEditorTriggerMode::Chromatic
    );
}

#[test]
fn controller_clears_source_summary_when_analysis_inputs_change() {
    let controller = LinnodVst3Controller::new();
    let mut patch = LinnodPatch {
        source_sample: Some(lindelion_sample_library::SampleReference::new(
            "hash",
            "Samples/source.wav",
        )),
        ..LinnodPatch::default()
    };
    notify_controller(
        &controller,
        LinnodPluginMessage::patch_update(patch_payload(&patch)),
    );
    notify_controller(
        &controller,
        LinnodPluginMessage::SourceSummaryResponse(source_summary_payload().encode().unwrap()),
    );
    patch.detection.min_slice_ms += 25.0;

    notify_controller(
        &controller,
        LinnodPluginMessage::patch_update(patch_payload(&patch)),
    );

    assert_eq!(controller.summary.borrow().source_label, "source.wav");
    assert!(controller.summary.borrow().waveform.is_empty());
    assert_eq!(controller.summary.borrow().slices[0].end_sample, 0);
}

#[test]
fn controller_preserves_source_summary_when_markers_change() {
    let controller = LinnodVst3Controller::new();
    let mut patch = LinnodPatch {
        source_sample: Some(lindelion_sample_library::SampleReference::new(
            "hash",
            "Samples/source.wav",
        )),
        ..LinnodPatch::default()
    };
    notify_controller(
        &controller,
        LinnodPluginMessage::patch_update(patch_payload(&patch)),
    );
    notify_controller(
        &controller,
        LinnodPluginMessage::SourceSummaryResponse(source_summary_payload().encode().unwrap()),
    );
    patch.markers.push(lindelion_onset_detect::SliceMarker {
        position_samples: 2_400,
        kind: lindelion_onset_detect::MarkerKind::User,
    });

    notify_controller(
        &controller,
        LinnodPluginMessage::patch_update(patch_payload(&patch)),
    );

    assert_eq!(controller.summary.borrow().source_label, "source.wav");
    assert_eq!(controller.summary.borrow().waveform.len(), 2);
    assert!(controller.summary.borrow().markers.iter().any(|marker| {
        marker.position_samples == 2_400
            && marker.kind == lindelion_ui::linnod_vizia::LinnodEditorMarkerKind::User
    }));
}

#[test]
fn controller_patch_mirror_tracks_parameter_edits() {
    let controller = LinnodVst3Controller::new();
    let normalized = normalized_parameter_value(MASTER_GAIN_PARAMETER_ID, -12.0);

    assert_eq!(
        controller.set_value(MASTER_GAIN_PARAMETER_ID, normalized),
        kResultOk
    );

    assert_eq!(controller.patch.borrow().output.master_gain_db, -12.0);
    assert_eq!(
        controller
            .values
            .value(parameter_index(MASTER_GAIN_PARAMETER_ID).unwrap())
            .unwrap(),
        normalized
    );
}

#[test]
fn controller_applies_slice_edit_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();
    let edit = LinnodSliceEditMessage::Reverse {
        slice_index: 0,
        reverse: true,
    };

    assert_eq!(controller.apply_slice_edit(edit), kResultFalse);

    assert!(controller.patch.borrow().slices[0].reverse);
}

#[test]
fn controller_applies_pad_edit_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();
    let edit = LinnodPadEditMessage::ChokeGroup {
        pad: PadId(3),
        group: Some(ChokeGroupId(2)),
    };

    assert_eq!(controller.apply_pad_edit(edit), kResultFalse);

    assert_eq!(
        controller.patch.borrow().pad_map[2].choke_group,
        Some(ChokeGroupId(2))
    );
    assert_eq!(
        controller.summary.borrow().pads[2].choke_group,
        Some(ChokeGroupId(2).0)
    );
}

#[test]
fn controller_applies_playback_edit_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();

    assert_eq!(
        controller.apply_playback_edit(LinnodPlaybackEditMessage::Mode(
            crate::PlaybackMode::Continue
        )),
        kResultFalse
    );
    assert_eq!(
        controller.apply_playback_edit(LinnodPlaybackEditMessage::Envelope(
            crate::EnvelopeConfig {
                attack_ms: 10.0,
                decay_ms: 20.0,
                sustain: 0.5,
                release_ms: 30.0,
            }
        )),
        kResultFalse
    );

    assert_eq!(
        controller.patch.borrow().playback.mode,
        crate::PlaybackMode::Continue
    );
    assert_eq!(controller.patch.borrow().playback.envelope.sustain, 0.5);
    assert_eq!(
        controller.summary.borrow().playback.mode,
        lindelion_ui::linnod_vizia::LinnodEditorPlaybackMode::Continue
    );
}

#[test]
fn controller_applies_marker_edit_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();
    controller.patch.borrow_mut().markers = vec![
        lindelion_onset_detect::SliceMarker {
            position_samples: 0,
            kind: lindelion_onset_detect::MarkerKind::Auto,
        },
        lindelion_onset_detect::SliceMarker {
            position_samples: 120,
            kind: lindelion_onset_detect::MarkerKind::Auto,
        },
        lindelion_onset_detect::SliceMarker {
            position_samples: 240,
            kind: lindelion_onset_detect::MarkerKind::User,
        },
    ];

    assert_eq!(
        controller.apply_marker_edit(LinnodMarkerEditMessage::RemoveAt {
            position_samples: 120
        }),
        kResultFalse
    );
    assert_eq!(
        controller.apply_marker_edit(LinnodMarkerEditMessage::AddUser {
            position_samples: 360
        }),
        kResultFalse
    );

    let patch = controller.patch.borrow();
    assert!(
        patch
            .markers
            .iter()
            .all(|marker| marker.position_samples != 120)
    );
    assert!(patch.markers.iter().any(|marker| {
        marker.position_samples == 360 && marker.kind == lindelion_onset_detect::MarkerKind::User
    }));
    assert_eq!(
        controller.summary.borrow().markers.len(),
        patch.markers.len()
    );
}

#[test]
fn controller_applies_detection_edit_through_typed_message_surface() {
    let controller = LinnodVst3Controller::new();

    assert_eq!(
        controller.apply_detection_edit(LinnodDetectionEditMessage::Algorithm(
            lindelion_onset_detect::DetectionAlgorithm::ComplexFlux,
        )),
        kResultFalse
    );
    assert_eq!(
        controller.apply_detection_edit(LinnodDetectionEditMessage::GroupDelayWeight(2.25)),
        kResultFalse
    );

    let patch = controller.patch.borrow();
    assert_eq!(
        patch.detection.algorithm,
        lindelion_onset_detect::DetectionAlgorithm::ComplexFlux
    );
    assert_eq!(
        patch.detection.params,
        lindelion_onset_detect::AlgorithmParams::ComplexFlux {
            lookback_frames: 3,
            group_delay_weight: 2.25,
        }
    );
    assert_eq!(
        controller.summary.borrow().detection.algorithm,
        lindelion_ui::linnod_vizia::LinnodEditorDetectionAlgorithm::ComplexFlux
    );
}

#[test]
fn processor_notify_applies_patch_payload() {
    let processor = LinnodVst3Processor::new();
    let patch = LinnodPatch {
        name: "Bridge Patch".to_string(),
        ..LinnodPatch::default()
    };
    let payload = patch_io::to_toml_string(&patch).unwrap().into_bytes();
    let message = LinnodPluginMessage::patch_update(payload)
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert_eq!(processor.plugin.borrow().patch().name, "Bridge Patch");
}

#[test]
fn processor_notify_applies_detection_edit_payload() {
    let processor = LinnodVst3Processor::new();
    let message = LinnodPluginMessage::DetectionEdit(
        LinnodDetectionEditMessage::ManualGridDivisions(12).encode(),
    )
    .into_com_message()
    .to_com_ptr::<IMessage>()
    .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert_eq!(
        processor.plugin.borrow().patch().detection.algorithm,
        lindelion_onset_detect::DetectionAlgorithm::ManualGrid
    );
    assert_eq!(
        processor.plugin.borrow().patch().detection.params,
        lindelion_onset_detect::AlgorithmParams::ManualGrid {
            divisions: 12,
            offset_ms: 0.0,
        }
    );
}

#[test]
fn processor_notify_applies_pad_edit_payload() {
    let processor = LinnodVst3Processor::new();
    let message = LinnodPluginMessage::PadEdit(
        LinnodPadEditMessage::ChokeGroup {
            pad: PadId(3),
            group: Some(ChokeGroupId(2)),
        }
        .encode(),
    )
    .into_com_message()
    .to_com_ptr::<IMessage>()
    .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert_eq!(
        processor.plugin.borrow().patch().pad_map[2].choke_group,
        Some(ChokeGroupId(2))
    );
}

#[test]
fn processor_notify_applies_playback_edit_payload() {
    let processor = LinnodVst3Processor::new();
    let message = LinnodPluginMessage::PlaybackEdit(
        LinnodPlaybackEditMessage::Mode(crate::PlaybackMode::Continue).encode(),
    )
    .into_com_message()
    .to_com_ptr::<IMessage>()
    .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert_eq!(
        processor.plugin.borrow().patch().playback.mode,
        crate::PlaybackMode::Continue
    );
}

#[test]
fn processor_notify_applies_marker_edit_payload() {
    let processor = LinnodVst3Processor::new();
    let patch = LinnodPatch {
        markers: vec![
            lindelion_onset_detect::SliceMarker {
                position_samples: 0,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
            lindelion_onset_detect::SliceMarker {
                position_samples: 120,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
        ],
        ..LinnodPatch::default()
    };
    processor.plugin.borrow_mut().set_patch(patch);
    let message = LinnodPluginMessage::MarkerEdit(
        LinnodMarkerEditMessage::RemoveAt {
            position_samples: 120,
        }
        .encode(),
    )
    .into_com_message()
    .to_com_ptr::<IMessage>()
    .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert!(
        processor
            .plugin
            .borrow()
            .patch()
            .markers
            .iter()
            .all(|marker| marker.position_samples != 120)
    );
}

fn assert_message_roundtrip(message: LinnodPluginMessage) {
    let encoded = message
        .clone()
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();
    let decoded = unsafe { LinnodPluginMessage::decode(encoded.as_ptr()) };

    assert_eq!(decoded, Ok(Some(message)));
}

fn notify_controller(controller: &LinnodVst3Controller, message: LinnodPluginMessage) {
    let message = message.into_com_message().to_com_ptr::<IMessage>().unwrap();

    assert_eq!(unsafe { controller.notify(message.as_ptr()) }, kResultOk);
}

fn patch_payload(patch: &LinnodPatch) -> Vec<u8> {
    patch_io::to_toml_string(patch).unwrap().into_bytes()
}

fn status_payload() -> LinnodStatusPayload {
    LinnodStatusPayload {
        source_status: SourceAnalysisStatus::Ready,
        has_source: true,
        has_analysis: true,
        marker_count: 3,
        selected_slice_index: Some(1),
        active_voices: 2,
    }
}

fn source_summary_payload() -> LinnodSourceSummaryPayload {
    LinnodSourceSummaryPayload {
        source_label: "source.wav".to_string(),
        source_sample_rate: 48_000,
        waveform: vec![
            LinnodWaveformPointPayload {
                min: -0.5,
                max: 0.1,
                rms: 0.25,
            },
            LinnodWaveformPointPayload {
                min: -0.2,
                max: 0.75,
                rms: 0.35,
            },
        ],
        slices: vec![LinnodSourceSlicePayload {
            index: 0,
            start_sample: 0,
            end_sample: 4_800,
            detected_f0_hz: Some(220.0),
            detected_midi_note: Some(57.0),
            nearest_midi_note: Some(57),
            nearest_scale_midi_note: Some(57),
            nearest_midi_note_hz: Some(220.0),
            nearest_scale_midi_note_hz: Some(220.0),
            cents_deviation: Some(0.0),
            root_target_f0_hz: Some(220.0),
        }],
    }
}
