use std::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
    path::PathBuf,
};

use lindelion_plugin_shell::{
    AudioPlugin, MidiControllerRoute, MidiEvent, MidiEventNormalizer, ParameterId, PluginState,
    ProcessContext as ShellProcessContext, ProcessSetup as ShellProcessSetup,
    vst3::{
        Vst3BusInfo, Vst3PeerConnection, can_process_32_bit_sample_size, clear_vst_outputs,
        fill_vst3_bus_info, for_each_vst3_parameter_change, process_setup_from_vst,
        read_plugin_state_from_stream, stereo_output_buffers_from_vst_process_data,
        vst_event_to_midi, vst3_bus_count, write_plugin_state_to_stream,
    },
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    Linnod, LinnodWorker, LinnodWorkerQueue, LinnodWorkerResult, SourceAnalysisJob,
    SourceAnalysisJobResult, SourceAnalysisStatus, patch_io,
    tuning::{
        snap_all_slices_to_scale, tune_all_slices_to_nearest_notes, tune_slice_to_nearest_note,
    },
};

use super::{
    LinnodPluginMessage, LinnodStatusPayload, LinnodVst3Controller, MAX_BLOCK_EVENTS,
    messages::{
        LinnodMarkerEditMessage, LinnodPadEditMessage, LinnodSliceEditMessage,
        LinnodTelemetryPayload,
    },
    patch_edits::{apply_marker_edit_message, apply_pad_edit_message, apply_slice_edit_message},
    processor_helpers::empty_midi_event,
    processor_notifications::send_source_summary_update,
};

#[path = "processor_detection.rs"]
mod processor_detection;
#[path = "processor_playback.rs"]
mod processor_playback;

const DEFAULT_PITCH_BEND_RANGE_SEMITONES: f32 = 2.0;
const LINNOD_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_output(2, "Output"),
    Vst3BusInfo::event_input(1, "MIDI Input"),
];
const LINNOD_MIDI_CONTROLLER_ROUTES: &[MidiControllerRoute] = &[
    MidiControllerRoute::new(ControllerNumbers_::kCtrlModWheel, 1),
    MidiControllerRoute::new(ControllerNumbers_::kCtrlFilterResonance, 74),
];

pub(super) struct LinnodVst3Processor {
    pub(super) plugin: RefCell<Linnod>,
    worker: RefCell<Box<dyn LinnodWorkerQueue>>,
    setup: Cell<ShellProcessSetup>,
    telemetry: Cell<LinnodTelemetryPayload>,
    peer: Vst3PeerConnection,
}

impl Class for LinnodVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IConnectionPoint,
    );
}

