//! Test-only fixture plugin: a bit-exact passthrough `IComponent`+`IAudioProcessor` behind a
//! one-class `IPluginFactory`. Built directly on `vst3` (no `lindelion-plugin-shell` helpers) so the
//! host's protocol driver can be exercised in-process on Linux. Mirrors the passthrough shape of
//! `plugins/cenedril/src/vst3_entry/processor.rs`.

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_char, c_void};
use std::ptr;
use std::slice;

use vst3::{Class, ComPtr, ComRef, ComWrapper, Interface, Steinberg::Vst::*, Steinberg::*, uid};

// VST3 `IStreamSeekMode` values used by the fixture's get/setState.
const SEEK_SET: i32 = 0;
const SEEK_END: i32 = 2;

/// Fixed class id for the fixture audio-processor class (unique within this fixture factory).
pub(super) const FIXTURE_CID: TUID = uid(0x5A11D000, 0x11114444, 0x22228888, 0x3333CCCC);

/// Fixed class id for the fixture edit-controller class.
pub(super) const FIXTURE_CONTROLLER_CID: TUID = uid(0x5A11D001, 0x55556666, 0x77778888, 0x9999AAAA);

/// Fixed class id for the fixture single-component audio/controller class.
pub(super) const SINGLE_COMPONENT_CID: TUID = uid(0x5A11D002, 0x22224444, 0x66668888, 0xAAAACCCC);

/// How a [`FixtureProcessor`] behaves in `process`, so tests can model a misbehaving plugin (M7).
/// A plugin that *panics*/aborts/hangs in foreign code is out of in-process reach (see Step 5); these
/// are the failure modes the host can actually contain.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FixtureBehavior {
    /// Bit-exact gain passthrough (the well-behaved fixture).
    Passthrough,
    /// `process` returns an error result (`kResultFalse`).
    ProcessError,
    /// `process` writes non-finite samples (NaN) into the output.
    NaNOutput,
    /// `process` rejects calls that omit declared audio busses.
    RequiresDeclaredAudioBuses,
    /// `process` rejects calls without a valid playing process context.
    RequiresProcessContext,
    /// `process` emits a signal only when a live note-on event is present.
    MidiNoteTriggersOutput,
    /// `process` emits the latest normalized value of parameter id 1.
    ParameterControlsOutput,
}

/// Bus-shape variations used to exercise host negotiation against common real-world VST3 patterns.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FixtureBusShape {
    /// One stereo input bus and one stereo output bus.
    Stereo,
    /// Main stereo input plus an additional inactive stereo input bus, like plugins with sidechain I/O.
    StereoWithExtraInput,
    /// Reports fixed stereo I/O but rejects `setBusArrangements`, which a host must tolerate by
    /// falling back to the reported arrangement.
    FixedStereoRejectsSet,
}

/// A stereo processor that scales by `gain` and reports `latency` (gain 1.0 / latency 0 = verbatim
/// passthrough, as M1 used). `state` is an opaque blob echoed through get/setState, so the host's
/// state capture/restore is observable. `behavior` lets a fixture misbehave (M7 containment tests).
pub(super) struct FixtureProcessor {
    gain: f32,
    latency: u32,
    behavior: FixtureBehavior,
    bus_shape: FixtureBusShape,
    state: RefCell<Vec<u8>>,
}

impl FixtureProcessor {
    pub(super) fn with_behavior(gain: f32, latency: u32, behavior: FixtureBehavior) -> Self {
        Self::with_shape(gain, latency, behavior, FixtureBusShape::Stereo)
    }

    pub(super) fn with_shape(
        gain: f32,
        latency: u32,
        behavior: FixtureBehavior,
        bus_shape: FixtureBusShape,
    ) -> Self {
        FixtureProcessor {
            gain,
            latency,
            behavior,
            bus_shape,
            state: RefCell::new(Vec::new()),
        }
    }
}

impl Class for FixtureProcessor {
    type Interfaces = (IComponent, IAudioProcessor);
}

