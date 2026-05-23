use std::{
    cell::{Cell, RefCell},
    ptr,
};

use lindelion_plugin_shell::{
    AudioPlugin, ParameterId, ProcessContext as ShellProcessContext, ProcessMode,
    ProcessSetup as ShellProcessSetup,
    vst3::{
        audio_input_buffer_from_vst_process_data, clear_vst_outputs, copy_wstring,
        read_plugin_state_from_stream, stereo_output_buffers_from_vst_process_data,
        transport_context_from_vst_process_context, write_plugin_state_to_stream,
    },
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*};

use crate::{
    AnalysisStatus, Glirdir, GlirdirWorker, GlirdirWorkerQueue, GlirdirWorkerResult,
    ParameterApplyKind,
};

use super::{GlirdirPluginMessage, GlirdirStatusPayload, GlirdirVst3Controller};

pub(super) struct GlirdirVst3Processor {
    pub(super) plugin: RefCell<Glirdir>,
    worker: RefCell<Box<dyn GlirdirWorkerQueue>>,
    pending_requantize: Cell<bool>,
    setup: Cell<ShellProcessSetup>,
    input_arrangement: Cell<SpeakerArrangement>,
    peer: Cell<*mut IConnectionPoint>,
}

impl Class for GlirdirVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IConnectionPoint,
    );
}

impl GlirdirVst3Processor {
    pub(super) fn new() -> Self {
        Self::with_worker(Box::new(GlirdirWorker::new()))
    }

    pub(super) fn with_worker(worker: Box<dyn GlirdirWorkerQueue>) -> Self {
        let setup = ShellProcessSetup::default();
        let mut plugin = Glirdir::default();
        plugin.reset(setup);
        Self {
            plugin: RefCell::new(plugin),
            worker: RefCell::new(worker),
            pending_requantize: Cell::new(false),
            setup: Cell::new(setup),
            input_arrangement: Cell::new(SpeakerArr::kStereo),
            peer: Cell::new(ptr::null_mut()),
        }
    }

    fn apply_parameter_changes(&self, changes: *mut IParameterChanges) {
        let Some(changes) = (unsafe { ComRef::from_raw(changes) }) else {
            return;
        };

        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return;
        };