impl LinnodVst3Processor {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.processor_cid[0],
        crate::VST3_BUNDLE_METADATA.processor_cid[1],
        crate::VST3_BUNDLE_METADATA.processor_cid[2],
        crate::VST3_BUNDLE_METADATA.processor_cid[3],
    );

    pub(super) fn new() -> Self {
        Self::with_worker(Box::new(LinnodWorker::new()))
    }

    pub(super) fn with_worker(worker: Box<dyn LinnodWorkerQueue>) -> Self {
        let setup = ShellProcessSetup::default();
        let mut plugin = Linnod::default();
        plugin.reset(setup);
        Self {
            plugin: RefCell::new(plugin),
            worker: RefCell::new(worker),
            setup: Cell::new(setup),
            telemetry: Cell::new(LinnodTelemetryPayload::default()),
            peer: Vst3PeerConnection::new(),
        }
    }

    fn apply_parameter_changes(&self, changes: *mut IParameterChanges) {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return;
        };
        unsafe {
            for_each_vst3_parameter_change(changes, |change| {
                plugin.set_parameter_normalized(
                    ParameterId(change.id),
                    change.normalized_value as f32,
                );
            });
        }
    }

    fn process_events(&self, input_events: *mut IEventList, events: &mut [MidiEvent]) -> usize {
        let Some(input_events) = (unsafe { ComRef::from_raw(input_events) }) else {
            return 0;
        };
        let event_count = unsafe { input_events.getEventCount() }.max(0) as usize;
        let normalizer = MidiEventNormalizer::new(
            LINNOD_MIDI_CONTROLLER_ROUTES,
            DEFAULT_PITCH_BEND_RANGE_SEMITONES,
        );
        let mut used = 0;
        for index in 0..event_count.min(events.len()) {
            let mut event = MaybeUninit::<Event>::uninit();
            let result = unsafe { input_events.getEvent(index as i32, event.as_mut_ptr()) };
            if result == kResultOk
                && let Some(midi_event) =
                    unsafe { vst_event_to_midi(event.assume_init(), normalizer) }
            {
                events[used] = midi_event;
                used += 1;
            }
        }
        used
    }

    fn apply_patch_payload(&self, payload: &[u8]) -> tresult {
        let Ok(text) = std::str::from_utf8(payload) else {
            return kResultFalse;
        };
        let Ok(patch) = patch_io::from_toml_str(text) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.set_patch(patch);
        kResultOk
    }

    fn schedule_source_load(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let Some(job) = plugin.request_source_load_job() else {
            return self.send_status_update(LinnodStatusMessage::Status);
        };
        drop(plugin);
        if self.worker.borrow().schedule_source_analysis(job) {
            self.send_status_update(LinnodStatusMessage::Analysis)
        } else {
            kResultFalse
        }
    }

    fn schedule_source_ingest(&self, payload: &[u8]) -> tresult {
        let Ok(path) = std::str::from_utf8(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let job = plugin.request_source_ingest_job(PathBuf::from(path));
        drop(plugin);
        if self.worker.borrow().schedule_source_analysis(job) {
            self.send_status_update(LinnodStatusMessage::Analysis)
        } else {
            kResultFalse
        }
    }

    fn apply_marker_edit(&self, payload: &[u8]) -> tresult {
        let Some(edit) = LinnodMarkerEditMessage::decode(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        let source_len = plugin
            .source_audio()
            .map(|audio| audio.samples().len())
            .unwrap_or(usize::MAX);
        apply_marker_edit_message(&mut patch, edit, source_len);
        plugin.set_patch(patch);
        drop(plugin);
        self.send_patch_update()
    }

    fn apply_slice_edit(&self, payload: &[u8]) -> tresult {
        let Some(edit) = LinnodSliceEditMessage::decode(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        if !apply_slice_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        plugin.set_patch_preserving_source_analysis(patch);
        drop(plugin);
        self.send_patch_update()
    }

    fn apply_pad_edit(&self, payload: &[u8]) -> tresult {
        let Some(edit) = LinnodPadEditMessage::decode(payload) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        if !apply_pad_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        plugin.set_patch_preserving_source_analysis(patch);
        drop(plugin);
        self.send_patch_update()
    }

    fn tune_selected_slice(&self) -> tresult {
        self.tune_patch(|patch, cache| {
            patch
                .selected_slice_index()
                .is_some_and(|index| tune_slice_to_nearest_note(patch, cache, index))
        })
    }

    fn tune_all_slices(&self) -> tresult {
        self.tune_patch(|patch, cache| tune_all_slices_to_nearest_notes(patch, cache) > 0)
    }

    fn snap_all_slices(&self) -> tresult {
        self.tune_patch(|patch, cache| snap_all_slices_to_scale(patch, cache) > 0)
    }

    fn tune_patch(
        &self,
        tune: impl FnOnce(
            &mut crate::LinnodPatch,
            &lindelion_pitch_shift::PitchShiftSourceCache,
        ) -> bool,
    ) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let Some(cache) = plugin
            .source_analysis()
            .map(|analysis| analysis.pitch_shift_cache.clone())
        else {
            return kResultFalse;
        };
        let mut patch = plugin.patch().clone();
        if !tune(&mut patch, &cache) {
            return kResultFalse;
        }
        plugin.set_patch_preserving_source_analysis(patch);
        drop(plugin);
        self.send_patch_update()
    }

    fn drain_worker_results(&self) -> tresult {
        let mut status = kResultOk;
        self.worker
            .borrow()
            .drain_results(&mut |result| match result {
                LinnodWorkerResult::SourceAnalysis(result) => {
                    let accepted = if let Ok(mut plugin) = self.plugin.try_borrow_mut() {
                        plugin.publish_source_analysis_result(result)
                    } else {
                        status = kResultFalse;
                        false
                    };
                    if accepted {
                        let patch_result = self.send_patch_update();
                        if patch_result != kResultOk {
                            status = patch_result;
                        }
                        let source_result = send_source_summary_update(&self.plugin, &self.peer);
                        if status == kResultOk && source_result != kResultOk {
                            status = source_result;
                        }
                    }
                }
            });
        if status == kResultOk {
            self.send_status_update(LinnodStatusMessage::Analysis)
        } else {
            status
        }
    }

    fn schedule_pending_source_load(&self) -> tresult {
        let pending = self
            .plugin
            .try_borrow()
            .map(|plugin| plugin.source_status() == SourceAnalysisStatus::PendingLoad)
            .unwrap_or(false);
        if pending {
            self.schedule_source_load()
        } else {
            kResultOk
        }
    }

    fn send_patch_update(&self) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let Ok(payload) = patch_io::to_toml_string(plugin.patch()) else {
            return kResultFalse;
        };
        self.peer.notify_if_connected(
            LinnodPluginMessage::patch_update(payload.into_bytes()).into_com_message(),
        )
    }

    fn send_status_update(&self, kind: LinnodStatusMessage) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let status = LinnodStatusPayload::from_plugin(&plugin);
        drop(plugin);
        let message = match kind {
            LinnodStatusMessage::Analysis => LinnodPluginMessage::AnalysisStatusResponse(status),
            LinnodStatusMessage::Status => LinnodPluginMessage::StatusResponse(status),
        };
        self.peer.notify_if_connected(message.into_com_message())
    }

    fn send_telemetry_update(&self) -> tresult {
        self.peer.notify_if_connected(
            LinnodPluginMessage::TelemetryResponse(self.telemetry.get().encode())
                .into_com_message(),
        )
    }
}

