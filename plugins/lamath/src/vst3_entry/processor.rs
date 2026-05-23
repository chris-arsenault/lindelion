use std::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
    ptr,
};

use lindelion_plugin_shell::{
    AudioPlugin, MidiEvent, MidiEventNormalizer, ParameterId,
    ProcessContext as ShellProcessContext, ProcessMode, ProcessSetup as ShellProcessSetup,
    vst3::{clear_vst_outputs, copy_wstring, stereo_output_buffers_from_vst_process_data},
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{ResonatorSynth, patch_io};

use super::{
    DEFAULT_PITCH_BEND_RANGE_SEMITONES, MAX_BLOCK_EVENTS, PITCH_BEND_PARAMETER_ID,
    RESONATOR_MIDI_CONTROLLER_ROUTES, ResonatorPluginMessage, ResonatorVst3Controller,
    empty_midi_event, encode_telemetry, read_plugin_state_from_stream, vst_event_to_midi,
    write_plugin_state_to_stream,
};

pub(super) struct ResonatorVst3Processor {
    pub(super) synth: RefCell<ResonatorSynth>,
    setup: Cell<ShellProcessSetup>,
    peer: Cell<*mut IConnectionPoint>,
}

impl Class for ResonatorVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IConnectionPoint,
    );
}

impl ResonatorVst3Processor {
    pub(super) const CID: TUID = uid(0x4B410E03, 0x80AD49B6, 0x9B7D5479, 0xF4A9B0D1);

    pub(super) fn new() -> Self {
        let setup = ShellProcessSetup::default();
        let mut synth = ResonatorSynth::default();
        synth.reset(setup);
        Self {
            synth: RefCell::new(synth),
            setup: Cell::new(setup),
            peer: Cell::new(ptr::null_mut()),
        }
    }

    fn process_events(&self, input_events: *mut IEventList, events: &mut [MidiEvent]) -> usize {
        let Some(input_events) = (unsafe { ComRef::from_raw(input_events) }) else {
            return 0;
        };
        let event_count = unsafe { input_events.getEventCount() }.max(0) as usize;
        let normalizer = self.midi_event_normalizer();
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

    fn midi_event_normalizer(&self) -> MidiEventNormalizer<'static> {
        let pitch_bend_range = self
            .synth
            .try_borrow()
            .map(|synth| synth.patch().modulation.pitch_bend_range_semitones)
            .unwrap_or(DEFAULT_PITCH_BEND_RANGE_SEMITONES);
        MidiEventNormalizer::new(RESONATOR_MIDI_CONTROLLER_ROUTES, pitch_bend_range)
    }

    fn apply_parameter_changes(&self, changes: *mut IParameterChanges) {
        let Some(changes) = (unsafe { ComRef::from_raw(changes) }) else {
            return;
        };

        let Ok(mut synth) = self.synth.try_borrow_mut() else {
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
                if parameter_id == PITCH_BEND_PARAMETER_ID {
                    synth.set_pitch_bend_normalized(value as f32);
                } else {
                    synth.set_parameter_normalized(ParameterId(parameter_id), value as f32);
                }
            }
        }
    }

    fn apply_patch_payload(&self, payload: &[u8]) -> tresult {
        let Ok(text) = std::str::from_utf8(payload) else {
            return kResultFalse;
        };
        let Ok(patch) = patch_io::from_toml_str(text) else {
            return kResultFalse;
        };
        let Ok(mut synth) = self.synth.try_borrow_mut() else {
            return kResultFalse;
        };
        synth.load_patch_from_sample_paths(patch);
        kResultOk
    }

    fn send_telemetry_response(&self) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultFalse;
        };
        let Ok(synth) = self.synth.try_borrow() else {
            return kResultFalse;
        };
        let payload = encode_telemetry(synth.telemetry()).into_bytes();
        let message = ResonatorPluginMessage::telemetry_response(payload).into_com_message();
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }
}

impl IPluginBaseTrait for ResonatorVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for ResonatorVst3Processor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        *class_id = ResonatorVst3Controller::CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kInput) => 0,
            (MediaTypes_::kAudio, BusDirections_::kOutput) => 1,
            (MediaTypes_::kEvent, BusDirections_::kInput) => 1,
            (MediaTypes_::kEvent, BusDirections_::kOutput) => 0,
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
            (MediaTypes_::kAudio, BusDirections_::kOutput) => {
                fill_bus_info(&mut *bus, media_type, dir, 2, "Output");
                kResultOk
            }
            (MediaTypes_::kEvent, BusDirections_::kInput) => {
                fill_bus_info(&mut *bus, media_type, dir, 1, "MIDI Input");
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
        let Ok(mut synth) = self.synth.try_borrow_mut() else {
            return kResultFalse;
        };
        synth.load_state(plugin_state);
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Ok(synth) = self.synth.try_borrow() else {
            return kResultFalse;
        };
        if write_plugin_state_to_stream(state, synth.state()) {
            kResultOk
        } else {
            kResultFalse
        }
    }
}

impl IAudioProcessorTrait for ResonatorVst3Processor {
    unsafe fn setBusArrangements(
        &self,
        _inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins != 0 || num_outs != 1 || outputs.is_null() {
            return kResultFalse;
        }
        if *outputs == SpeakerArr::kStereo {
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

        let Ok(mut synth) = self.synth.try_borrow_mut() else {
            return kResultFalse;
        };
        synth.reset(shell_setup);
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

        let input_events = data.inputEvents;
        let Some(mut buffer) = stereo_output_buffers_from_vst_process_data(data) else {
            clear_vst_outputs(data);
            return kResultOk;
        };
        let mut events = [empty_midi_event(); MAX_BLOCK_EVENTS];
        let event_count = self.process_events(input_events, &mut events);

        let Ok(mut synth) = self.synth.try_borrow_mut() else {
            buffer.clear();
            return kResultFalse;
        };
        synth.process(ShellProcessContext::new(
            self.setup.get(),
            buffer,
            &events[..event_count],
        ));

        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for ResonatorVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

impl IConnectionPointTrait for ResonatorVst3Processor {
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
        let message = match ResonatorPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };

        match message {
            ResonatorPluginMessage::PatchUpdate(payload) => self.apply_patch_payload(&payload),
            ResonatorPluginMessage::TelemetryRequest => self.send_telemetry_response(),
            ResonatorPluginMessage::TelemetryResponse(_) => kNotImplemented,
        }
    }
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
