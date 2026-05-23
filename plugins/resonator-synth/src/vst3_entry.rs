#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, CString as StdCString, c_char, c_void},
    fs, io,
    mem::MaybeUninit,
    path::{Path, PathBuf},
    ptr, slice,
};

use ahara_plugin_shell::{
    AudioBuffer, AudioPlugin, ControlEvent, MidiEvent, NoteEvent, ParameterId, PluginState,
    ProcessContext as ShellProcessContext, ProcessMode, ProcessSetup as ShellProcessSetup,
};
use ahara_sample_library::{
    FileSampleLibrary, LibraryPaths, SampleLibrary, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use vst3::{Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    DESCRIPTOR, PARAMETERS, ResonatorSynth, ResonatorSynthPatch, ResonatorTelemetry,
    parameter_binding, parameter_binding_by_index, parameter_binding_index, patch_io,
    patch_parameter_plain_value,
};

mod editor;

const MAX_BLOCK_EVENTS: usize = 128;
const STATE_MAGIC: [u8; 4] = *b"AHRS";
const STATE_HEADER_BYTES: usize = 12;
const MAX_STATE_BYTES: usize = 1_048_576;
const SUBCATEGORY: &str = "Instrument|Synth";
const PITCH_BEND_PARAMETER_ID: u32 = 10_000;
const PITCH_BEND_PARAMETER_INDEX: usize = PARAMETERS.len();
const VST3_PARAMETER_COUNT: usize = PARAMETERS.len() + 1;
const DEFAULT_PITCH_BEND_RANGE_SEMITONES: f32 = 2.0;
const MESSAGE_PATCH_UPDATE: &str = "ahara.resonator.patch_update";
const MESSAGE_TELEMETRY_REQUEST: &str = "ahara.resonator.telemetry_request";
const MESSAGE_TELEMETRY_RESPONSE: &str = "ahara.resonator.telemetry_response";
const MESSAGE_ATTRIBUTE_PAYLOAD: &[u8] = b"payload\0";
const DEFAULT_LIBRARY_DIR: &str = "Ahara";

struct ResonatorVst3Processor {
    synth: RefCell<ResonatorSynth>,
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
    const CID: TUID = uid(0x4B410E03, 0x80AD49B6, 0x9B7D5479, 0xF4A9B0D1);

    fn new() -> Self {
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
        let message = PluginMessage::with_payload(MESSAGE_TELEMETRY_RESPONSE, payload);
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
        let Some(id) = message_id(message) else {
            return kResultFalse;
        };

        match id.as_str() {
            MESSAGE_PATCH_UPDATE => {
                let Some(payload) = message_payload(message) else {
                    return kResultFalse;
                };
                self.apply_patch_payload(&payload)
            }
            MESSAGE_TELEMETRY_REQUEST => self.send_telemetry_response(),
            _ => kNotImplemented,
        }
    }
}

struct ResonatorVst3Controller {
    values: Cell<[f64; VST3_PARAMETER_COUNT]>,
    handler: Cell<*mut IComponentHandler>,
    editor_summary: RefCell<EditorPatchSummary>,
    patch: RefCell<ResonatorSynthPatch>,
    peer: Cell<*mut IConnectionPoint>,
    telemetry: Cell<EditorTelemetry>,
    library_samples: RefCell<Vec<SampleMetadata>>,
}

impl Class for ResonatorVst3Controller {
    type Interfaces = (IEditController, IMidiMapping, IConnectionPoint);
}

impl ResonatorVst3Controller {
    const CID: TUID = uid(0x15C8B012, 0xF4B64F5E, 0x93D9AA38, 0x69383E3B);

    fn new() -> Self {
        Self {
            values: Cell::new(default_parameter_values()),
            handler: Cell::new(ptr::null_mut()),
            editor_summary: RefCell::new(EditorPatchSummary::from_patch(
                &crate::ResonatorSynthPatch::default(),
            )),
            patch: RefCell::new(ResonatorSynthPatch::default()),
            peer: Cell::new(ptr::null_mut()),
            telemetry: Cell::new(EditorTelemetry::default()),
            library_samples: RefCell::new(Vec::new()),
        }
    }

    fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameter_index(id) else {
            return kInvalidArgument;
        };
        let mut values = self.values.get();
        values[index] = if normalized.is_finite() {
            normalized.clamp(0.0, 1.0)
        } else {
            default_parameter_values()[index]
        };
        self.values.set(values);
        if id != PITCH_BEND_PARAMETER_ID
            && let Some(parameter) = parameter_by_id(id)
        {
            let plain = parameter.range.denormalize(values[index] as f32);
            crate::apply_parameter_plain_for_controller(&mut self.patch.borrow_mut(), id, plain);
            self.editor_summary
                .replace(EditorPatchSummary::from_patch_and_library(
                    &self.patch.borrow(),
                    &self.library_samples.borrow(),
                ));
        }
        kResultOk
    }

    fn editor_summary(&self) -> EditorPatchSummary {
        self.editor_summary.borrow().clone()
    }

    fn telemetry(&self) -> EditorTelemetry {
        self.telemetry.get()
    }

    fn replace_patch_mirror(&self, patch: ResonatorSynthPatch) {
        self.values.set(parameter_values_from_patch(&patch));
        self.patch.replace(patch);
        self.editor_summary
            .replace(EditorPatchSummary::from_patch_and_library(
                &self.patch.borrow(),
                &self.library_samples.borrow(),
            ));
    }

    fn notify_parameter_values_changed(&self) {
        let Some(handler) = (unsafe { ComRef::from_raw(self.handler.get()) }) else {
            return;
        };
        unsafe {
            handler.restartComponent(RestartFlags_::kParamValuesChanged);
        }
    }

    fn send_patch_to_processor(&self) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultFalse;
        };
        let payload_patch = processor_patch_from_controller_patch(&self.patch.borrow());
        let Ok(payload) = patch_io::to_toml_string(&payload_patch) else {
            return kResultFalse;
        };
        let message = PluginMessage::with_payload(MESSAGE_PATCH_UPDATE, payload.into_bytes());
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }

    fn request_telemetry(&self) {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return;
        };
        let message = PluginMessage::with_payload(MESSAGE_TELEMETRY_REQUEST, Vec::new());
        if let Some(message) = message.to_com_ptr::<IMessage>() {
            unsafe {
                peer.notify(message.as_ptr());
            }
        }
    }

    fn save_patch_to_path(&self, path: &Path) -> Result<(), patch_io::PatchIoError> {
        patch_io::save_patch(path, &self.patch.borrow())
    }

    fn load_patch_from_path(&self, path: &Path) -> Result<tresult, patch_io::PatchIoError> {
        let mut patch = patch_io::load_patch(path)?;
        resolve_patch_samples_for_loaded_path(&mut patch, path);
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        Ok(self.send_patch_to_processor())
    }

    fn export_patch_bundle(&self, directory: &Path) -> io::Result<PathBuf> {
        export_patch_bundle(directory, &self.patch.borrow())
    }

    fn refresh_library(&self) -> io::Result<()> {
        let samples = open_default_sample_library()
            .map_err(io::Error::other)?
            .list_samples()
            .map_err(io::Error::other)?;
        self.library_samples.replace(samples);
        self.editor_summary
            .replace(EditorPatchSummary::from_patch_and_library(
                &self.patch.borrow(),
                &self.library_samples.borrow(),
            ));
        Ok(())
    }

    fn ingest_sample(&self, path: PathBuf) -> io::Result<SampleReference> {
        let mut library = open_default_sample_library().map_err(io::Error::other)?;
        let metadata = library.ingest(path).map_err(io::Error::other)?;
        let reference = metadata.reference.clone();
        self.refresh_library()?;
        Ok(reference)
    }

    fn assign_library_sample_to_slot(&self, sample_index: usize, slot_index: usize) -> tresult {
        let Some(metadata) = self.library_samples.borrow().get(sample_index).cloned() else {
            return kInvalidArgument;
        };
        self.assign_sample_reference_to_slot(metadata.reference, slot_index)
    }

    fn assign_sample_reference_to_slot(
        &self,
        reference: SampleReference,
        slot_index: usize,
    ) -> tresult {
        let mut patch = self.patch.borrow().clone();
        ensure_excitation_slot(&mut patch, slot_index);
        if let Some(slot) = patch.excitation_slots.get_mut(slot_index) {
            slot.sample = Some(reference);
        }
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        self.send_patch_to_processor()
    }

    fn clear_slot(&self, slot_index: usize) -> tresult {
        let mut patch = self.patch.borrow().clone();
        ensure_excitation_slot(&mut patch, slot_index);
        if let Some(slot) = patch.excitation_slots.get_mut(slot_index) {
            slot.sample = None;
        }
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        self.send_patch_to_processor()
    }
}