impl IPluginBaseTrait for FixtureProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for FixtureProcessor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        *class_id = FIXTURE_CONTROLLER_CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        if media_type == MediaTypes_::kEvent as MediaType {
            return if dir == BusDirections_::kInput as BusDirection
                && self.behavior == FixtureBehavior::MidiNoteTriggersOutput
            {
                1
            } else {
                0
            };
        }
        if media_type != MediaTypes_::kAudio as MediaType {
            return 0;
        }
        if dir == BusDirections_::kInput as BusDirection {
            match self.bus_shape {
                FixtureBusShape::StereoWithExtraInput => 2,
                _ => 1,
            }
        } else if dir == BusDirections_::kOutput as BusDirection {
            1
        } else {
            0
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        if bus.is_null() {
            return kInvalidArgument;
        }
        let count = self.getBusCount(media_type, dir);
        if index < 0 || index >= count {
            return kInvalidArgument;
        }
        let bus = &mut *bus;
        if media_type == MediaTypes_::kEvent as MediaType {
            bus.mediaType = MediaTypes_::kEvent as MediaType;
            bus.direction = dir;
            bus.channelCount = 1;
            fill_utf16(&mut bus.name, "MIDI Input");
            bus.busType = BusTypes_::kMain as BusType;
            bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;
            return kResultOk;
        }
        if media_type != MediaTypes_::kAudio as MediaType {
            return kInvalidArgument;
        }
        bus.mediaType = MediaTypes_::kAudio as MediaType;
        bus.direction = dir;
        bus.channelCount = 2;
        let is_main = index == 0;
        let name = if dir == BusDirections_::kInput as BusDirection && is_main {
            "Input"
        } else if dir == BusDirections_::kInput as BusDirection {
            "Sidechain"
        } else {
            "Output"
        };
        fill_utf16(&mut bus.name, name);
        bus.busType = if is_main {
            BusTypes_::kMain as BusType
        } else {
            BusTypes_::kAux as BusType
        };
        bus.flags = if is_main {
            BusInfo_::BusFlags_::kDefaultActive as u32
        } else {
            0
        };
        kResultOk
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
        let Some(stream) = ComRef::from_raw(state) else {
            return kInvalidArgument;
        };
        let mut end: i64 = 0;
        stream.seek(0, SEEK_END, &mut end);
        stream.seek(0, SEEK_SET, ptr::null_mut());
        let len = end.max(0) as usize;
        let mut buffer = vec![0u8; len];
        let mut read: i32 = 0;
        if len > 0 {
            stream.read(buffer.as_mut_ptr() as *mut c_void, len as i32, &mut read);
        }
        buffer.truncate(read.max(0) as usize);
        *self.state.borrow_mut() = buffer;
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(state) else {
            return kInvalidArgument;
        };
        let bytes = self.state.borrow();
        let mut written: i32 = 0;
        if !bytes.is_empty() {
            stream.write(
                bytes.as_ptr() as *mut c_void,
                bytes.len() as i32,
                &mut written,
            );
        }
        kResultOk
    }
}

