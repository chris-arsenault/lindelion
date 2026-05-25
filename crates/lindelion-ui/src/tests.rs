use super::*;

#[test]
fn command_state_tracks_selected_excitation_slot() {
    let mut state = UiCommandState::default();
    let slot = PadId::new(3).unwrap();

    let dispatch = state.dispatch(UiCommand::LoadExcitationSlot(slot));

    assert_eq!(state.selected_slot, Some(slot));
    assert_eq!(
        state.last_command,
        Some(UiCommand::LoadExcitationSlot(slot))
    );
    assert_eq!(dispatch.command, UiCommand::LoadExcitationSlot(slot));
    assert_eq!(dispatch.selected_slot, Some(slot));
    assert_eq!(command_label(state.last_command), "Slot load requested");
}

#[test]
fn selected_slot_commands_resolve_to_current_slot() {
    let mut state = UiCommandState::default();
    let slot = PadId::new(4).unwrap();

    state.dispatch(UiCommand::SelectExcitationSlot(slot));
    state.dispatch(UiCommand::ClearSelectedExcitationSlot);

    assert_eq!(state.selected_slot, Some(slot));
    assert_eq!(
        state.last_command,
        Some(UiCommand::ClearExcitationSlot(slot))
    );
}

#[test]
fn command_bus_resolves_selected_slot_commands() {
    let mut bus = EditorCommandBus::default();
    let slot = PadId::new(12).unwrap();

    bus.dispatch(UiCommand::SelectExcitationSlot(slot));
    let dispatch = bus.dispatch(UiCommand::LoadSelectedExcitationSlot);

    assert_eq!(bus.selected_slot(), Some(slot));
    assert_eq!(
        bus.last_command(),
        Some(UiCommand::LoadExcitationSlot(slot))
    );
    assert_eq!(dispatch.command, UiCommand::LoadExcitationSlot(slot));
    assert_eq!(dispatch.selected_slot, Some(slot));
}

#[test]
fn command_float_adapter_round_trips_every_command() {
    for command in command_fixtures() {
        let payload = EditorCommandFloatAdapter::encode(Some(command));

        assert_eq!(
            EditorCommandFloatAdapter::decode(payload),
            Some(command),
            "payload {payload} should round-trip {command:?}"
        );
    }

    assert_eq!(
        EditorCommandFloatAdapter::decode(EditorCommandFloatAdapter::encode(None)),
        None
    );
}

#[test]
fn command_float_adapter_ignores_invalid_payloads() {
    for payload in [
        f32::NAN,
        f32::INFINITY,
        -1.0,
        1.5,
        44.0,
        100.0,
        117.0,
        200.0,
        217.0,
        300.0,
        317.0,
    ] {
        assert_eq!(EditorCommandFloatAdapter::decode(payload), None);
    }
}

#[test]
fn slot_command_payloads_preserve_slot_identity() {
    for index in 1..=16 {
        let slot = PadId::new(index).unwrap();
        for command in [
            UiCommand::SelectExcitationSlot(slot),
            UiCommand::LoadExcitationSlot(slot),
            UiCommand::ClearExcitationSlot(slot),
        ] {
            assert_eq!(
                EditorCommandFloatAdapter::decode(EditorCommandFloatAdapter::encode(Some(command))),
                Some(command)
            );
        }
    }
}

#[test]
fn waveform_points_from_samples_are_bounded_and_sanitized() {
    let points = waveform_points_from_samples(&[-0.5, f32::NAN, 0.25, 1.0], 2);

    assert_eq!(points.len(), 2);
    assert_eq!(points[0].min, -0.5);
    assert_eq!(points[0].max, 0.0);
    assert_eq!(points[1].min, 0.0);
    assert_eq!(points[1].max, 1.0);
    assert!(points.iter().all(|point| point.rms.is_finite()));
    assert!(waveform_points_from_samples(&[], 2).is_empty());
    assert!(waveform_points_from_samples(&[1.0], 0).is_empty());
}