impl IConnectionPointTrait for ResonatorVst3Controller {
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
        let Some(id) = message_id(message) else {
            return kResultFalse;
        };
        if id.as_str() != MESSAGE_TELEMETRY_RESPONSE {
            return kNotImplemented;
        }
        let Some(payload) = message_payload(message) else {
            return kResultFalse;
        };
        let Some(telemetry) = decode_telemetry(&payload) else {
            return kResultFalse;
        };
        self.telemetry.set(telemetry);
        kResultOk
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct EditorTelemetry {
    left_peak: f32,
    right_peak: f32,
    left_rms: f32,
    right_rms: f32,
    active_voices: f32,
}

impl From<ResonatorTelemetry> for EditorTelemetry {
    fn from(value: ResonatorTelemetry) -> Self {
        Self {
            left_peak: value.left_peak,
            right_peak: value.right_peak,
            left_rms: value.left_rms,
            right_rms: value.right_rms,
            active_voices: value.active_voices as f32,
        }
    }
}

fn encode_telemetry(telemetry: ResonatorTelemetry) -> String {
    format!(
        "{:.8},{:.8},{:.8},{:.8},{}",
        telemetry.left_peak,
        telemetry.right_peak,
        telemetry.left_rms,
        telemetry.right_rms,
        telemetry.active_voices
    )
}

fn decode_telemetry(payload: &[u8]) -> Option<EditorTelemetry> {
    let text = std::str::from_utf8(payload).ok()?;
    let mut parts = text.split(',');
    let left_peak = finite_telemetry(parts.next()?.parse().ok()?);
    let right_peak = finite_telemetry(parts.next()?.parse().ok()?);
    let left_rms = finite_telemetry(parts.next()?.parse().ok()?);
    let right_rms = finite_telemetry(parts.next()?.parse().ok()?);
    let active_voices = parts.next()?.parse::<f32>().ok()?.clamp(0.0, 64.0);
    if parts.next().is_some() {
        return None;
    }
    Some(EditorTelemetry {
        left_peak,
        right_peak,
        left_rms,
        right_rms,
        active_voices,
    })
}

fn finite_telemetry(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 64.0)
    } else {
        0.0
    }
}