include!("processor_restore.rs");

impl IPluginBaseTrait for LinnodVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for LinnodVst3Processor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        *class_id = LinnodVst3Controller::CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        vst3_bus_count(&LINNOD_BUSES, media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        fill_vst3_bus_info(&LINNOD_BUSES, media_type, dir, index, bus)
    }

    unsafe fn getRoutingInfo(
        &self,
        _in_info: *mut RoutingInfo,
        _out_info: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        _media_type: MediaType,
        _dir: BusDirection,
        _index: i32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultFalse;
        };
        self.restore_plugin_state(plugin_state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        if write_plugin_state_to_stream(state, plugin.state()) {
            kResultOk
        } else {
            kResultFalse
        }
    }
}

impl IAudioProcessorTrait for LinnodVst3Processor {
    unsafe fn setBusArrangements(
        &self,
        _inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins == 0 && num_outs == 1 && !outputs.is_null() && *outputs == SpeakerArr::kStereo {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        if arrangement.is_null() || index != 0 || dir as BusDirections != BusDirections_::kOutput {
            return kInvalidArgument;
        }
        *arrangement = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        can_process_32_bit_sample_size(symbolic_sample_size)
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        if setup.is_null() {
            return kInvalidArgument;
        }
        let shell_setup = process_setup_from_vst(&*setup);
        self.setup.set(shell_setup);
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.reset(shell_setup);
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        let data = &mut *data;
        self.apply_parameter_changes(data.inputParameterChanges);

        if data.symbolicSampleSize as SymbolicSampleSizes != SymbolicSampleSizes_::kSample32 {
            clear_vst_outputs(data);
            return kResultOk;
        }

        let data_ptr = data as *mut ProcessData;
        let input_events = (*data_ptr).inputEvents;
        let Some(buffer) = stereo_output_buffers_from_vst_process_data(&mut *data_ptr) else {
            clear_vst_outputs(&mut *data_ptr);
            return kResultOk;
        };
        let mut events = [empty_midi_event(); MAX_BLOCK_EVENTS];
        let event_count = self.process_events(input_events, &mut events);

        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            let mut buffer = buffer;
            buffer.clear();
            return kResultFalse;
        };
        plugin.process(ShellProcessContext::new(
            self.setup.get(),
            buffer,
            &events[..event_count],
        ));
        self.telemetry.set(LinnodTelemetryPayload {
            active_voices: plugin.active_voice_count() as f32,
            ..self.telemetry.get()
        });
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for LinnodVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

impl IConnectionPointTrait for LinnodVst3Processor {
    unsafe fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.connect(other)
    }

    unsafe fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.disconnect(other)
    }

    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        let drain_result = self.drain_worker_results();
        if drain_result != kResultOk {
            return drain_result;
        }
        let schedule_result = self.schedule_pending_source_load();
        if schedule_result != kResultOk {
            return schedule_result;
        }

        let message = match LinnodPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };
        match message {
            LinnodPluginMessage::PatchUpdate(payload) => self.apply_patch_payload(&payload),
            LinnodPluginMessage::SourceLoadRequest | LinnodPluginMessage::RedetectSlices => {
                self.schedule_source_load()
            }
            LinnodPluginMessage::SourceIngestRequest(payload) => {
                self.schedule_source_ingest(&payload)
            }
            LinnodPluginMessage::TuneSelectedSlice => self.tune_selected_slice(),
            LinnodPluginMessage::TuneAllSlices => self.tune_all_slices(),
            LinnodPluginMessage::SnapAllSlicesToScale => self.snap_all_slices(),
            LinnodPluginMessage::MarkerEdit(payload) => self.apply_marker_edit(&payload),
            LinnodPluginMessage::PadEdit(payload) => self.apply_pad_edit(&payload),
            LinnodPluginMessage::PlaybackEdit(payload) => self.apply_playback_edit(&payload),
            LinnodPluginMessage::DetectionEdit(payload) => self.apply_detection_edit(&payload),
            LinnodPluginMessage::SliceEdit(payload) => self.apply_slice_edit(&payload),
            LinnodPluginMessage::StatusRequest => {
                self.send_status_update(LinnodStatusMessage::Status)
            }
            LinnodPluginMessage::SourceSummaryRequest => {
                send_source_summary_update(&self.plugin, &self.peer)
            }
            LinnodPluginMessage::TelemetryRequest => self.send_telemetry_update(),
            LinnodPluginMessage::AnalysisStatusResponse(_)
            | LinnodPluginMessage::StatusResponse(_)
            | LinnodPluginMessage::SourceSummaryResponse(_)
            | LinnodPluginMessage::TelemetryResponse(_) => kNotImplemented,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinnodStatusMessage {
    Analysis,
    Status,
}