        for index in 0..unsafe { changes.getParameterCount() } {
            let Some(queue) = (unsafe { ComRef::from_raw(changes.getParameterData(index)) }) else {
                continue;
            };
            let point_count = unsafe { queue.getPointCount() };
            if point_count <= 0 {
                continue;
            }

            let mut sample_offset = 0;
            let mut value = 0.0;
            let result = unsafe { queue.getPoint(point_count - 1, &mut sample_offset, &mut value) };
            if result == kResultTrue {
                let parameter_id = unsafe { queue.getParameterId() };
                let apply = plugin
                    .set_parameter_normalized_deferred(ParameterId(parameter_id), value as f32);
                self.track_deferred_parameter_apply(apply);
            }
        }
    }

    fn track_deferred_parameter_apply(&self, apply: ParameterApplyKind) {
        match apply {
            ParameterApplyKind::Quantize => self.pending_requantize.set(true),
            ParameterApplyKind::Analysis => self.pending_requantize.set(false),
            ParameterApplyKind::Capture
            | ParameterApplyKind::Audition
            | ParameterApplyKind::Ignored => {}
        }
    }

    fn apply_patch_payload(&self, payload: &[u8]) -> tresult {
        let Ok(text) = std::str::from_utf8(payload) else {
            return kResultFalse;
        };
        let Ok(patch) = crate::patch_io::from_toml_str(text) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.set_patch(patch);
        kResultOk
    }

    fn arm_capture(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.arm_capture();
        self.pending_requantize.set(false);
        drop(plugin);
        self.send_status_update(GlirdirStatusMessage::Analysis)
    }

    fn clear_scratchpad(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.clear_capture();
        self.pending_requantize.set(false);
        drop(plugin);
        self.send_status_update(GlirdirStatusMessage::Analysis)
    }

    fn finalize_completed_capture(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let job = plugin
            .finalize_completed_capture()
            .or_else(|| request_pending_analysis_job(&mut plugin));
        drop(plugin);
        if let Some(job) = job
            && !self.worker.borrow().schedule_analysis(job)
        {
            return kResultFalse;
        }
        self.pending_requantize.set(false);
        self.send_status_update(GlirdirStatusMessage::Analysis)
    }

    fn schedule_deferred_jobs(&self) -> tresult {
        if self.schedule_pending_analysis() == kResultFalse {
            return kResultFalse;
        }
        if self.pending_requantize.replace(false) {
            return self.schedule_pending_requantize();
        }
        kResultOk
    }

    fn schedule_pending_analysis(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let job = request_pending_analysis_job(&mut plugin);
        drop(plugin);
        if let Some(job) = job {
            self.pending_requantize.set(false);
            if self.worker.borrow().schedule_analysis(job) {
                kResultOk
            } else {
                kResultFalse
            }
        } else {
            kResultOk
        }
    }

    fn schedule_pending_requantize(&self) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        let job = plugin.request_requantize_job();
        drop(plugin);
        if let Some(job) = job
            && !self.worker.borrow().schedule_requantize(job)
        {
            return kResultFalse;
        }
        kResultOk
    }

    fn schedule_midi_export(&self) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let Some(analysis) = plugin.analysis() else {
            return kResultFalse;
        };
        let sequence = plugin.analysis_cache().sequence();
        let clip = analysis.midi_clip.clone();
        drop(plugin);

        if self.worker.borrow().schedule_midi_export(sequence, clip) {
            kResultOk
        } else {
            kResultFalse
        }
    }

    fn drain_worker_results(&self) -> tresult {
        let mut status = kResultOk;
        self.worker
            .borrow()
            .drain_results(&mut |result| match result {
                GlirdirWorkerResult::Analysis(result) => {
                    if let Ok(mut plugin) = self.plugin.try_borrow_mut() {
                        plugin.publish_analysis_result(result);
                    } else {
                        status = kResultFalse;
                    }
                }
                GlirdirWorkerResult::MidiExport { sequence, bytes } => {
                    if self.midi_export_is_current(sequence) {
                        let result =
                            self.send_to_peer(GlirdirPluginMessage::MidiExportResponse(bytes));
                        if result != kResultOk {
                            status = result;
                        }
                    }
                }
            });
        status
    }

    fn midi_export_is_current(&self, sequence: u64) -> bool {
        self.plugin
            .try_borrow()
            .map(|plugin| plugin.analysis_cache().sequence() == sequence)
            .unwrap_or(false)
    }

    fn send_status_update(&self, kind: GlirdirStatusMessage) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        let status = GlirdirStatusPayload::from_plugin(&plugin);
        drop(plugin);

        match kind {
            GlirdirStatusMessage::Analysis => {
                self.send_to_peer(GlirdirPluginMessage::AnalysisStatusResponse(status))
            }
            GlirdirStatusMessage::Status => {
                self.send_to_peer(GlirdirPluginMessage::StatusResponse(status))
            }
            GlirdirStatusMessage::Telemetry => {
                self.send_to_peer(GlirdirPluginMessage::TelemetryResponse(status))
            }
        }
    }

    fn send_to_peer(&self, message: GlirdirPluginMessage) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultOk;
        };
        let message = message.into_com_message();
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }
}

impl IPluginBaseTrait for GlirdirVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for GlirdirVst3Processor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        *class_id = GlirdirVst3Controller::CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kInput) => 1,
            (MediaTypes_::kAudio, BusDirections_::kOutput) => 1,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        if bus.is_null() || index != 0 {
            return kInvalidArgument;
        }

        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kInput) => {
                fill_bus_info(&mut *bus, media_type, dir, 2, "Input");
                kResultOk
            }
            (MediaTypes_::kAudio, BusDirections_::kOutput) => {
                fill_bus_info(&mut *bus, media_type, dir, 2, "Output");
                kResultOk
            }
            _ => kInvalidArgument,
        }
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
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.load_state(plugin_state);
        kResultOk
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