struct PluginAttributes {
    payload: RefCell<Vec<u8>>,
}

impl PluginAttributes {
    fn new(payload: Vec<u8>) -> Self {
        Self {
            payload: RefCell::new(payload),
        }
    }
}

impl Class for PluginAttributes {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for PluginAttributes {
    unsafe fn setInt(&self, _id: IAttrID, _value: int64) -> tresult {
        kNotImplemented
    }

    unsafe fn getInt(&self, _id: IAttrID, _value: *mut int64) -> tresult {
        kNotImplemented
    }

    unsafe fn setFloat(&self, _id: IAttrID, _value: f64) -> tresult {
        kNotImplemented
    }

    unsafe fn getFloat(&self, _id: IAttrID, _value: *mut f64) -> tresult {
        kNotImplemented
    }

    unsafe fn setString(&self, _id: IAttrID, _string: *const TChar) -> tresult {
        kNotImplemented
    }

    unsafe fn getString(&self, _id: IAttrID, _string: *mut TChar, _sizeInBytes: uint32) -> tresult {
        kNotImplemented
    }

    unsafe fn setBinary(&self, id: IAttrID, data: *const c_void, sizeInBytes: uint32) -> tresult {
        if !is_payload_attribute(id) || (data.is_null() && sizeInBytes > 0) {
            return kResultFalse;
        }
        let bytes = if sizeInBytes == 0 {
            Vec::new()
        } else {
            slice::from_raw_parts(data.cast::<u8>(), sizeInBytes as usize).to_vec()
        };
        self.payload.replace(bytes);
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: IAttrID,
        data: *mut *const c_void,
        sizeInBytes: *mut uint32,
    ) -> tresult {
        if !is_payload_attribute(id) || data.is_null() || sizeInBytes.is_null() {
            return kResultFalse;
        }
        let payload = self.payload.borrow();
        *data = payload.as_ptr().cast::<c_void>();
        *sizeInBytes = payload.len().min(u32::MAX as usize) as uint32;
        kResultOk
    }
}

struct PluginMessage {
    message_id: RefCell<StdCString>,
    attributes: ComPtr<IAttributeList>,
}

impl PluginMessage {
    fn with_payload(id: &str, payload: Vec<u8>) -> ComWrapper<Self> {
        let attributes = ComWrapper::new(PluginAttributes::new(payload))
            .to_com_ptr::<IAttributeList>()
            .expect("PluginAttributes must expose IAttributeList");
        ComWrapper::new(Self {
            message_id: RefCell::new(StdCString::new(id).unwrap_or_default()),
            attributes,
        })
    }
}

impl Class for PluginMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for PluginMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        self.message_id.borrow().as_ptr()
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        if id.is_null() {
            self.message_id.replace(StdCString::default());
        } else {
            self.message_id.replace(CStr::from_ptr(id).to_owned());
        }
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes.as_ptr()
    }
}

