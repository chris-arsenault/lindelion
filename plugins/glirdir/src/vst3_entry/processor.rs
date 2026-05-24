use std::cell::{Cell, RefCell};

use lindelion_plugin_shell::{
    AudioPlugin, ParameterId, ProcessContext as ShellProcessContext,
    ProcessSetup as ShellProcessSetup,
    vst3::{
        Vst3BusInfo, Vst3PeerConnection, audio_input_buffer_from_vst_process_data,
        can_process_32_bit_sample_size, clear_vst_outputs, fill_vst3_bus_info,
        for_each_vst3_parameter_change, process_setup_from_vst, read_plugin_state_from_stream,
        stereo_output_buffers_from_vst_process_data, transport_context_from_vst_process_context,
        vst3_bus_count, write_plugin_state_to_stream,
    },
};
use vst3::{Class, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    AnalysisStatus, Glirdir, GlirdirWorker, GlirdirWorkerQueue, GlirdirWorkerResult,
    ParameterApplyKind,
    midi_export::MidiExportJob,
    sample_library::{SampleLibrarySaveJob, SampleLibrarySavePayload},
};

use super::{GlirdirPluginMessage, GlirdirStatusPayload, GlirdirVst3Controller};

const GLIRDIR_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_input(2, "Input"),
    Vst3BusInfo::audio_output(2, "Output"),
];

pub(super) struct GlirdirVst3Processor {
    pub(super) plugin: RefCell<Glirdir>,
    worker: RefCell<Box<dyn GlirdirWorkerQueue>>,
    pending_requantize: Cell<bool>,
    setup: Cell<ShellProcessSetup>,
    input_arrangement: Cell<SpeakerArrangement>,
    peer: Vst3PeerConnection,
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
    pub(super) const CID: TUID = uid(0x7C2E2B8A, 0xB1C44F0D, 0xA6F92427, 0x6C9E0D5B);

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
            peer: Vst3PeerConnection::new(),
        }
    }

    fn apply_parameter_changes(&self, changes: *mut IParameterChanges) {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return;
        };

        unsafe {
            for_each_vst3_parameter_change(changes, |change| {
                let apply = plugin.set_parameter_normalized_deferred(
                    ParameterId(change.id),
                    change.normalized_value as f32,
                );
                self.track_deferred_parameter_apply(apply);
            });
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

    fn update_plugin(&self, update: fn(&mut Glirdir)) -> tresult {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        update(&mut plugin);
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
}

include!("processor_jobs.rs");

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
        vst3_bus_count(&GLIRDIR_BUSES, media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        fill_vst3_bus_info(&GLIRDIR_BUSES, media_type, dir, index, bus)
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

        let speaker_arrangement = match dir as BusDirections {
            BusDirections_::kInput => self.input_arrangement.get(),
            BusDirections_::kOutput => SpeakerArr::kStereo,
            _ => return kInvalidArgument,
        };
        *arrangement = speaker_arrangement;
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
            GlirdirPluginMessage::PlayAudition => self.update_plugin(Glirdir::play_audition),
            GlirdirPluginMessage::StopAudition => self.update_plugin(Glirdir::stop_audition),
            GlirdirPluginMessage::ToggleAuditionLoop => {
                self.update_plugin(Glirdir::toggle_audition_loop)
            }
            GlirdirPluginMessage::ToggleAuditionLiveEdit => {
                self.update_plugin(Glirdir::toggle_audition_live_edit)
            }
            GlirdirPluginMessage::SampleLibrarySaveRequest => self.schedule_sample_library_save(),
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
            | GlirdirPluginMessage::SampleLibrarySaveResponse(_)
            | GlirdirPluginMessage::StatusResponse(_)
            | GlirdirPluginMessage::TelemetryResponse(_) => kNotImplemented,
        }
    }
}

fn input_arrangement_supported(arrangement: SpeakerArrangement) -> bool {
    matches!(arrangement, SpeakerArr::kMono | SpeakerArr::kStereo)
}
