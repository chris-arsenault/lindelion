#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    cell::{Cell, RefCell},
    ffi::{CString as StdCString, c_char, c_void},
    mem::MaybeUninit,
    ptr, slice,
};

use ahara_plugin_shell::{
    AudioBuffer, AudioPlugin, ControlEvent, MidiEvent, NoteEvent, ParameterId, PluginState,
    ProcessContext as ShellProcessContext, ProcessMode, ProcessSetup as ShellProcessSetup,
};
use vst3::{Class, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{DESCRIPTOR, PARAMETERS, ResonatorSynth, patch_io};

const MAX_BLOCK_EVENTS: usize = 128;
const STATE_MAGIC: [u8; 4] = *b"AHRS";
const STATE_HEADER_BYTES: usize = 12;
const MAX_STATE_BYTES: usize = 1_048_576;
const SUBCATEGORY: &str = "Instrument|Synth";

struct ResonatorVst3Processor {
    synth: RefCell<ResonatorSynth>,
    setup: Cell<ShellProcessSetup>,
}

impl Class for ResonatorVst3Processor {
    type Interfaces = (IComponent, IAudioProcessor, IProcessContextRequirements);
}

impl ResonatorVst3Processor {
    const CID: TUID = uid(0x4B410E03, 0x80AD49B6, 0x9B7D5479, 0xF4A9B0D1);

    fn new() -> Self {
        let setup = ShellProcessSetup::default();
        let mut synth = ResonatorSynth::default();
        synth.reset(setup);
        Self {
            synth: RefCell::new(synth),
            setup: Cell::new(setup),
        }
    }

    fn process_events(&self, input_events: *mut IEventList, events: &mut [MidiEvent]) -> usize {
        let Some(input_events) = (unsafe { ComRef::from_raw(input_events) }) else {
            return 0;
        };
        let event_count = unsafe { input_events.getEventCount() }.max(0) as usize;
        let mut used = 0;

        for index in 0..event_count.min(events.len()) {
            let mut event = MaybeUninit::<Event>::uninit();
            let result = unsafe { input_events.getEvent(index as i32, event.as_mut_ptr()) };
            if result == kResultOk
                && let Some(midi_event) = unsafe { vst_event_to_midi(event.assume_init()) }
            {
                events[used] = midi_event;
                used += 1;
            }
        }

        used
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
                synth.set_parameter_normalized(
                    ParameterId(unsafe { queue.getParameterId() }),
                    value as f32,
                );
            }
        }
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
            clear_outputs(data);
            return kResultOk;
        }

        let input_events = data.inputEvents;
        let Some((left, right)) = stereo_output_buffers(data) else {
            clear_outputs(data);
            return kResultOk;
        };
        let mut events = [empty_midi_event(); MAX_BLOCK_EVENTS];
        let event_count = self.process_events(input_events, &mut events);

        let Ok(mut synth) = self.synth.try_borrow_mut() else {
            left.fill(0.0);
            right.fill(0.0);
            return kResultFalse;
        };
        synth.process(ShellProcessContext {
            setup: self.setup.get(),
            buffer: AudioBuffer { left, right },
            events: &events[..event_count],
        });

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

struct ResonatorVst3Controller {
    values: Cell<[f64; PARAMETERS.len()]>,
    handler: Cell<*mut IComponentHandler>,
}

impl Class for ResonatorVst3Controller {
    type Interfaces = (IEditController,);
}

impl ResonatorVst3Controller {
    const CID: TUID = uid(0x15C8B012, 0xF4B64F5E, 0x93D9AA38, 0x69383E3B);

    fn new() -> Self {
        Self {
            values: Cell::new(default_parameter_values()),
            handler: Cell::new(ptr::null_mut()),
        }
    }

    fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameter_index(id) else {
            return kInvalidArgument;
        };
        let mut values = self.values.get();
        values[index] = normalized.clamp(0.0, 1.0);
        self.values.set(values);
        kResultOk
    }
}