unsafe fn is_payload_attribute(id: IAttrID) -> bool {
    !id.is_null() && CStr::from_ptr(id).to_bytes_with_nul() == MESSAGE_ATTRIBUTE_PAYLOAD
}

unsafe fn message_id(message: *mut IMessage) -> Option<String> {
    let message = ComRef::from_raw(message)?;
    let id = message.getMessageID();
    if id.is_null() {
        return None;
    }
    Some(CStr::from_ptr(id).to_string_lossy().into_owned())
}

unsafe fn message_payload(message: *mut IMessage) -> Option<Vec<u8>> {
    let message = ComRef::from_raw(message)?;
    let attributes = ComRef::from_raw(message.getAttributes())?;
    let mut data = ptr::null::<c_void>();
    let mut size = 0;
    if attributes.getBinary(
        MESSAGE_ATTRIBUTE_PAYLOAD.as_ptr().cast::<c_char>(),
        &mut data,
        &mut size,
    ) != kResultOk
        || (data.is_null() && size > 0)
    {
        return None;
    }
    Some(slice::from_raw_parts(data.cast::<u8>(), size as usize).to_vec())
}

fn default_library_paths() -> LibraryPaths {
    LibraryPaths::from_root(default_library_root())
}

fn default_library_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Music")
        .join(DEFAULT_LIBRARY_DIR)
}

fn open_default_sample_library()
-> Result<FileSampleLibrary, ahara_sample_library::SampleLibraryError> {
    FileSampleLibrary::open(default_library_paths())
}

fn ensure_excitation_slot(patch: &mut ResonatorSynthPatch, slot_index: usize) {
    let target_len = (slot_index + 1).min(crate::dsp::MAX_EXCITATION_LAYERS);
    while patch.excitation_slots.len() < target_len {
        patch
            .excitation_slots
            .push(crate::ExcitationSlot::default());
    }
}

fn processor_patch_from_controller_patch(patch: &ResonatorSynthPatch) -> ResonatorSynthPatch {
    let mut patch = patch.clone();
    let paths = default_library_paths();
    for slot in &mut patch.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        if reference.last_known_path.is_relative() {
            let candidate = paths.root.join(&reference.last_known_path);
            if candidate.exists() {
                reference.last_known_path = candidate;
            }
        }
    }
    patch
}

fn resolve_patch_samples_for_loaded_path(patch: &mut ResonatorSynthPatch, patch_path: &Path) {
    let Some(patch_dir) = patch_path.parent() else {
        return;
    };
    let default_root = default_library_root();
    for slot in &mut patch.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        if reference.last_known_path.is_absolute() {
            continue;
        }
        let relative = reference.last_known_path.clone();
        for candidate in [patch_dir.join(&relative), default_root.join(&relative)] {
            if candidate.exists() {
                reference.last_known_path = candidate;
                break;
            }
        }
    }
}

fn export_patch_bundle(directory: &Path, patch: &ResonatorSynthPatch) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let samples_dir = directory.join("Samples");
    fs::create_dir_all(&samples_dir)?;

    let mut exported = patch.clone();
    let default_root = default_library_root();
    for slot in &mut exported.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        let source = if reference.last_known_path.is_absolute() {
            reference.last_known_path.clone()
        } else {
            default_root.join(&reference.last_known_path)
        };
        if !source.is_file() {
            continue;
        }
        let filename = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sample.wav");
        let target = samples_dir.join(filename);
        fs::copy(&source, &target)?;
        reference.last_known_path = PathBuf::from("Samples").join(filename);
    }

    let patch_path = directory.join(format!("{}.toml", sanitize_patch_filename(&exported.name)));
    patch_io::save_patch(&patch_path, &exported)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    Ok(patch_path)
}

