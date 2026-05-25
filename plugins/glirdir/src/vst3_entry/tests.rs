use std::{cell::RefCell, fs, rc::Rc};

use lindelion_plugin_shell::{
    AudioBuffer, AudioInputBuffer, AudioPlugin, ProcessContext, ProcessMode, ProcessSetup,
    vst3::{PluginMessage, PluginMessageDecodeError, PluginMessageType},
};
use vst3::{Class, ComPtr, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use super::*;
use crate::{
    AnalysisError, AnalysisJob, AnalysisJobResult, AnalysisSequence, AnalysisStatus, CaptureState,
    GlirdirPatch, GlirdirWorkerQueue, GlirdirWorkerResult, RequantizeJob,
    TIMING_STRENGTH_PARAMETER_ID,
    midi_export::{MidiExportJob, MidiExportPayload},
    patch_io,
    sample_library::{SampleLibrarySavePayload, SampleLibrarySaveStatus},
    vst3_entry::{
        controller::parameter_index, messages::GlirdirMessageKind, processor::GlirdirVst3Processor,
    },
};
use lindelion_midi::MidiClip;
use lindelion_pitch_detect::PitchContour;
use lindelion_plugin_shell::ParameterId;
use lindelion_ui::glirdir_vizia::{
    GlirdirEditorCommand, GlirdirEditorLibraryStatus, GlirdirEditorMidiDrag,
};

#[test]
fn plugin_message_roundtrips_controller_payloads() {
    let status = GlirdirStatusPayload {
        capture_state: CaptureState::Captured,
        analysis_status: AnalysisStatus::Ready,
        has_scratchpad: true,
        has_analysis: true,
    };

    assert_message_roundtrip(GlirdirPluginMessage::PatchUpdate(b"patch".to_vec()));
    assert_message_roundtrip(GlirdirPluginMessage::arm_capture());
    assert_message_roundtrip(GlirdirPluginMessage::clear_scratchpad());
    assert_message_roundtrip(GlirdirPluginMessage::finalize_capture_request());
    assert_message_roundtrip(GlirdirPluginMessage::play_audition());
    assert_message_roundtrip(GlirdirPluginMessage::stop_audition());
    assert_message_roundtrip(GlirdirPluginMessage::toggle_audition_loop());
    assert_message_roundtrip(GlirdirPluginMessage::toggle_audition_live_edit());
    assert_message_roundtrip(GlirdirPluginMessage::sample_library_save_request());
    assert_message_roundtrip(GlirdirPluginMessage::SampleLibrarySaveResponse(
        SampleLibrarySavePayload::saved("scratch.wav").encode(),
    ));
    assert_message_roundtrip(GlirdirPluginMessage::midi_export_request());
    assert_message_roundtrip(GlirdirPluginMessage::MidiExportResponse(b"midi".to_vec()));
    assert_message_roundtrip(GlirdirPluginMessage::status_request());
    assert_message_roundtrip(GlirdirPluginMessage::StatusResponse(status));
    assert_message_roundtrip(GlirdirPluginMessage::TelemetryRequest);
    assert_message_roundtrip(GlirdirPluginMessage::TelemetryResponse(status));
    assert_message_roundtrip(GlirdirPluginMessage::AnalysisStatusResponse(status));
}

#[test]
fn unknown_plugin_messages_are_ignored_safely() {
    let processor = GlirdirVst3Processor::new();
    let controller = GlirdirVst3Controller::new();
    let message = PluginMessage::with_payload("lindelion.glirdir.future", Vec::new())
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { GlirdirPluginMessage::decode(message.as_ptr()) };
    let processor_result = unsafe { processor.notify(message.as_ptr()) };
    let controller_result = unsafe { controller.notify(message.as_ptr()) };

    assert_eq!(decoded, Ok(None));
    assert_eq!(processor_result, kNotImplemented);
    assert_eq!(controller_result, kNotImplemented);
}

#[test]
fn malformed_plugin_message_payloads_do_not_panic() {
    let processor = GlirdirVst3Processor::new();
    let message = PluginMessage::with_payload(
        GlirdirMessageKind::StatusRequest.id(),
        b"unexpected".to_vec(),
    )
    .to_com_ptr::<IMessage>()
    .unwrap();

    let decoded = unsafe { GlirdirPluginMessage::decode(message.as_ptr()) };
    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(decoded, Err(PluginMessageDecodeError::MalformedPayload));
    assert_eq!(result, kResultFalse);
}

#[test]
fn controller_patch_mirror_tracks_parameter_edits() {
    let controller = GlirdirVst3Controller::new();

    assert_eq!(
        controller.set_value(TIMING_STRENGTH_PARAMETER_ID, 0.25),
        kResultOk
    );

    assert_eq!(controller.patch.borrow().quantize.timing_strength, 0.25);
    assert_eq!(
        controller
            .values
            .value(parameter_index(TIMING_STRENGTH_PARAMETER_ID).unwrap())
            .unwrap(),
        0.25
    );
}

#[test]
fn controller_status_response_updates_mirror() {
    let controller = GlirdirVst3Controller::new();
    let status = GlirdirStatusPayload {
        capture_state: CaptureState::Captured,
        analysis_status: AnalysisStatus::CapturedPendingAnalysis,
        has_scratchpad: true,
        has_analysis: false,
    };
    let message = GlirdirPluginMessage::StatusResponse(status)
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    assert_eq!(unsafe { controller.notify(message.as_ptr()) }, kResultOk);

    assert_eq!(controller.status.get(), status);
}

#[test]
fn controller_sample_library_response_updates_editor_status() {
    let controller = GlirdirVst3Controller::new();
    let payload = SampleLibrarySavePayload::empty_scratchpad().encode();
    let message = GlirdirPluginMessage::SampleLibrarySaveResponse(payload)
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();
    let host = super::editor::glirdir_editor_host(std::ptr::from_ref(&controller));

    assert_eq!(unsafe { controller.notify(message.as_ptr()) }, kResultOk);

    assert_eq!(
        unsafe { host.status() }.library_status,
        GlirdirEditorLibraryStatus::EmptyScratchpad
    );
}

#[test]
fn processor_notify_applies_patch_payload() {
    let processor = GlirdirVst3Processor::new();
    let patch = GlirdirPatch {
        name: "Bridge Patch".to_string(),
        ..GlirdirPatch::default()
    };
    let payload = patch_io::to_toml_string(&patch).unwrap().into_bytes();
    let message = GlirdirPluginMessage::PatchUpdate(payload)
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(result, kResultOk);
    assert_eq!(processor.plugin.borrow().patch().name, "Bridge Patch");
}

#[test]
fn processor_save_request_rejects_empty_scratchpad_visibly() {
    let controller_messages = Rc::new(RefCell::new(Vec::new()));
    let peer = recording_peer(Rc::clone(&controller_messages));
    let processor = GlirdirVst3Processor::new();

    assert_eq!(unsafe { processor.connect(peer.as_ptr()) }, kResultOk);
    notify_processor(
        &processor,
        GlirdirPluginMessage::sample_library_save_request(),
    );

    let messages = controller_messages.borrow();
    let [GlirdirPluginMessage::SampleLibrarySaveResponse(payload)] = messages.as_slice() else {
        panic!("expected sample-library save response");
    };
    let payload = SampleLibrarySavePayload::decode(payload).expect("save response payload");
    assert_eq!(payload.status, SampleLibrarySaveStatus::EmptyScratchpad);
}

#[test]
fn processor_finalize_request_materializes_analysis_job_off_audio_path() {
    let processor = GlirdirVst3Processor::new();
    capture_one_block(&processor);
    let message = GlirdirPluginMessage::finalize_capture_request()
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    let result = unsafe { processor.notify(message.as_ptr()) };

    let plugin = processor.plugin.borrow();
    assert_eq!(result, kResultOk);
    assert!(plugin.patch().scratchpad.is_some());
    assert_eq!(plugin.analysis_status(), AnalysisStatus::Analyzing);
}

#[test]
fn captured_buffer_schedules_one_analysis_job() {
    let worker = Rc::new(RecordingWorker::default());
    let processor = GlirdirVst3Processor::with_worker(Box::new(Rc::clone(&worker)));
    capture_one_block(&processor);
    let message = GlirdirPluginMessage::finalize_capture_request()
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    assert_eq!(unsafe { processor.notify(message.as_ptr()) }, kResultOk);

    let jobs = worker.analysis_jobs.borrow();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].sample_rate, 10);
    assert_eq!(jobs[0].scratchpad.samples.len(), 80);
    assert_eq!(
        processor.plugin.borrow().analysis_status(),
        AnalysisStatus::Analyzing
    );
}