#[test]
fn waveform_points_for_view_aggregates_visible_bins() {
    let points = vec![
        WaveformPoint {
            min: -0.1,
            max: 0.2,
            rms: 0.1,
        },
        WaveformPoint {
            min: -0.8,
            max: 0.4,
            rms: 0.4,
        },
        WaveformPoint {
            min: -0.2,
            max: 0.9,
            rms: 0.6,
        },
        WaveformPoint {
            min: -0.3,
            max: 0.3,
            rms: 0.2,
        },
    ];

    let visible = waveform_points_for_view(&points, 0.25, 0.75, 1);

    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].min, -0.8);
    assert_eq!(visible[0].max, 0.9);
    assert!(visible[0].rms > 0.4);
}

#[test]
fn command_handler_invokes_patch_io_services() {
    let mut patch_io = MockPatchIoService::default();
    let mut sample_slots = MockSampleSlotService::default();

    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::SavePatch,
            EditorCommandContext {
                patch_save_path: Some(Path::new("/patches/save.toml")),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::PatchSaved)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::LoadPatch,
            EditorCommandContext {
                patch_load_path: Some(Path::new("/patches/load.toml")),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::PatchLoaded)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::ExportPatchWithSamples,
            EditorCommandContext {
                patch_export_directory: Some(Path::new("/exports")),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::PatchExported)
    );

    assert_eq!(
        patch_io.calls,
        [
            "save:/patches/save.toml",
            "load:/patches/load.toml",
            "export:/exports"
        ]
    );
    assert!(sample_slots.calls.is_empty());
}

#[test]
fn command_handler_invokes_sample_slot_services() {
    let mut patch_io = MockPatchIoService::default();
    let mut sample_slots = MockSampleSlotService::default();
    let slot = PadId::new(3).unwrap();

    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::OpenLibrary,
            EditorCommandContext::default(),
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::LibraryRefreshed)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::OpenLibrary,
            EditorCommandContext {
                sample_path: Some(Path::new("/samples/kick.wav")),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::SampleIngested)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::LoadExcitationSlot(slot),
            EditorCommandContext {
                selected_library_sample: Some(7),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::SlotAssigned)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::LoadExcitationSlot(slot),
            EditorCommandContext {
                sample_path: Some(Path::new("/samples/snare.wav")),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::SlotAssigned)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::ClearExcitationSlot(slot),
            EditorCommandContext::default(),
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::SlotCleared)
    );

    assert!(patch_io.calls.is_empty());
    assert_eq!(
        sample_slots.calls,
        [
            "refresh",
            "ingest:/samples/kick.wav",
            "assign-library:7:3",
            "ingest:/samples/snare.wav",
            "assign-reference:/samples/snare.wav:3",
            "clear:3",
        ]
    );
}

#[test]
fn command_handler_falls_back_to_sample_path_when_selected_assignment_fails() {
    let mut patch_io = MockPatchIoService::default();
    let mut sample_slots = MockSampleSlotService {
        fail_library_assignment: true,
        ..MockSampleSlotService::default()
    };
    let slot = PadId::new(2).unwrap();

    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::LoadExcitationSlot(slot),
            EditorCommandContext {
                selected_library_sample: Some(99),
                sample_path: Some(Path::new("/samples/fallback.wav")),
                ..EditorCommandContext::default()
            },
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::SlotAssigned)
    );

    assert_eq!(
        sample_slots.calls,
        [
            "assign-library:99:2",
            "ingest:/samples/fallback.wav",
            "assign-reference:/samples/fallback.wav:2",
        ]
    );
}