fn sanitize_patch_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();
    if sanitized.is_empty() {
        "Untitled".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EditorPatchSummary {
    patch_name: String,
    slots: [EditorSlotSummary; 4],
    library_samples: Vec<EditorSampleSummary>,
}

#[derive(Debug, Clone, PartialEq)]
struct EditorSampleSummary {
    label: String,
    detail: String,
    preview: Vec<EditorWaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EditorWaveformPoint {
    min: f32,
    max: f32,
    rms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorSlotSummary {
    label: String,
    detail: String,
    sample_backed: bool,
    pitch_track: bool,
    looping: bool,
}

impl EditorPatchSummary {
    fn from_patch(patch: &crate::ResonatorSynthPatch) -> Self {
        Self::from_patch_and_library(patch, &[])
    }

    fn from_patch_and_library(
        patch: &crate::ResonatorSynthPatch,
        samples: &[SampleMetadata],
    ) -> Self {
        Self {
            patch_name: patch.name.clone(),
            slots: std::array::from_fn(|index| {
                let slot = patch.excitation_slots.get(index);
                EditorSlotSummary::from_slot(index, slot)
            }),
            library_samples: samples
                .iter()
                .map(EditorSampleSummary::from_metadata)
                .collect(),
        }
    }
}

impl EditorSampleSummary {
    fn from_metadata(metadata: &SampleMetadata) -> Self {
        let duration = metadata.duration_ms as f32 / 1_000.0;
        let detail = format!(
            "{duration:.2}s  {} Hz  {}ch",
            metadata.sample_rate, metadata.channels
        );
        Self {
            label: metadata.filename.clone(),
            detail,
            preview: waveform_points(&metadata.waveform_preview),
        }
    }
}

fn waveform_points(preview: &SampleWaveformPreview) -> Vec<EditorWaveformPoint> {
    preview
        .points
        .iter()
        .map(|point| EditorWaveformPoint {
            min: point.min,
            max: point.max,
            rms: point.rms,
        })
        .collect()
}

impl EditorSlotSummary {
    fn from_slot(index: usize, slot: Option<&crate::ExcitationSlot>) -> Self {
        let Some(slot) = slot else {
            return Self::empty(index);
        };

        let Some(reference) = slot.sample.as_ref() else {
            return if index == 0 {
                Self::builtin(slot)
            } else {
                Self::empty(index)
            };
        };

        let filename = reference
            .last_known_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Sample");
        Self {
            label: format!("Slot {}", index + 1),
            detail: filename.to_string(),
            sample_backed: true,
            pitch_track: slot.pitch_track,
            looping: slot.looping,
        }
    }

    fn builtin(slot: &crate::ExcitationSlot) -> Self {
        Self {
            label: "Slot 1".to_string(),
            detail: "Built-in pluck".to_string(),
            sample_backed: false,
            pitch_track: slot.pitch_track,
            looping: slot.looping,
        }
    }

    fn empty(index: usize) -> Self {
        Self {
            label: format!("Slot {}", index + 1),
            detail: "Empty layer".to_string(),
            sample_backed: false,
            pitch_track: false,
            looping: false,
        }
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

        self.replace_patch_mirror(patch);
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        VST3_PARAMETER_COUNT as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        if param_index as usize == PITCH_BEND_PARAMETER_INDEX {
            info.id = PITCH_BEND_PARAMETER_ID;
            copy_wstring("Pitch Bend", &mut info.title);
            copy_wstring("Pitch", &mut info.shortTitle);
            copy_wstring("st", &mut info.units);
            info.stepCount = 0;
            info.defaultNormalizedValue = 0.5;
            info.unitId = 0;
            info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate
                | ParameterInfo_::ParameterFlags_::kIsHidden;
            return kResultOk;
        }

        let Some(binding) = parameter_binding_by_index(param_index as usize) else {
            return kInvalidArgument;
        };
        let parameter = binding.info();

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
        if id == PITCH_BEND_PARAMETER_ID {
            copy_wstring(
                &format_plain_value(pitch_bend_plain_from_normalized(value_normalized) as f32),
                &mut *string,
            );
            return kResultOk;
        }

        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        copy_wstring(&format_parameter_plain_value(id, plain), &mut *string);
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
        let len = len_wstring(string as *const TChar);
        let Ok(text) = String::from_utf16(slice::from_raw_parts(string as *const u16, len)) else {
            return kInvalidArgument;
        };
        let Ok(value) = text.trim().parse::<f32>() else {
            return kInvalidArgument;
        };
        if id == PITCH_BEND_PARAMETER_ID {
            *value_normalized = pitch_bend_normalized_from_plain(value);
            return kResultOk;
        }

        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        *value_normalized = parameter.range.normalize(value) as f64;
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        if id == PITCH_BEND_PARAMETER_ID {
            return pitch_bend_plain_from_normalized(value_normalized);
        }

        parameter_by_id(id)
            .map(|parameter| parameter.range.denormalize(value_normalized as f32) as f64)
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        if id == PITCH_BEND_PARAMETER_ID {
            return pitch_bend_normalized_from_plain(plain_value as f32);
        }

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
        editor::create_editor_view(self)
    }
}

impl IMidiMappingTrait for ResonatorVst3Controller {
    unsafe fn getMidiControllerAssignment(
        &self,
        busIndex: i32,
        channel: i16,
        midiControllerNumber: CtrlNumber,
        id: *mut u32,
    ) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }

        if busIndex == 0
            && (0..=15).contains(&channel)
            && midiControllerNumber == ControllerNumbers_::kPitchBend as CtrlNumber
        {
            *id = PITCH_BEND_PARAMETER_ID;
            return kResultTrue;
        }

        kResultFalse
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
        match self {
            Self::Processor => DESCRIPTOR.name,
            Self::Controller => "Ahara Resonator Synth Controller",
        }
    }

    fn class_flags(self) -> u32 {
        match self {
            Self::Processor => ComponentFlags_::kDistributable,
            Self::Controller => 0,
        }
    }

    fn subcategories(self) -> &'static str {
        match self {
            Self::Processor => SUBCATEGORY,
            Self::Controller => "",
        }
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
        info.classFlags = self.class_flags();
        copy_cstring(self.subcategories(), &mut info.subCategories);
        copy_cstring(DESCRIPTOR.vendor, &mut info.vendor);
        copy_cstring(DESCRIPTOR.version, &mut info.version);
        copy_cstring("VST 3.8.0", &mut info.sdkVersion);
    }

    fn fill_class_info_w(self, info: &mut PClassInfoW) {
        info.cid = self.cid();
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category(), &mut info.category);
        copy_wstring(self.name(), &mut info.name);
        info.classFlags = self.class_flags();
        copy_cstring(self.subcategories(), &mut info.subCategories);
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
            Some(MidiEvent::Control(ControlEvent::PolyPressure {
                channel: pressure.channel.clamp(0, 15) as u8,
                note: pressure.pitch.clamp(0, 127) as u8,
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

fn default_parameter_values() -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = [0.0; VST3_PARAMETER_COUNT];
    for (index, parameter) in PARAMETERS.iter().enumerate() {
        values[index] = parameter.range.normalize(parameter.range.default) as f64;
    }
    values[PITCH_BEND_PARAMETER_INDEX] = 0.5;
    values
}

fn parameter_values_from_patch(patch: &crate::ResonatorSynthPatch) -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = default_parameter_values();
    for binding in (0..PARAMETERS.len()).filter_map(parameter_binding_by_index) {
        let parameter = binding.info();
        if let Some(plain) = patch_parameter_plain_value(patch, parameter.id.0)
            && let Some(index) = parameter_index(parameter.id.0)
        {
            values[index] = parameter.range.normalize(plain) as f64;
        }
    }
    values
}

fn parameter_index(id: u32) -> Option<usize> {
    if id == PITCH_BEND_PARAMETER_ID {
        return Some(PITCH_BEND_PARAMETER_INDEX);
    }

    parameter_binding_index(id)
}

fn parameter_by_id(id: u32) -> Option<&'static ahara_plugin_shell::ParameterInfo> {
    parameter_binding(id).map(|binding| {
        let info = binding.info();
        PARAMETERS
            .iter()
            .find(|parameter| parameter.id == info.id)
            .expect("binding info should be mirrored in PARAMETERS")
    })
}

fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    parameter_binding(id)
        .map(|binding| binding.info().range.normalize(plain) as f64)
        .unwrap_or(0.0)
}