#[test]
fn clearing_during_analysis_prevents_stale_worker_publication() {
    let worker = Rc::new(RecordingWorker::default());
    let processor = GlirdirVst3Processor::with_worker(Box::new(Rc::clone(&worker)));
    capture_one_block(&processor);
    notify_processor(&processor, GlirdirPluginMessage::finalize_capture_request());
    let sequence = worker.analysis_jobs.borrow()[0].sequence;

    notify_processor(&processor, GlirdirPluginMessage::clear_scratchpad());
    worker.push_result(GlirdirWorkerResult::Analysis(AnalysisJobResult::error(
        sequence,
        AnalysisError::EmptyScratchpad,
    )));
    notify_processor(&processor, GlirdirPluginMessage::status_request());

    let plugin = processor.plugin.borrow();
    assert_eq!(plugin.analysis_status(), AnalysisStatus::Idle);
    assert!(plugin.analysis().is_none());
    assert!(plugin.patch().scratchpad.is_none());
}

#[test]
fn quantize_only_change_does_not_schedule_analysis_worker() {
    let worker = Rc::new(RecordingWorker::default());
    let processor = GlirdirVst3Processor::with_worker(Box::new(Rc::clone(&worker)));
    let patch = GlirdirPatch {
        scratchpad: Some(crate::ScratchpadAudio::new(48_000, vec![0.2; 4_800])),
        ..GlirdirPatch::default()
    };

    {
        let mut plugin = processor.plugin.borrow_mut();
        AudioPlugin::load_state(&mut *plugin, patch_io::to_plugin_state(&patch).unwrap());
        let job = plugin.request_analysis_job().expect("analysis job");
        assert!(plugin.publish_analysis_result(AnalysisJobResult::ready(
            job.sequence,
            empty_analysis_result()
        )));
        plugin.set_parameter_normalized(ParameterId(TIMING_STRENGTH_PARAMETER_ID), 0.25);
    }

    assert!(worker.analysis_jobs.borrow().is_empty());
}