#[test]
fn command_handler_ignores_missing_file_dialog_selection() {
    let mut patch_io = MockPatchIoService::default();
    let mut sample_slots = MockSampleSlotService::default();

    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::SavePatch,
            EditorCommandContext::default(),
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::Ignored)
    );
    assert_eq!(
        EditorCommandHandler::handle(
            UiCommand::LoadExcitationSlot(PadId::new(1).unwrap()),
            EditorCommandContext::default(),
            &mut patch_io,
            &mut sample_slots,
        ),
        Ok(EditorCommandOutcome::Ignored)
    );

    assert!(patch_io.calls.is_empty());
    assert!(sample_slots.calls.is_empty());
}

#[test]
fn telemetry_request_handler_invokes_service() {
    let mut telemetry = MockTelemetryService::default();

    assert_eq!(
        EditorCommandHandler::request_telemetry(&mut telemetry),
        Ok(EditorCommandOutcome::TelemetryRequested)
    );
    assert_eq!(telemetry.requests, 1);
}

#[derive(Default)]
struct MockPatchIoService {
    calls: Vec<String>,
}

impl PatchIoService for MockPatchIoService {
    type Error = &'static str;

    fn save_patch(&mut self, path: &Path) -> Result<(), Self::Error> {
        self.calls.push(format!("save:{}", path.display()));
        Ok(())
    }

    fn load_patch(&mut self, path: &Path) -> Result<(), Self::Error> {
        self.calls.push(format!("load:{}", path.display()));
        Ok(())
    }

    fn export_patch_with_samples(&mut self, directory: &Path) -> Result<(), Self::Error> {
        self.calls.push(format!("export:{}", directory.display()));
        Ok(())
    }
}

#[derive(Default)]
struct MockSampleSlotService {
    calls: Vec<String>,
    fail_library_assignment: bool,
}

impl SampleSlotService for MockSampleSlotService {
    type SampleReference = String;
    type Error = &'static str;

    fn refresh_library(&mut self) -> Result<(), Self::Error> {
        self.calls.push("refresh".to_string());
        Ok(())
    }

    fn ingest_sample(&mut self, path: &Path) -> Result<Self::SampleReference, Self::Error> {
        let reference = path.display().to_string();
        self.calls.push(format!("ingest:{reference}"));
        Ok(reference)
    }

    fn assign_library_sample_to_slot(
        &mut self,
        sample_index: usize,
        slot: PadId,
    ) -> Result<(), Self::Error> {
        self.calls
            .push(format!("assign-library:{sample_index}:{}", slot.0));
        if self.fail_library_assignment {
            Err("library assignment failed")
        } else {
            Ok(())
        }
    }

    fn assign_sample_to_slot(
        &mut self,
        reference: Self::SampleReference,
        slot: PadId,
    ) -> Result<(), Self::Error> {
        self.calls
            .push(format!("assign-reference:{reference}:{}", slot.0));
        Ok(())
    }

    fn clear_slot(&mut self, slot: PadId) -> Result<(), Self::Error> {
        self.calls.push(format!("clear:{}", slot.0));
        Ok(())
    }
}

#[derive(Default)]
struct MockTelemetryService {
    requests: usize,
}

impl TelemetryRequestService for MockTelemetryService {
    type Error = &'static str;

    fn request_telemetry(&mut self) -> Result<(), Self::Error> {
        self.requests += 1;
        Ok(())
    }
}

fn command_fixtures() -> Vec<UiCommand> {
    let mut commands = vec![
        UiCommand::SavePatch,
        UiCommand::LoadPatch,
        UiCommand::ExportPatchWithSamples,
        UiCommand::OpenLibrary,
        UiCommand::LoadSelectedExcitationSlot,
        UiCommand::ClearSelectedExcitationSlot,
        UiCommand::RedetectSlices,
        UiCommand::TuneSelectedSlice,
        UiCommand::TuneAllSlices,
        UiCommand::SnapAllSlicesToScale,
    ];

    for index in 1..=16 {
        let slot = PadId::new(index).unwrap();
        commands.push(UiCommand::SelectExcitationSlot(slot));
        commands.push(UiCommand::LoadExcitationSlot(slot));
        commands.push(UiCommand::ClearExcitationSlot(slot));
    }

    commands
}