impl IPluginBaseTrait for ResonatorVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for ResonatorVst3Controller {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultFalse;
        };
        let Ok(payload) = std::str::from_utf8(&plugin_state.payload) else {
            return kResultFalse;
        };
        let Ok(patch) = patch_io::from_toml_str(payload) else {
            return kResultFalse;
        };

        self.set_value(
            1,
            normalized_parameter_value(1, patch.output.master_gain_db),
        );
        self.set_value(2, normalized_parameter_value(2, patch_loop_gain(&patch)));
        self.set_value(3, normalized_parameter_value(3, patch.output.filter_cutoff));
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        PARAMETERS.len() as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(parameter) = PARAMETERS.get(param_index as usize) else {
            return kInvalidArgument;
        };

        let info = &mut *info;
        info.id = parameter.id.0;
        copy_wstring(parameter.name, &mut info.title);
        copy_wstring(parameter.name, &mut info.shortTitle);
        copy_wstring(parameter.units, &mut info.units);
        info.stepCount = parameter.step_count.map_or(0, |steps| steps as i32);
        info.defaultNormalizedValue = parameter.range.normalize(parameter.range.default) as f64;
        info.unitId = 0;
        info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate;
        kResultOk
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value_normalized: f64,
        string: *mut String128,
    ) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };

        let plain = parameter.range.denormalize(value_normalized as f32);
        copy_wstring(&format_plain_value(plain), &mut *string);
        kResultOk
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value_normalized: *mut f64,
    ) -> tresult {
        if string.is_null() || value_normalized.is_null() {
            return kInvalidArgument;
        }
        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };

        let len = len_wstring(string as *const TChar);
        let Ok(text) = String::from_utf16(slice::from_raw_parts(string as *const u16, len)) else {
            return kInvalidArgument;
        };
        let Ok(value) = text.trim().parse::<f32>() else {
            return kInvalidArgument;
        };

        *value_normalized = parameter.range.normalize(value) as f64;
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        parameter_by_id(id)
            .map(|parameter| parameter.range.denormalize(value_normalized as f32) as f64)
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        normalized_parameter_value(id, plain_value as f32)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        let Some(index) = parameter_index(id) else {
            return 0.0;
        };
        self.values.get()[index]
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        self.set_value(id, value)
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        self.handler.set(handler);
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        ptr::null_mut()
    }
}

struct Factory;

impl Class for Factory {
    type Interfaces = (IPluginFactory, IPluginFactory2, IPluginFactory3);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        copy_cstring(DESCRIPTOR.vendor, &mut info.vendor);
        copy_cstring(DESCRIPTOR.url, &mut info.url);
        copy_cstring(DESCRIPTOR.email, &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as i32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        2
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = FactoryClass::from_index(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info(&mut *info);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }

        let instance = match *(cid as *const TUID) {
            ResonatorVst3Processor::CID => ComWrapper::new(ResonatorVst3Processor::new())
                .to_com_ptr::<FUnknown>()
                .unwrap(),
            ResonatorVst3Controller::CID => ComWrapper::new(ResonatorVst3Controller::new())
                .to_com_ptr::<FUnknown>()
                .unwrap(),
            _ => return kInvalidArgument,
        };
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

impl IPluginFactory2Trait for Factory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = FactoryClass::from_index(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info2(&mut *info);
        kResultOk
    }
}

impl IPluginFactory3Trait for Factory {
    unsafe fn getClassInfoUnicode(&self, index: i32, info: *mut PClassInfoW) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = FactoryClass::from_index(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info_w(&mut *info);
        kResultOk
    }

    unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
}

#[derive(Debug, Clone, Copy)]
enum FactoryClass {
    Processor,
    Controller,
}

impl FactoryClass {
    fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Processor),
            1 => Some(Self::Controller),
            _ => None,
        }
    }

    fn cid(self) -> TUID {
        match self {
            Self::Processor => ResonatorVst3Processor::CID,
            Self::Controller => ResonatorVst3Controller::CID,
        }
    }

    fn category(self) -> &'static str {
        match self {
            Self::Processor => "Audio Module Class",
            Self::Controller => "Component Controller Class",
        }
    }

    fn name(self) -> &'static str {
        DESCRIPTOR.name
    }

    fn fill_class_info(self, info: &mut PClassInfo) {
        info.cid = self.cid();
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category(), &mut info.category);
        copy_cstring(self.name(), &mut info.name);
    }

    fn fill_class_info2(self, info: &mut PClassInfo2) {
        info.cid = self.cid();
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category(), &mut info.category);
        copy_cstring(self.name(), &mut info.name);
        info.classFlags = 0;
        copy_cstring(SUBCATEGORY, &mut info.subCategories);
        copy_cstring(DESCRIPTOR.vendor, &mut info.vendor);
        copy_cstring(DESCRIPTOR.version, &mut info.version);
        copy_cstring("VST 3.8.0", &mut info.sdkVersion);
    }

    fn fill_class_info_w(self, info: &mut PClassInfoW) {
        info.cid = self.cid();
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category(), &mut info.category);
        copy_wstring(self.name(), &mut info.name);
        info.classFlags = 0;
        copy_cstring(SUBCATEGORY, &mut info.subCategories);
        copy_wstring(DESCRIPTOR.vendor, &mut info.vendor);
        copy_wstring(DESCRIPTOR.version, &mut info.version);
        copy_wstring("VST 3.8.0", &mut info.sdkVersion);
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