#[test]
fn controller_emits_finalize_request_to_peer() {
    let controller = GlirdirVst3Controller::new();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let peer = recording_peer(Rc::clone(&messages));

    assert_eq!(unsafe { controller.connect(peer.as_ptr()) }, kResultOk);
    assert_eq!(controller.request_finalize_completed_capture(), kResultOk);

    assert_eq!(
        messages.borrow().as_slice(),
        &[GlirdirPluginMessage::FinalizeCaptureRequest]
    );
}

#[test]
fn editor_host_commands_emit_typed_controller_messages() {
    let controller = GlirdirVst3Controller::new();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let peer = recording_peer(Rc::clone(&messages));
    let host = super::editor::glirdir_editor_host(std::ptr::from_ref(&controller));

    assert_eq!(unsafe { controller.connect(peer.as_ptr()) }, kResultOk);
    unsafe {
        host.handle_command(GlirdirEditorCommand::ArmCapture);
        host.handle_command(GlirdirEditorCommand::PlayAudition);
        host.handle_command(GlirdirEditorCommand::StopAudition);
        host.handle_command(GlirdirEditorCommand::ToggleLoop);
        host.handle_command(GlirdirEditorCommand::ToggleLiveEdit);
        host.handle_command(GlirdirEditorCommand::SaveScratchpadToLibrary);
        host.handle_command(GlirdirEditorCommand::ExportMidi);
    }

    assert_eq!(
        messages.borrow().as_slice(),
        &[
            GlirdirPluginMessage::ArmCapture,
            GlirdirPluginMessage::PlayAudition,
            GlirdirPluginMessage::StopAudition,
            GlirdirPluginMessage::ToggleAuditionLoop,
            GlirdirPluginMessage::ToggleAuditionLiveEdit,
            GlirdirPluginMessage::SampleLibrarySaveRequest,
            GlirdirPluginMessage::MidiExportRequest,
        ]
    );
}

#[test]
fn controller_prepares_empty_drag_file_without_analysis() {
    let controller = GlirdirVst3Controller::new();

    let drag = controller.prepare_midi_drag_file();

    let GlirdirEditorMidiDrag::Ready { path } = drag else {
        panic!("empty drag should produce a MIDI file");
    };
    let bytes = fs::read(&path).expect("drag MIDI file should exist");
    assert_eq!(&bytes[..4], b"MThd");
    controller.cleanup_midi_drag_files();
    assert!(!path.exists());
}

#[test]
fn controller_bounds_repeated_drag_temp_files() {
    let controller = GlirdirVst3Controller::new();
    let midi = MidiClip::empty(120).to_smf_bytes().unwrap();
    controller
        .last_midi_export
        .replace(Some(MidiExportPayload::new(
            "glirdir-Cchrom-4bar-120bpm.mid",
            midi,
        )));
    let mut paths = Vec::new();

    for _ in 0..(super::controller::MAX_MIDI_DRAG_FILES + 2) {
        let GlirdirEditorMidiDrag::Ready { path } = controller.prepare_midi_drag_file() else {
            panic!("cached MIDI export should be drag-ready");
        };
        paths.push(path);
    }

    assert!(controller.midi_drag_files.borrow().len() <= super::controller::MAX_MIDI_DRAG_FILES);
    assert!(
        paths
            .last()
            .unwrap()
            .ends_with("glirdir-Cchrom-4bar-120bpm.mid")
    );
    assert!(!paths[0].exists());
    assert!(paths.last().is_some_and(|path| path.exists()));
    controller.cleanup_midi_drag_files();
}