impl IAudioProcessorTrait for FixtureProcessor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if self.bus_shape == FixtureBusShape::FixedStereoRejectsSet {
            return kResultFalse;
        }
        let expected_ins = match self.bus_shape {
            FixtureBusShape::StereoWithExtraInput => 2,
            _ => 1,
        };
        if inputs.is_null() || outputs.is_null() || num_ins != expected_ins || num_outs != 1 {
            return kResultFalse;
        }
        let input_arrangements = slice::from_raw_parts(inputs, num_ins as usize);
        if input_arrangements
            .iter()
            .all(|arr| *arr == SpeakerArr::kStereo)
            && *outputs == SpeakerArr::kStereo
        {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        _dir: BusDirection,
        index: i32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        if arrangement.is_null() {
            return kInvalidArgument;
        }
        let count = self.getBusCount(MediaTypes_::kAudio as MediaType, _dir);
        if index < 0 || index >= count {
            return kInvalidArgument;
        }
        *arrangement = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        if symbolic_sample_size == SymbolicSampleSizes_::kSample32 as i32 {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        self.latency
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        if self.behavior == FixtureBehavior::ProcessError {
            return kResultFalse;
        }
        let data = &mut *data;
        if self.behavior == FixtureBehavior::RequiresDeclaredAudioBuses
            && (data.numInputs
                != self.getBusCount(
                    MediaTypes_::kAudio as MediaType,
                    BusDirections_::kInput as BusDirection,
                )
                || data.numOutputs
                    != self.getBusCount(
                        MediaTypes_::kAudio as MediaType,
                        BusDirections_::kOutput as BusDirection,
                    ))
        {
            return kResultFalse;
        }
        if self.behavior == FixtureBehavior::RequiresProcessContext {
            if data.processContext.is_null() {
                return kResultFalse;
            }
            let context = &*data.processContext;
            if context.sampleRate <= 0.0
                || context.state & ProcessContext_::StatesAndFlags_::kPlaying as u32 == 0
            {
                return kResultFalse;
            }
        }
        if data.symbolicSampleSize != SymbolicSampleSizes_::kSample32 as i32 || data.numSamples <= 0
        {
            return kResultOk;
        }
        if self.behavior == FixtureBehavior::MidiNoteTriggersOutput {
            if has_note_on(data.inputEvents) {
                fill_output(data, 0.25);
            } else {
                fill_output(data, 0.0);
            }
            return kResultOk;
        }
        if self.behavior == FixtureBehavior::ParameterControlsOutput {
            fill_output(
                data,
                parameter_value(data.inputParameterChanges, 1).unwrap_or(0.0) as f32,
            );
            return kResultOk;
        }
        match self.behavior {
            FixtureBehavior::NaNOutput => fill_output(data, f32::NAN),
            _ => apply_gain(data, self.gain),
        }
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

unsafe fn has_note_on(input_events: *mut IEventList) -> bool {
    let Some(events) = ComRef::from_raw(input_events) else {
        return false;
    };
    let count = events.getEventCount().max(0);
    for index in 0..count {
        let mut event = std::mem::zeroed::<Event>();
        if events.getEvent(index, &mut event) == kResultOk
            && event.r#type == Event_::EventTypes_::kNoteOnEvent as u16
        {
            return true;
        }
    }
    false
}

unsafe fn parameter_value(
    input_changes: *mut IParameterChanges,
    id: ParamID,
) -> Option<ParamValue> {
    let changes = ComRef::from_raw(input_changes)?;
    for index in 0..changes.getParameterCount() {
        let Some(queue) = ComRef::from_raw(changes.getParameterData(index)) else {
            continue;
        };
        if queue.getParameterId() != id {
            continue;
        }
        let point_count = queue.getPointCount();
        if point_count <= 0 {
            continue;
        }
        let mut sample_offset = 0;
        let mut value = 0.0;
        if queue.getPoint(point_count - 1, &mut sample_offset, &mut value) == kResultTrue {
            return Some(value);
        }
    }
    None
}

/// Copy the first input bus into the first output bus, per channel, scaled by `gain` (1.0 = verbatim
/// passthrough); an absent input channel writes silence. Mirrors the `channelBuffers32` layout read in
/// `crates/lindelion-plugin-shell/src/vst3_process.rs`.
unsafe fn apply_gain(data: &mut ProcessData, gain: f32) {
    if data.numInputs < 1 || data.numOutputs < 1 || data.inputs.is_null() || data.outputs.is_null()
    {
        return;
    }
    let n = data.numSamples as usize;
    let input = &*data.inputs;
    let output = &mut *data.outputs;

    let out_bufs = output.__field0.channelBuffers32;
    if out_bufs.is_null() {
        return;
    }
    let out_channels = slice::from_raw_parts(out_bufs, output.numChannels.max(0) as usize);

    let in_bufs = input.__field0.channelBuffers32;
    let in_channels: &[*mut Sample32] = if in_bufs.is_null() {
        &[]
    } else {
        slice::from_raw_parts(in_bufs, input.numChannels.max(0) as usize)
    };

    for (c, &dst) in out_channels.iter().enumerate() {
        if dst.is_null() {
            continue;
        }
        let dst = slice::from_raw_parts_mut(dst, n);
        match in_channels.get(c) {
            Some(&src) if !src.is_null() => {
                let src = slice::from_raw_parts(src, n);
                for (out, &sample) in dst.iter_mut().zip(src) {
                    *out = sample * gain;
                }
            }
            _ => dst.fill(0.0),
        }
    }
}

/// Write `value` into every output channel (used to inject NaN for the misbehaving-plugin fixture).
unsafe fn fill_output(data: &mut ProcessData, value: f32) {
    if data.numOutputs < 1 || data.outputs.is_null() {
        return;
    }
    let n = data.numSamples as usize;
    let output = &mut *data.outputs;
    let out_bufs = output.__field0.channelBuffers32;
    if out_bufs.is_null() {
        return;
    }
    let out_channels = slice::from_raw_parts(out_bufs, output.numChannels.max(0) as usize);
    for &dst in out_channels {
        if dst.is_null() {
            continue;
        }
        slice::from_raw_parts_mut(dst, n).fill(value);
    }
}

/// A fixture edit controller whose `createView("editor")` returns a [`FixtureView`]; everything else
/// is a benign stub. Exposes the editor path so the host's M5 editor protocol is testable in-process.
pub(super) struct FixtureController;

impl FixtureController {
    pub(super) fn new() -> Self {
        FixtureController
    }
}

impl Class for FixtureController {
    type Interfaces = (IEditController,);
}

impl IPluginBaseTrait for FixtureController {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for FixtureController {
    unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        0
    }

    unsafe fn getParameterInfo(&self, _index: i32, _info: *mut ParameterInfo) -> tresult {
        kResultFalse
    }

    unsafe fn getParamStringByValue(
        &self,
        _id: ParamID,
        _value: ParamValue,
        _string: *mut String128,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn getParamValueByString(
        &self,
        _id: ParamID,
        _string: *mut TChar,
        _value: *mut ParamValue,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn normalizedParamToPlain(&self, _id: ParamID, value: ParamValue) -> ParamValue {
        value
    }

    unsafe fn plainParamToNormalized(&self, _id: ParamID, value: ParamValue) -> ParamValue {
        value
    }

    unsafe fn getParamNormalized(&self, _id: ParamID) -> ParamValue {
        0.0
    }

    unsafe fn setParamNormalized(&self, _id: ParamID, _value: ParamValue) -> tresult {
        kResultOk
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, name: FIDString) -> *mut IPlugView {
        if name.is_null() || CStr::from_ptr(name).to_bytes() != b"editor" {
            return ptr::null_mut();
        }
        ComWrapper::new(FixtureView::new())
            .to_com_ptr::<IPlugView>()
            .map_or(ptr::null_mut(), |view| view.into_raw())
    }
}

#[derive(Clone, Copy)]
enum SingleComponentControllerMode {
    NoSeparateClass,
    OwnClassId,
}

/// A one-class VST3 fixture: the audio component itself also exposes `IEditController`.
struct SingleComponentFixture {
    processor: FixtureProcessor,
    controller: FixtureController,
    controller_mode: SingleComponentControllerMode,
}

impl SingleComponentFixture {
    fn new(controller_mode: SingleComponentControllerMode) -> Self {
        Self {
            processor: FixtureProcessor::with_behavior(1.0, 0, FixtureBehavior::Passthrough),
            controller: FixtureController::new(),
            controller_mode,
        }
    }
}

impl Class for SingleComponentFixture {
    type Interfaces = (IComponent, IAudioProcessor, IEditController);
}

impl IPluginBaseTrait for SingleComponentFixture {
    unsafe fn initialize(&self, context: *mut FUnknown) -> tresult {
        self.processor.initialize(context)
    }

    unsafe fn terminate(&self) -> tresult {
        self.processor.terminate()
    }
}

impl IComponentTrait for SingleComponentFixture {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        match self.controller_mode {
            SingleComponentControllerMode::NoSeparateClass => kNotImplemented,
            SingleComponentControllerMode::OwnClassId => {
                if class_id.is_null() {
                    return kInvalidArgument;
                }
                *class_id = SINGLE_COMPONENT_CID;
                kResultOk
            }
        }
    }

    unsafe fn setIoMode(&self, mode: IoMode) -> tresult {
        self.processor.setIoMode(mode)
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        self.processor.getBusCount(media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        self.processor.getBusInfo(media_type, dir, index, bus)
    }

    unsafe fn getRoutingInfo(
        &self,
        in_info: *mut RoutingInfo,
        out_info: *mut RoutingInfo,
    ) -> tresult {
        self.processor.getRoutingInfo(in_info, out_info)
    }

    unsafe fn activateBus(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        state: TBool,
    ) -> tresult {
        self.processor.activateBus(media_type, dir, index, state)
    }

    unsafe fn setActive(&self, state: TBool) -> tresult {
        self.processor.setActive(state)
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        self.processor.setState(state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        self.processor.getState(state)
    }
}

impl IAudioProcessorTrait for SingleComponentFixture {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        self.processor
            .setBusArrangements(inputs, num_ins, outputs, num_outs)
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        self.processor.getBusArrangement(dir, index, arrangement)
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        self.processor.canProcessSampleSize(symbolic_sample_size)
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        self.processor.getLatencySamples()
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        self.processor.setupProcessing(setup)
    }

    unsafe fn setProcessing(&self, state: TBool) -> tresult {
        self.processor.setProcessing(state)
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        self.processor.process(data)
    }

    unsafe fn getTailSamples(&self) -> u32 {
        self.processor.getTailSamples()
    }
}

impl IEditControllerTrait for SingleComponentFixture {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        self.controller.setComponentState(state)
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        self.controller.setState(state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        self.controller.getState(state)
    }

    unsafe fn getParameterCount(&self) -> i32 {
        self.controller.getParameterCount()
    }

    unsafe fn getParameterInfo(&self, index: i32, info: *mut ParameterInfo) -> tresult {
        self.controller.getParameterInfo(index, info)
    }

    unsafe fn getParamStringByValue(
        &self,
        id: ParamID,
        value: ParamValue,
        string: *mut String128,
    ) -> tresult {
        self.controller.getParamStringByValue(id, value, string)
    }

    unsafe fn getParamValueByString(
        &self,
        id: ParamID,
        string: *mut TChar,
        value: *mut ParamValue,
    ) -> tresult {
        self.controller.getParamValueByString(id, string, value)
    }

    unsafe fn normalizedParamToPlain(&self, id: ParamID, value: ParamValue) -> ParamValue {
        self.controller.normalizedParamToPlain(id, value)
    }

    unsafe fn plainParamToNormalized(&self, id: ParamID, value: ParamValue) -> ParamValue {
        self.controller.plainParamToNormalized(id, value)
    }

    unsafe fn getParamNormalized(&self, id: ParamID) -> ParamValue {
        self.controller.getParamNormalized(id)
    }

    unsafe fn setParamNormalized(&self, id: ParamID, value: ParamValue) -> tresult {
        self.controller.setParamNormalized(id, value)
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        self.controller.setComponentHandler(handler)
    }

    unsafe fn createView(&self, name: FIDString) -> *mut IPlugView {
        self.controller.createView(name)
    }
}

struct SingleComponentFixtureFactory {
    controller_mode: SingleComponentControllerMode,
}

impl Class for SingleComponentFixtureFactory {
    type Interfaces = (IPluginFactory,);
}

impl IPluginFactoryTrait for SingleComponentFixtureFactory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        fill_cstr(&mut info.vendor, "Ahara");
        fill_cstr(&mut info.url, "");
        fill_cstr(&mut info.email, "");
        info.flags = 0;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        1
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() || index != 0 {
            return kInvalidArgument;
        }
        let info = &mut *info;
        info.cid = SINGLE_COMPONENT_CID;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        fill_cstr(&mut info.category, "Audio Module Class");
        fill_cstr(&mut info.name, "Galad Single Component Fixture");
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = ptr::null_mut();
        if *(cid as *const TUID) != SINGLE_COMPONENT_CID {
            return kInvalidArgument;
        }
        let instance = ComWrapper::new(SingleComponentFixture::new(self.controller_mode))
            .to_com_ptr::<FUnknown>()
            .expect("single-component fixture exposes FUnknown");
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

/// A fixture `IPlugView`: supports the `HWND` platform, reports a fixed 320×240 size, and records the
/// host frame; the rest is benign.
pub(super) struct FixtureView {
    frame: Cell<*mut IPlugFrame>,
    supports_hwnd: bool,
}

impl FixtureView {
    pub(super) fn new() -> Self {
        FixtureView {
            frame: Cell::new(ptr::null_mut()),
            supports_hwnd: true,
        }
    }

    /// A view that reports no `HWND` support (for the host's "unsupported editor" path).
    pub(super) fn without_hwnd() -> Self {
        FixtureView {
            frame: Cell::new(ptr::null_mut()),
            supports_hwnd: false,
        }
    }
}

impl Class for FixtureView {
    type Interfaces = (IPlugView,);
}

impl IPlugViewTrait for FixtureView {
    unsafe fn isPlatformTypeSupported(&self, r#type: FIDString) -> tresult {
        if self.supports_hwnd && !r#type.is_null() && CStr::from_ptr(r#type).to_bytes() == b"HWND" {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn attached(&self, _parent: *mut c_void, _type: FIDString) -> tresult {
        kResultOk
    }

    unsafe fn removed(&self) -> tresult {
        kResultOk
    }

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyDown(&self, _key: u16, _key_code: i16, _modifiers: i16) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyUp(&self, _key: u16, _key_code: i16, _modifiers: i16) -> tresult {
        kNotImplemented
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        *size = ViewRect {
            left: 0,
            top: 0,
            right: 320,
            bottom: 240,
        };
        kResultOk
    }

    unsafe fn onSize(&self, _new_size: *mut ViewRect) -> tresult {
        kResultOk
    }

    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setFrame(&self, frame: *mut IPlugFrame) -> tresult {
        self.frame.set(frame);
        kResultOk
    }

    unsafe fn canResize(&self) -> tresult {
        kResultTrue
    }

    unsafe fn checkSizeConstraint(&self, _rect: *mut ViewRect) -> tresult {
        kNotImplemented
    }
}

/// A factory exposing a [`FixtureProcessor`] (audio class) and a [`FixtureController`] (controller
/// class), configured with `gain`, `latency`, and a `behavior`.
pub(super) struct FixtureFactory {
    gain: f32,
    latency: u32,
    behavior: FixtureBehavior,
    bus_shape: FixtureBusShape,
}

impl Class for FixtureFactory {
    type Interfaces = (IPluginFactory, IPluginFactory2);
}

impl IPluginFactoryTrait for FixtureFactory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        fill_cstr(&mut info.vendor, "Ahara");
        fill_cstr(&mut info.url, "");
        fill_cstr(&mut info.email, "");
        info.flags = 0;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        2
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        match index {
            0 => {
                info.cid = FIXTURE_CID;
                fill_cstr(&mut info.category, "Audio Module Class");
                fill_cstr(&mut info.name, "Galad Fixture");
                kResultOk
            }
            1 => {
                info.cid = FIXTURE_CONTROLLER_CID;
                fill_cstr(&mut info.category, "Component Controller Class");
                fill_cstr(&mut info.name, "Galad Fixture Controller");
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = ptr::null_mut();
        let requested = *(cid as *const TUID);
        let instance = if requested == FIXTURE_CID {
            ComWrapper::new(FixtureProcessor::with_shape(
                self.gain,
                self.latency,
                self.behavior,
                self.bus_shape,
            ))
            .to_com_ptr::<FUnknown>()
            .expect("fixture processor exposes FUnknown")
        } else if requested == FIXTURE_CONTROLLER_CID {
            ComWrapper::new(FixtureController::new())
                .to_com_ptr::<FUnknown>()
                .expect("fixture controller exposes FUnknown")
        } else {
            return kInvalidArgument;
        };
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

impl IPluginFactory2Trait for FixtureFactory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        info.classFlags = ComponentFlags_::kDistributable as u32;
        fill_cstr(&mut info.subCategories, "Fx");
        fill_cstr(&mut info.vendor, "Ahara");
        fill_cstr(&mut info.version, "1.0.0");
        fill_cstr(&mut info.sdkVersion, "VST 3.8.0");
        match index {
            0 => {
                info.cid = FIXTURE_CID;
                fill_cstr(&mut info.category, "Audio Module Class");
                fill_cstr(&mut info.name, "Galad Fixture");
                kResultOk
            }
            1 => {
                info.cid = FIXTURE_CONTROLLER_CID;
                fill_cstr(&mut info.category, "Component Controller Class");
                fill_cstr(&mut info.name, "Galad Fixture Controller");
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }
}

/// A `ComPtr<IPluginFactory>` over a verbatim-passthrough fixture (gain 1.0, latency 0).
pub(super) fn fixture_factory() -> ComPtr<IPluginFactory> {
    gain_fixture_factory(1.0, 0)
}

/// A `ComPtr<IPluginFactory>` over a fixture that scales by `gain` and reports `latency`.
pub(super) fn gain_fixture_factory(gain: f32, latency: u32) -> ComPtr<IPluginFactory> {
    behaving_fixture_factory(gain, latency, FixtureBehavior::Passthrough)
}

/// A `ComPtr<IPluginFactory>` over a fixture with an explicit `behavior` (M7 containment tests).
pub(super) fn behaving_fixture_factory(
    gain: f32,
    latency: u32,
    behavior: FixtureBehavior,
) -> ComPtr<IPluginFactory> {
    shaped_fixture_factory(gain, latency, behavior, FixtureBusShape::Stereo)
}

pub(super) fn shaped_fixture_factory(
    gain: f32,
    latency: u32,
    behavior: FixtureBehavior,
    bus_shape: FixtureBusShape,
) -> ComPtr<IPluginFactory> {
    ComWrapper::new(FixtureFactory {
        gain,
        latency,
        behavior,
        bus_shape,
    })
    .to_com_ptr::<IPluginFactory>()
    .expect("fixture factory exposes IPluginFactory")
}

/// A fixture whose `process` returns an error result — models a plugin that fails mid-process.
pub(super) fn process_error_factory() -> ComPtr<IPluginFactory> {
    behaving_fixture_factory(1.0, 0, FixtureBehavior::ProcessError)
}

/// A fixture whose `process` writes NaN — models a plugin emitting non-finite output.
pub(super) fn nan_fixture_factory() -> ComPtr<IPluginFactory> {
    behaving_fixture_factory(1.0, 0, FixtureBehavior::NaNOutput)
}

pub(super) fn sidechain_fixture_factory() -> ComPtr<IPluginFactory> {
    shaped_fixture_factory(
        1.0,
        0,
        FixtureBehavior::Passthrough,
        FixtureBusShape::StereoWithExtraInput,
    )
}

pub(super) fn strict_sidechain_fixture_factory() -> ComPtr<IPluginFactory> {
    shaped_fixture_factory(
        1.0,
        0,
        FixtureBehavior::RequiresDeclaredAudioBuses,
        FixtureBusShape::StereoWithExtraInput,
    )
}

pub(super) fn context_fixture_factory() -> ComPtr<IPluginFactory> {
    behaving_fixture_factory(1.0, 0, FixtureBehavior::RequiresProcessContext)
}

pub(super) fn midi_note_fixture_factory() -> ComPtr<IPluginFactory> {
    behaving_fixture_factory(1.0, 0, FixtureBehavior::MidiNoteTriggersOutput)
}

pub(super) fn parameter_fixture_factory() -> ComPtr<IPluginFactory> {
    behaving_fixture_factory(1.0, 0, FixtureBehavior::ParameterControlsOutput)
}

pub(super) fn fixed_stereo_rejects_arrangement_factory() -> ComPtr<IPluginFactory> {
    shaped_fixture_factory(
        1.0,
        0,
        FixtureBehavior::Passthrough,
        FixtureBusShape::FixedStereoRejectsSet,
    )
}

pub(super) fn single_component_fixture_factory() -> ComPtr<IPluginFactory> {
    single_component_factory(SingleComponentControllerMode::NoSeparateClass)
}

pub(super) fn own_cid_single_component_fixture_factory() -> ComPtr<IPluginFactory> {
    single_component_factory(SingleComponentControllerMode::OwnClassId)
}

fn single_component_factory(
    controller_mode: SingleComponentControllerMode,
) -> ComPtr<IPluginFactory> {
    ComWrapper::new(SingleComponentFixtureFactory { controller_mode })
        .to_com_ptr::<IPluginFactory>()
        .expect("single-component fixture factory exposes IPluginFactory")
}

/// A factory exposing only a controller class — no "Audio Module Class" — so the host's instantiate
/// path rejects it with `HostError::NoAudioClass`, modelling an incompatible plugin.
pub(super) struct NoAudioClassFactory;

impl Class for NoAudioClassFactory {
    type Interfaces = (IPluginFactory,);
}

impl IPluginFactoryTrait for NoAudioClassFactory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        fill_cstr(&mut info.vendor, "Ahara");
        fill_cstr(&mut info.url, "");
        fill_cstr(&mut info.email, "");
        info.flags = 0;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        1
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        match index {
            0 => {
                info.cid = FIXTURE_CONTROLLER_CID;
                fill_cstr(&mut info.category, "Component Controller Class");
                fill_cstr(&mut info.name, "Galad Fixture Controller");
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = ptr::null_mut();
        if *(cid as *const TUID) != FIXTURE_CONTROLLER_CID {
            return kInvalidArgument;
        }
        let instance = ComWrapper::new(FixtureController::new())
            .to_com_ptr::<FUnknown>()
            .expect("fixture controller exposes FUnknown");
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

/// A `ComPtr<IPluginFactory>` over a factory with no audio class (incompatible-plugin fixture).
pub(super) fn no_audio_class_factory() -> ComPtr<IPluginFactory> {
    ComWrapper::new(NoAudioClassFactory)
        .to_com_ptr::<IPluginFactory>()
        .expect("no-audio-class factory exposes IPluginFactory")
}

/// Write `s` as a NUL-terminated C-string into a `[c_char]`, truncating to fit.
fn fill_cstr(dst: &mut [c_char], s: &str) {
    dst.iter_mut().for_each(|b| *b = 0);
    let n = dst.len().saturating_sub(1);
    for (slot, byte) in dst.iter_mut().zip(s.bytes()).take(n) {
        *slot = byte as c_char;
    }
}

/// Write `s` as NUL-terminated UTF-16 into a `[u16]`, truncating to fit.
fn fill_utf16(dst: &mut [u16], s: &str) {
    dst.iter_mut().for_each(|b| *b = 0);
    let n = dst.len().saturating_sub(1);
    for (slot, unit) in dst.iter_mut().zip(s.encode_utf16()).take(n) {
        *slot = unit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_factory_creates_audio_processor() {
        let factory = fixture_factory();
        assert_eq!(unsafe { factory.countClasses() }, 2);

        let mut obj: *mut c_void = ptr::null_mut();
        let result = unsafe {
            factory.createInstance(
                FIXTURE_CID.as_ptr(),
                IComponent::IID.as_ptr().cast(),
                &mut obj,
            )
        };

        assert_eq!(result, kResultOk);
        assert!(!obj.is_null());

        let component =
            unsafe { ComPtr::from_raw(obj.cast::<IComponent>()) }.expect("non-null IComponent");
        let processor = component
            .cast::<IAudioProcessor>()
            .expect("fixture exposes IAudioProcessor");
        assert_eq!(unsafe { processor.getLatencySamples() }, 0);
    }

    #[test]
    fn fixture_factory_creates_edit_controller_and_links_it() {
        let factory = fixture_factory();

        // The processor points at the controller class.
        let mut obj: *mut c_void = ptr::null_mut();
        unsafe {
            factory.createInstance(
                FIXTURE_CID.as_ptr(),
                IComponent::IID.as_ptr().cast(),
                &mut obj,
            )
        };
        let component =
            unsafe { ComPtr::from_raw(obj.cast::<IComponent>()) }.expect("non-null IComponent");
        let mut controller_cid: TUID = [0; 16];
        assert_eq!(
            unsafe { component.getControllerClassId(&mut controller_cid) },
            kResultOk
        );
        assert_eq!(controller_cid, FIXTURE_CONTROLLER_CID);

        // The controller class instantiates as an IEditController.
        let mut ctrl_obj: *mut c_void = ptr::null_mut();
        let result = unsafe {
            factory.createInstance(
                FIXTURE_CONTROLLER_CID.as_ptr(),
                IEditController::IID.as_ptr().cast(),
                &mut ctrl_obj,
            )
        };
        assert_eq!(result, kResultOk);
        assert!(!ctrl_obj.is_null());
        let _controller = unsafe { ComPtr::from_raw(ctrl_obj.cast::<IEditController>()) }
            .expect("non-null IEditController");
    }

    #[test]
    fn hostile_fixtures_construct() {
        // The no-audio-class factory exposes only a controller class…
        let no_audio = no_audio_class_factory();
        assert_eq!(unsafe { no_audio.countClasses() }, 1);
        let mut info: PClassInfo = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { no_audio.getClassInfo(0, &mut info) }, kResultOk);
        let category = unsafe { CStr::from_ptr(info.category.as_ptr()) }.to_bytes();
        assert_ne!(category, b"Audio Module Class");

        // …while the misbehaving fixtures still expose the audio class (the misbehavior is at process).
        assert_eq!(unsafe { process_error_factory().countClasses() }, 2);
        assert_eq!(unsafe { nan_fixture_factory().countClasses() }, 2);
    }

    #[test]
    fn gain_fixture_scales_and_reports_latency() {
        use crate::vst3_host::{HostContext, PluginInstance, ProcessDriver};

        let factory = gain_fixture_factory(0.5, 7);
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");
        let driver = ProcessDriver::new(48_000.0, 512);
        driver.prepare(&instance).expect("prepare");

        let input = vec![vec![1.0f32, 2.0, 3.0], vec![4.0f32, 5.0, 6.0]];
        let output = driver.process_block(&instance, &input);

        assert_eq!(output[0], vec![0.5, 1.0, 1.5]);
        assert_eq!(output[1], vec![2.0, 2.5, 3.0]);
        assert_eq!(unsafe { instance.processor().getLatencySamples() }, 7);
    }
}