unsafe fn stereo_output_buffers(data: &mut ProcessData) -> Option<(&mut [f32], &mut [f32])> {
    if data.numSamples <= 0 || data.numOutputs != 1 || data.outputs.is_null() {
        return None;
    }
    let outputs = slice::from_raw_parts_mut(data.outputs, data.numOutputs as usize);
    if outputs[0].numChannels != 2 {
        return None;
    }

    let channel_buffers = outputs[0].__field0.channelBuffers32;
    if channel_buffers.is_null() {
        return None;
    }
    let channels = slice::from_raw_parts_mut(channel_buffers, 2);
    if channels[0].is_null() || channels[1].is_null() {
        return None;
    }

    let sample_count = data.numSamples as usize;
    Some((
        slice::from_raw_parts_mut(channels[0], sample_count),
        slice::from_raw_parts_mut(channels[1], sample_count),
    ))
}

unsafe fn clear_outputs(data: &mut ProcessData) {
    if data.numSamples <= 0 || data.numOutputs <= 0 || data.outputs.is_null() {
        return;
    }
    let outputs = slice::from_raw_parts_mut(data.outputs, data.numOutputs as usize);
    for output in outputs {
        clear_output_bus(output, data.numSamples as usize);
    }
}

unsafe fn clear_output_bus(output: &mut AudioBusBuffers, sample_count: usize) {
    if output.numChannels <= 0 || output.__field0.channelBuffers32.is_null() {
        return;
    }
    let channels = slice::from_raw_parts_mut(
        output.__field0.channelBuffers32,
        output.numChannels as usize,
    );
    for channel in channels {
        if !channel.is_null() {
            slice::from_raw_parts_mut(*channel, sample_count).fill(0.0);
        }
    }
}

const fn empty_midi_event() -> MidiEvent {
    MidiEvent::Control(ControlEvent::ContinuousController {
        channel: 0,
        controller: 0,
        value: 0.0,
    })
}

unsafe fn vst_event_to_midi(event: Event) -> Option<MidiEvent> {
    match event.r#type as Event_::EventTypes {
        Event_::EventTypes_::kNoteOnEvent => {
            let note = event.__field0.noteOn;
            Some(MidiEvent::Note(NoteEvent::On {
                channel: note.channel.clamp(0, 15) as u8,
                note: note.pitch.clamp(0, 127) as u8,
                velocity: note.velocity.clamp(0.0, 1.0),
            }))
        }
        Event_::EventTypes_::kNoteOffEvent => {
            let note = event.__field0.noteOff;
            Some(MidiEvent::Note(NoteEvent::Off {
                channel: note.channel.clamp(0, 15) as u8,
                note: note.pitch.clamp(0, 127) as u8,
                velocity: note.velocity.clamp(0.0, 1.0),
            }))
        }
        Event_::EventTypes_::kPolyPressureEvent => {
            let pressure = event.__field0.polyPressure;
            Some(MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: pressure.channel.clamp(0, 15) as u8,
                value: pressure.pressure.clamp(0.0, 1.0),
            }))
        }
        Event_::EventTypes_::kLegacyMIDICCOutEvent => {
            legacy_midi_cc_to_event(event.__field0.midiCCOut)
        }
        _ => None,
    }
}

fn legacy_midi_cc_to_event(event: LegacyMIDICCOutEvent) -> Option<MidiEvent> {
    let channel = event.channel.clamp(0, 15) as u8;
    match u32::from(event.controlNumber) {
        ControllerNumbers_::kCtrlModWheel => {
            Some(MidiEvent::Control(ControlEvent::ContinuousController {
                channel,
                controller: 1,
                value: midi7(event.value),
            }))
        }
        ControllerNumbers_::kCtrlFilterResonance => {
            Some(MidiEvent::Control(ControlEvent::ContinuousController {
                channel,
                controller: 74,
                value: midi7(event.value),
            }))
        }
        ControllerNumbers_::kAfterTouch => {
            Some(MidiEvent::Control(ControlEvent::ChannelPressure {
                channel,
                value: midi7(event.value),
            }))
        }
        ControllerNumbers_::kPitchBend => Some(MidiEvent::Control(ControlEvent::PitchBend {
            channel,
            semitones: pitch_bend_semitones(event.value, event.value2),
        })),
        _ => None,
    }
}

fn midi7(value: i8) -> f32 {
    f32::from(value.clamp(0, 127)) / 127.0
}

fn pitch_bend_semitones(lsb: i8, msb: i8) -> f32 {
    let raw = i32::from(lsb.clamp(0, 127)) | (i32::from(msb.clamp(0, 127)) << 7);
    ((raw as f32 - 8_192.0) / 8_192.0).clamp(-1.0, 1.0) * 2.0
}

fn default_parameter_values() -> [f64; PARAMETERS.len()] {
    let mut values = [0.0; PARAMETERS.len()];
    for (index, parameter) in PARAMETERS.iter().enumerate() {
        values[index] = parameter.range.normalize(parameter.range.default) as f64;
    }
    values
}