#[test]
fn controller_queues_drag_export_when_analysis_has_no_cached_midi() {
    let controller = GlirdirVst3Controller::new();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let peer = recording_peer(Rc::clone(&messages));
    controller.status.set(GlirdirStatusPayload {
        capture_state: CaptureState::Captured,
        analysis_status: AnalysisStatus::Ready,
        has_scratchpad: true,
        has_analysis: true,
    });

    assert_eq!(unsafe { controller.connect(peer.as_ptr()) }, kResultOk);
    assert_eq!(
        controller.prepare_midi_drag_file(),
        GlirdirEditorMidiDrag::Requested
    );
    assert_eq!(
        messages.borrow().as_slice(),
        &[GlirdirPluginMessage::MidiExportRequest]
    );
}

fn notify_processor(processor: &GlirdirVst3Processor, message: GlirdirPluginMessage) {
    let message = message.into_com_message().to_com_ptr::<IMessage>().unwrap();
    assert_eq!(unsafe { processor.notify(message.as_ptr()) }, kResultOk);
}

fn empty_analysis_result() -> crate::AnalysisResult {
    crate::AnalysisResult {
        pitch_contour: PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: Vec::new(),
        },
        markers: Vec::new(),
        detected_notes: Vec::new(),
        midi_clip: MidiClip::empty(120),
    }
}

fn assert_message_roundtrip(message: GlirdirPluginMessage) {
    let com_message = message
        .clone()
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    assert_eq!(
        unsafe { GlirdirPluginMessage::decode(com_message.as_ptr()) },
        Ok(Some(message))
    );
}

fn capture_one_block(processor: &GlirdirVst3Processor) {
    let setup = ProcessSetup {
        sample_rate: 10.0,
        max_block_size: 80,
        mode: ProcessMode::Realtime,
    };
    let input = vec![0.25; 80];
    let mut left = vec![0.0; 80];
    let mut right = vec![0.0; 80];
    let mut plugin = processor.plugin.borrow_mut();

    plugin.reset(setup);
    plugin.arm_capture();
    plugin.process(
        ProcessContext::new(
            setup,
            AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            &[],
        )
        .with_input(AudioInputBuffer::mono(&input)),
    );
}

fn recording_peer(messages: Rc<RefCell<Vec<GlirdirPluginMessage>>>) -> ComPtr<IConnectionPoint> {
    ComWrapper::new(RecordingConnectionPoint { messages })
        .to_com_ptr::<IConnectionPoint>()
        .expect("RecordingConnectionPoint must expose IConnectionPoint")
}

struct RecordingConnectionPoint {
    messages: Rc<RefCell<Vec<GlirdirPluginMessage>>>,
}

impl Class for RecordingConnectionPoint {
    type Interfaces = (IConnectionPoint,);
}

impl IConnectionPointTrait for RecordingConnectionPoint {
    unsafe fn connect(&self, _other: *mut IConnectionPoint) -> tresult {
        kResultOk
    }

    unsafe fn disconnect(&self, _other: *mut IConnectionPoint) -> tresult {
        kResultOk
    }

    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        match GlirdirPluginMessage::decode(message) {
            Ok(Some(message)) => {
                self.messages.borrow_mut().push(message);
                kResultOk
            }
            Ok(None) => kNotImplemented,
            Err(_) => kResultFalse,
        }
    }
}

#[derive(Default)]
struct RecordingWorker {
    analysis_jobs: RefCell<Vec<AnalysisJob>>,
    requantize_jobs: RefCell<Vec<RequantizeJob>>,
    midi_exports: RefCell<Vec<AnalysisSequence>>,
    sample_saves: RefCell<Vec<AnalysisSequence>>,
    results: RefCell<Vec<GlirdirWorkerResult>>,
}

impl RecordingWorker {
    fn push_result(&self, result: GlirdirWorkerResult) {
        self.results.borrow_mut().push(result);
    }
}

impl GlirdirWorkerQueue for Rc<RecordingWorker> {
    fn schedule_analysis(&self, job: AnalysisJob) -> bool {
        self.analysis_jobs.borrow_mut().push(job);
        true
    }

    fn schedule_requantize(&self, job: RequantizeJob) -> bool {
        self.requantize_jobs.borrow_mut().push(job);
        true
    }

    fn schedule_midi_export(&self, job: MidiExportJob) -> bool {
        self.midi_exports.borrow_mut().push(job.sequence);
        true
    }

    fn schedule_sample_library_save(
        &self,
        job: crate::sample_library::SampleLibrarySaveJob,
    ) -> bool {
        self.sample_saves.borrow_mut().push(job.sequence);
        true
    }

    fn drain_results(&self, publish: &mut dyn FnMut(GlirdirWorkerResult)) -> usize {
        let mut count = 0;
        while let Some(result) = self.results.borrow_mut().pop() {
            publish(result);
            count += 1;
        }
        count
    }
}