impl IAudioProcessorTrait for GlirdirVst3Processor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if inputs.is_null() || outputs.is_null() || num_ins != 1 || num_outs != 1 {
            return kResultFalse;
        }
        if input_arrangement_supported(*inputs) && *outputs == SpeakerArr::kStereo {
            self.input_arrangement.set(*inputs);
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
        if arrangement.is_null() || index != 0 {
            return kInvalidArgument;
        }

        match dir as BusDirections {
            BusDirections_::kInput => {
                *arrangement = self.input_arrangement.get();
                kResultOk
            }
            BusDirections_::kOutput => {
                *arrangement = SpeakerArr::kStereo;
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        match symbolic_sample_size as SymbolicSampleSizes {
            SymbolicSampleSizes_::kSample32 => kResultOk,
            SymbolicSampleSizes_::kSample64 => kNotImplemented,
            _ => kInvalidArgument,
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        if setup.is_null() {
            return kInvalidArgument;
        }

        let setup = &*setup;
        let mode = if setup.processMode as ProcessModes == ProcessModes_::kOffline {
            ProcessMode::Offline
        } else {
            ProcessMode::Realtime
        };
        let shell_setup = ShellProcessSetup {
            sample_rate: setup.sampleRate,
            max_block_size: setup.maxSamplesPerBlock.max(1) as usize,
            mode,
        };
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
        let input = audio_input_buffer_from_vst_process_data(&*data_ptr);
        let transport = transport_context_from_vst_process_context((*data_ptr).processContext);
        let Some(buffer) = stereo_output_buffers_from_vst_process_data(&mut *data_ptr) else {
            clear_vst_outputs(&mut *data_ptr);
            return kResultOk;
        };

        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            let mut buffer = buffer;
            buffer.clear();
            return kResultFalse;
        };
        plugin.process(
            ShellProcessContext::new(self.setup.get(), buffer, &[])
                .with_input(input)
                .with_transport(transport),
        );

        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for GlirdirVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        IProcessContextRequirements_::Flags_::kNeedBarPositionMusic
            | IProcessContextRequirements_::Flags_::kNeedProjectTimeMusic
            | IProcessContextRequirements_::Flags_::kNeedTempo
            | IProcessContextRequirements_::Flags_::kNeedTimeSignature
            | IProcessContextRequirements_::Flags_::kNeedTransportState
    }
}

impl IConnectionPointTrait for GlirdirVst3Processor {
    unsafe fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.set(other);
        kResultOk
    }

    unsafe fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        if self.peer.get() == other {
            self.peer.set(ptr::null_mut());
        }
        kResultOk
    }

    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        let drain_result = self.drain_worker_results();
        if drain_result != kResultOk {
            return drain_result;
        }
        let schedule_result = self.schedule_deferred_jobs();
        if schedule_result != kResultOk {
            return schedule_result;
        }

        let message = match GlirdirPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };

        match message {
            GlirdirPluginMessage::ArmCapture => self.arm_capture(),
            GlirdirPluginMessage::ClearScratchpad => self.clear_scratchpad(),
            GlirdirPluginMessage::FinalizeCaptureRequest => self.finalize_completed_capture(),
            GlirdirPluginMessage::PatchUpdate(payload) => self.apply_patch_payload(&payload),
            GlirdirPluginMessage::StatusRequest => {
                self.send_status_update(GlirdirStatusMessage::Status)
            }
            GlirdirPluginMessage::TelemetryRequest => {
                self.send_status_update(GlirdirStatusMessage::Telemetry)
            }
            GlirdirPluginMessage::MidiExportRequest => self.schedule_midi_export(),
            GlirdirPluginMessage::AnalysisStatusResponse(_)
            | GlirdirPluginMessage::MidiExportResponse(_)
            | GlirdirPluginMessage::StatusResponse(_)
            | GlirdirPluginMessage::TelemetryResponse(_) => kNotImplemented,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlirdirStatusMessage {
    Analysis,
    Status,
    Telemetry,
}

fn request_pending_analysis_job(plugin: &mut Glirdir) -> Option<crate::AnalysisJob> {
    (plugin.analysis_status() == AnalysisStatus::CapturedPendingAnalysis)
        .then(|| plugin.request_analysis_job())
        .flatten()
}

fn input_arrangement_supported(arrangement: SpeakerArrangement) -> bool {
    matches!(arrangement, SpeakerArr::kMono | SpeakerArr::kStereo)
}

fn fill_bus_info(
    bus: &mut BusInfo,
    media_type: MediaType,
    direction: BusDirection,
    channel_count: i32,
    name: &str,
) {
    bus.mediaType = media_type;
    bus.direction = direction;
    bus.channelCount = channel_count;
    copy_wstring(name, &mut bus.name);
    bus.busType = BusTypes_::kMain as BusType;
    bus.flags = BusInfo_::BusFlags_::kDefaultActive;
}