fn parameter_index(id: u32) -> Option<usize> {
    PARAMETERS
        .iter()
        .position(|parameter| parameter.id == ParameterId(id))
}

fn parameter_by_id(id: u32) -> Option<&'static ahara_plugin_shell::ParameterInfo> {
    PARAMETERS
        .iter()
        .find(|parameter| parameter.id == ParameterId(id))
}

fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    parameter_by_id(id)
        .map(|parameter| parameter.range.normalize(plain) as f64)
        .unwrap_or(0.0)
}

fn patch_loop_gain(patch: &crate::ResonatorSynthPatch) -> f32 {
    match (patch.resonator_a, patch.resonator_b) {
        (crate::ResonatorConfig::Waveguide(config), _) => config.loop_gain,
        (_, crate::ResonatorConfig::Waveguide(config)) => config.loop_gain,
        _ => 0.92,
    }
}

fn format_plain_value(value: f32) -> String {
    if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn copy_cstring(src: &str, dst: &mut [c_char]) {
    let c_string = StdCString::new(src).unwrap_or_default();
    let bytes = c_string.as_bytes_with_nul();

    for (src, dst) in bytes.iter().zip(dst.iter_mut()) {
        *dst = *src as c_char;
    }

    if bytes.len() > dst.len()
        && let Some(last) = dst.last_mut()
    {
        *last = 0;
    }
}

fn copy_wstring(src: &str, dst: &mut [TChar]) {
    let mut len = 0;
    for (src, dst) in src.encode_utf16().zip(dst.iter_mut()) {
        *dst = src as TChar;
        len += 1;
    }

    if len < dst.len() {
        dst[len] = 0;
    } else if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

unsafe fn len_wstring(string: *const TChar) -> usize {
    let mut len = 0;
    while *string.add(len) != 0 {
        len += 1;
    }
    len
}

unsafe fn read_plugin_state_from_stream(stream: *mut IBStream) -> Option<PluginState> {
    let stream = ComRef::from_raw(stream)?;
    let mut header = [0; STATE_HEADER_BYTES];
    if !read_exact(&stream, &mut header) {
        return None;
    }
    if header[..4] != STATE_MAGIC {
        return None;
    }

    let format_version = u32::from_le_bytes(header[4..8].try_into().ok()?);
    let payload_len = u32::from_le_bytes(header[8..12].try_into().ok()?) as usize;
    if payload_len > MAX_STATE_BYTES {
        return None;
    }

    let mut payload = vec![0; payload_len];
    if !read_exact(&stream, &mut payload) {
        return None;
    }
    Some(PluginState {
        format_version,
        payload,
    })
}

unsafe fn write_plugin_state_to_stream(stream: *mut IBStream, state: PluginState) -> bool {
    let Some(stream) = ComRef::from_raw(stream) else {
        return false;
    };
    if state.payload.len() > u32::MAX as usize {
        return false;
    }

    let mut header = [0; STATE_HEADER_BYTES];
    header[..4].copy_from_slice(&STATE_MAGIC);
    header[4..8].copy_from_slice(&state.format_version.to_le_bytes());
    header[8..12].copy_from_slice(&(state.payload.len() as u32).to_le_bytes());

    write_all(&stream, &header) && write_all(&stream, &state.payload)
}

unsafe fn read_exact(stream: &ComRef<IBStream>, buffer: &mut [u8]) -> bool {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut bytes_read = 0;
        let chunk_len = (buffer.len() - offset).min(i32::MAX as usize) as i32;
        let result = stream.read(
            buffer[offset..].as_mut_ptr().cast::<c_void>(),
            chunk_len,
            &mut bytes_read,
        );
        if result != kResultOk || bytes_read <= 0 {
            return false;
        }
        offset += bytes_read as usize;
    }
    true
}

unsafe fn write_all(stream: &ComRef<IBStream>, buffer: &[u8]) -> bool {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut bytes_written = 0;
        let chunk_len = (buffer.len() - offset).min(i32::MAX as usize) as i32;
        let result = stream.write(
            buffer[offset..].as_ptr().cast::<c_void>() as *mut c_void,
            chunk_len,
            &mut bytes_written,
        );
        if result != kResultOk || bytes_written <= 0 {
            return false;
        }
        offset += bytes_written as usize;
    }
    true
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "system" fn InitDll() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "system" fn ExitDll() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "system" fn BundleEntry(_bundle_ref: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "system" fn BundleExit() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "system" fn ModuleEntry(_library_handle: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "system" fn ModuleExit() -> bool {
    true
}

#[unsafe(no_mangle)]
pub extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory)
        .to_com_ptr::<IPluginFactory>()
        .unwrap()
        .into_raw()
}