fn pitch_bend_plain_from_normalized(normalized: f64) -> f64 {
    let normalized = if normalized.is_finite() {
        normalized
    } else {
        0.5
    };
    (normalized.clamp(0.0, 1.0) * 2.0 - 1.0) * f64::from(DEFAULT_PITCH_BEND_RANGE_SEMITONES)
}

fn pitch_bend_normalized_from_plain(plain: f32) -> f64 {
    let plain = plain.clamp(
        -DEFAULT_PITCH_BEND_RANGE_SEMITONES,
        DEFAULT_PITCH_BEND_RANGE_SEMITONES,
    );
    f64::from((plain / DEFAULT_PITCH_BEND_RANGE_SEMITONES + 1.0) * 0.5)
}

fn format_parameter_plain_value(parameter_id: u32, value: f32) -> String {
    parameter_binding(parameter_id)
        .map(|binding| binding.format_plain_value(value))
        .unwrap_or_else(|| format_plain_value(value))
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
pub extern "C" fn bundleEntry(_bundle_ref: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn bundleExit() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn BundleEntry(bundle_ref: *mut c_void) -> bool {
    bundleEntry(bundle_ref)
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn BundleExit() -> bool {
    bundleExit()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_mapping_assigns_pitch_bend_parameter() {
        let controller = ResonatorVst3Controller::new();
        let mut parameter_id = 0;
        let result = unsafe {
            controller.getMidiControllerAssignment(
                0,
                0,
                ControllerNumbers_::kPitchBend as CtrlNumber,
                &mut parameter_id,
            )
        };

        assert_eq!(result, kResultTrue);
        assert_eq!(parameter_id, PITCH_BEND_PARAMETER_ID);
    }

    #[test]
    fn pitch_bend_parameter_uses_centered_normalized_range() {
        assert_eq!(pitch_bend_plain_from_normalized(0.0), -2.0);
        assert_eq!(pitch_bend_plain_from_normalized(0.5), 0.0);
        assert_eq!(pitch_bend_plain_from_normalized(1.0), 2.0);
        assert_eq!(pitch_bend_normalized_from_plain(-2.0), 0.0);
        assert_eq!(pitch_bend_normalized_from_plain(0.0), 0.5);
        assert_eq!(pitch_bend_normalized_from_plain(2.0), 1.0);
    }

    #[test]
    fn vst_poly_pressure_maps_to_internal_poly_pressure() {
        let mut event = unsafe { std::mem::zeroed::<Event>() };
        event.r#type = Event_::EventTypes_::kPolyPressureEvent as u16;
        event.__field0.polyPressure.channel = 2;
        event.__field0.polyPressure.pitch = 64;
        event.__field0.polyPressure.pressure = 0.75;

        let mapped = unsafe { vst_event_to_midi(event) };

        assert_eq!(
            mapped,
            Some(MidiEvent::Control(ControlEvent::PolyPressure {
                channel: 2,
                note: 64,
                value: 0.75,
            }))
        );
    }

    #[test]
    fn waveguide_style_parameters_format_as_labels() {
        assert_eq!(format_parameter_plain_value(35, 0.0), "String");
        assert_eq!(format_parameter_plain_value(35, 1.0), "Tube");
        assert_eq!(format_parameter_plain_value(55, 1.0), "Tube");
    }

    #[test]
    fn modulation_parameters_format_as_labels() {
        assert_eq!(format_parameter_plain_value(81, 2.0), "Velocity");
        assert_eq!(format_parameter_plain_value(81, 3.0), "Pressure");
        assert_eq!(format_parameter_plain_value(81, 4.0), "Mod Wheel");
        assert_eq!(format_parameter_plain_value(81, 5.0), "Brightness");
        assert_eq!(format_parameter_plain_value(82, 0.0), "Filter Cutoff");
        assert_eq!(format_parameter_plain_value(82, 4.0), "Res B Position");
        assert_eq!(format_parameter_plain_value(86, 6.0), "LFO Rate");
    }

    #[test]
    fn telemetry_payload_roundtrips() {
        let telemetry = ResonatorTelemetry {
            left_peak: 0.25,
            right_peak: 0.5,
            left_rms: 0.125,
            right_rms: 0.375,
            active_voices: 3,
        };

        let decoded = decode_telemetry(encode_telemetry(telemetry).as_bytes()).unwrap();

        assert_eq!(decoded.left_peak, 0.25);
        assert_eq!(decoded.right_peak, 0.5);
        assert_eq!(decoded.left_rms, 0.125);
        assert_eq!(decoded.right_rms, 0.375);
        assert_eq!(decoded.active_voices, 3.0);
    }

    #[test]
    fn plugin_message_roundtrips_payload() {
        let message = PluginMessage::with_payload(MESSAGE_PATCH_UPDATE, b"patch".to_vec());
        let message = message.to_com_ptr::<IMessage>().unwrap();

        let id = unsafe { message_id(message.as_ptr()) }.unwrap();
        let payload = unsafe { message_payload(message.as_ptr()) }.unwrap();

        assert_eq!(id, MESSAGE_PATCH_UPDATE);
        assert_eq!(payload, b"patch");
    }

    #[test]
    fn controller_patch_mirror_tracks_parameter_edits() {
        let controller = ResonatorVst3Controller::new();
        let normalized = normalized_parameter_value(1, -12.0);

        assert_eq!(controller.set_value(1, normalized), kResultOk);

        assert!((controller.patch.borrow().output.master_gain_db + 12.0).abs() < 1.0e-5);
        assert_eq!(controller.editor_summary.borrow().patch_name, "Default");
    }

    #[test]
    fn controller_roundtrips_expression_slot_choices() {
        let controller = ResonatorVst3Controller::new();

        assert_eq!(
            controller.set_value(81, normalized_parameter_value(81, 4.0)),
            kResultOk
        );
        assert_eq!(
            controller.set_value(82, normalized_parameter_value(82, 5.0)),
            kResultOk
        );

        {
            let patch = controller.patch.borrow();
            assert_eq!(
                patch.modulation.slots[0].source,
                crate::ModulationSource::ModWheel
            );
            assert_eq!(
                patch.modulation.slots[0].destination,
                crate::ModulationDestination::ExcitationGain
            );
        }

        let patch = controller.patch.borrow();
        let values = parameter_values_from_patch(&patch);
        assert_parameter_value(&values, 81, 4.0);
        assert_parameter_value(&values, 82, 5.0);
    }

    #[test]
    fn controller_slot_assignment_updates_patch_and_summary_before_processor_bridge() {
        let controller = ResonatorVst3Controller::new();
        let reference = SampleReference::new("sample-hash", "Samples/kick.wav");

        let result = controller.assign_sample_reference_to_slot(reference.clone(), 2);

        assert_eq!(result, kResultFalse);
        assert_eq!(
            controller.patch.borrow().excitation_slots[2].sample,
            Some(reference)
        );
        assert_eq!(
            controller.editor_summary.borrow().slots[2].detail,
            "kick.wav"
        );
    }

    #[test]
    fn processor_notify_applies_patch_payload() {
        let processor = ResonatorVst3Processor::new();
        let patch = ResonatorSynthPatch {
            name: "Bridge Patch".to_string(),
            ..ResonatorSynthPatch::default()
        };
        let payload = patch_io::to_toml_string(&patch).unwrap().into_bytes();
        let message = PluginMessage::with_payload(MESSAGE_PATCH_UPDATE, payload);
        let message = message.to_com_ptr::<IMessage>().unwrap();

        let result = unsafe { processor.notify(message.as_ptr()) };

        assert_eq!(result, kResultOk);
        assert_eq!(processor.synth.borrow().patch().name, "Bridge Patch");
    }

    #[test]
    fn component_state_projection_covers_expanded_parameter_surface() {
        let mut patch = crate::ResonatorSynthPatch {
            output: crate::OutputConfig {
                filter_mode: crate::FilterMode::HighPass,
                filter_resonance: 0.4,
                master_pan: -0.25,
                ..crate::OutputConfig::default()
            },
            routing: crate::ResonatorRouting::Series {
                mix_a: 0.5,
                mix_b: 0.5,
            },
            resonator_a: crate::ResonatorConfig::Waveguide(crate::WaveguideConfig {
                style: crate::WaveguideStyle::Tube,
                loop_gain: 0.96,
                boundary_reflection: -0.4,
                ..crate::WaveguideConfig::default()
            }),
            resonator_b: crate::ResonatorConfig::Modal(crate::ModalConfig {
                preset: crate::ModalPreset::MetalBar,
                brightness: 0.75,
                ..crate::ModalConfig::default()
            }),
            ..crate::ResonatorSynthPatch::default()
        };
        patch.modulation.lfo.shape = crate::LfoShape::Square;
        patch.modulation.slots[0].source = crate::ModulationSource::Brightness;
        patch.modulation.slots[0].destination = crate::ModulationDestination::ResonatorBPosition;

        let values = parameter_values_from_patch(&patch);

        assert_parameter_value(&values, 5, -0.25);
        assert_parameter_value(&values, 7, 2.0);
        assert_parameter_value(&values, 10, 1.0);
        assert_parameter_value(&values, 20, 1.0);
        assert_parameter_value(&values, 32, 0.96);
        assert_parameter_value(&values, 35, 1.0);
        assert_parameter_value(&values, 36, -0.4);
        assert_parameter_value(&values, 41, 4.0);
        assert_parameter_value(&values, 46, 0.75);
        assert_parameter_value(&values, 69, 3.0);
        assert_parameter_value(&values, 81, 5.0);
        assert_parameter_value(&values, 82, 4.0);
    }

    #[test]
    fn editor_patch_summary_reflects_excitation_samples() {
        let mut patch = crate::ResonatorSynthPatch {
            name: "Sample Patch".to_string(),
            ..crate::ResonatorSynthPatch::default()
        };
        patch.excitation_slots[0].sample = Some(ahara_sample_library::SampleReference::new(
            "hash",
            "Samples/strikes/metal.wav",
        ));
        patch.excitation_slots[0].pitch_track = true;
        patch.excitation_slots[0].looping = true;

        let summary = EditorPatchSummary::from_patch(&patch);

        assert_eq!(summary.patch_name, "Sample Patch");
        assert_eq!(summary.slots[0].detail, "metal.wav");
        assert!(summary.slots[0].sample_backed);
        assert!(summary.slots[0].pitch_track);
        assert!(summary.slots[0].looping);
        assert_eq!(summary.slots[1].detail, "Empty layer");
    }

    fn assert_parameter_value(values: &[f64; VST3_PARAMETER_COUNT], id: u32, plain: f32) {
        let index = parameter_index(id).unwrap();
        let expected = normalized_parameter_value(id, plain);
        assert!(
            (values[index] - expected).abs() < 1.0e-6,
            "parameter {id} was {}, expected {expected}",
            values[index]
        );
    }
}
