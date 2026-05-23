#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, CString as StdCString, c_char, c_void},
    ptr, slice,
};

use crate::{
    AudioBuffer, AudioInputBuffer, HostMidiEvent, MidiEvent, MidiEventNormalizer, PluginDescriptor,
    PluginState, TimeSignature, TransportContext,
};
use vst3::{Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

const MESSAGE_ATTRIBUTE_PAYLOAD: &[u8] = b"payload\0";
const STATE_MAGIC: [u8; 4] = *b"LPS1";
const LEGACY_STATE_MAGIC: [u8; 4] = *b"AHRS";
const STATE_HEADER_BYTES: usize = 12;
const MAX_STATE_BYTES: usize = 32 * 1_048_576;

pub trait PluginMessageType: Copy + Eq {
    fn id(self) -> &'static str;
    fn from_id(id: &str) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedPluginMessage<M> {
    pub kind: M,
    pub payload: Vec<u8>,
}

impl<M: PluginMessageType> TypedPluginMessage<M> {
    pub fn new(kind: M, payload: Vec<u8>) -> Self {
        Self { kind, payload }
    }

    pub fn empty(kind: M) -> Self {
        Self {
            kind,
            payload: Vec::new(),
        }
    }

    pub fn id(&self) -> &'static str {
        self.kind.id()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginMessageDecodeError {
    MissingMessageId,
    MissingPayload,
    MalformedPayload,
}

/// Decode a VST3 message into a typed Lindelion message.
///
/// # Safety
/// `message` must be either null or a valid VST3 `IMessage` pointer for the duration of the call.
pub unsafe fn decode_typed_message<M: PluginMessageType>(
    message: *mut IMessage,
) -> Result<Option<TypedPluginMessage<M>>, PluginMessageDecodeError> {
    let id = message_id(message).ok_or(PluginMessageDecodeError::MissingMessageId)?;
    let Some(kind) = M::from_id(&id) else {
        return Ok(None);
    };
    let payload = message_payload(message).ok_or(PluginMessageDecodeError::MissingPayload)?;
    Ok(Some(TypedPluginMessage::new(kind, payload)))
}

pub struct PluginAttributes {
    payload: RefCell<Vec<u8>>,
}

impl PluginAttributes {
    pub fn new(payload: Vec<u8>) -> Self {
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

pub struct PluginMessage {
    message_id: RefCell<StdCString>,
    attributes: ComPtr<IAttributeList>,
}

impl PluginMessage {
    pub fn with_payload(id: &str, payload: Vec<u8>) -> ComWrapper<Self> {
        let attributes = ComWrapper::new(PluginAttributes::new(payload))
            .to_com_ptr::<IAttributeList>()
            .expect("PluginAttributes must expose IAttributeList");
        ComWrapper::new(Self {
            message_id: RefCell::new(StdCString::new(id).unwrap_or_default()),
            attributes,
        })
    }

    pub fn from_typed<M: PluginMessageType>(message: TypedPluginMessage<M>) -> ComWrapper<Self> {
        Self::with_payload(message.id(), message.payload)
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

/// Read a VST3 message id.
///
/// # Safety
/// `message` must be either null or a valid VST3 `IMessage` pointer for the duration of the call.
pub unsafe fn message_id(message: *mut IMessage) -> Option<String> {
    let message = ComRef::from_raw(message)?;
    let id = message.getMessageID();
    if id.is_null() {
        return None;
    }
    Some(CStr::from_ptr(id).to_string_lossy().into_owned())
}

/// Read the Lindelion binary payload attribute from a VST3 message.
///
/// # Safety
/// `message` must be either null or a valid VST3 `IMessage` pointer for the duration of the call.
pub unsafe fn message_payload(message: *mut IMessage) -> Option<Vec<u8>> {
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

pub fn copy_cstring(src: &str, dst: &mut [c_char]) {
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

pub fn copy_wstring(src: &str, dst: &mut [TChar]) {
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

/// Return the length of a null-terminated VST3 UTF-16 string.
///
/// # Safety
/// `string` must be either null or point to readable memory containing a null terminator.
pub unsafe fn len_wstring(string: *const TChar) -> usize {
    if string.is_null() {
        return 0;
    }

    let mut len = 0;
    while *string.add(len) != 0 {
        len += 1;
    }
    len
}

/// Project the first VST3 audio input bus into a shell audio input buffer.
///
/// # Safety
/// Any non-null pointers inside `data.inputs` must remain valid for `data.numSamples` 32-bit
/// samples and for the returned buffer lifetime.
pub unsafe fn audio_input_buffer_from_vst_process_data(data: &ProcessData) -> AudioInputBuffer<'_> {
    if data.symbolicSampleSize as SymbolicSampleSizes != SymbolicSampleSizes_::kSample32
        || data.numSamples <= 0
        || data.numInputs <= 0
        || data.inputs.is_null()
    {
        return AudioInputBuffer::empty();
    }

    let inputs = slice::from_raw_parts(data.inputs, data.numInputs as usize);
    let input = &inputs[0];
    if input.numChannels <= 0 {
        return AudioInputBuffer::empty();
    }

    let channel_buffers = input.__field0.channelBuffers32;
    if channel_buffers.is_null() {
        return AudioInputBuffer::empty();
    }

    let sample_count = data.numSamples as usize;
    let channels = slice::from_raw_parts(channel_buffers, input.numChannels as usize);
    let left = channels
        .first()
        .copied()
        .filter(|channel| !channel.is_null())
        .map(|channel| slice::from_raw_parts(channel.cast_const(), sample_count));
    let right = channels
        .get(1)
        .copied()
        .filter(|channel| !channel.is_null())
        .map(|channel| slice::from_raw_parts(channel.cast_const(), sample_count));

    AudioInputBuffer { left, right }
}

/// Project the first VST3 stereo output bus into a mutable shell audio buffer.
///
/// # Safety
/// Any non-null pointers inside `data.outputs` must remain valid for writes of `data.numSamples`
/// 32-bit samples and for the returned buffer lifetime.
pub unsafe fn stereo_output_buffers_from_vst_process_data(
    data: &mut ProcessData,
) -> Option<AudioBuffer<'_>> {
    if data.symbolicSampleSize as SymbolicSampleSizes != SymbolicSampleSizes_::kSample32
        || data.numSamples <= 0
        || data.numOutputs != 1
        || data.outputs.is_null()
    {
        return None;
    }

    let outputs = slice::from_raw_parts_mut(data.outputs, data.numOutputs as usize);
    let output = &mut outputs[0];
    if output.numChannels != 2 {
        return None;
    }

    let channel_buffers = output.__field0.channelBuffers32;
    if channel_buffers.is_null() {
        return None;
    }

    let channels = slice::from_raw_parts_mut(channel_buffers, 2);
    if channels[0].is_null() || channels[1].is_null() {
        return None;
    }

    let sample_count = data.numSamples as usize;
    Some(AudioBuffer {
        left: slice::from_raw_parts_mut(channels[0], sample_count),
        right: slice::from_raw_parts_mut(channels[1], sample_count),
    })
}

/// Clear all valid 32-bit VST3 output buses in-place.
///
/// # Safety
/// Any non-null pointers inside `data.outputs` must be valid for writes of `data.numSamples`
/// 32-bit samples.
pub unsafe fn clear_vst_outputs(data: &mut ProcessData) {
    if data.symbolicSampleSize as SymbolicSampleSizes != SymbolicSampleSizes_::kSample32
        || data.numSamples <= 0
        || data.numOutputs <= 0
        || data.outputs.is_null()
    {
        return;
    }

    let outputs = slice::from_raw_parts_mut(data.outputs, data.numOutputs as usize);
    for output in outputs {
        clear_vst_output_bus(output, data.numSamples as usize);
    }
}

/// Project a VST3 process context into Lindelion's host transport context.
///
/// # Safety
/// `context` must be either null or a valid VST3 `ProcessContext` pointer for the duration of the
/// call.
pub unsafe fn transport_context_from_vst_process_context(
    context: *const ProcessContext,
) -> TransportContext {
    let Some(context) = context.as_ref() else {
        return TransportContext::default();
    };
    let state = context.state;
    let cycle_valid = context_flag_is_set(state, ProcessContext_::StatesAndFlags_::kCycleValid);

    TransportContext {
        playing: context_flag_is_set(state, ProcessContext_::StatesAndFlags_::kPlaying),
        recording: context_flag_is_set(state, ProcessContext_::StatesAndFlags_::kRecording),
        sample_position: (context.projectTimeSamples >= 0).then_some(context.projectTimeSamples),
        project_quarter_note: finite_context_value(
            state,
            ProcessContext_::StatesAndFlags_::kProjectTimeMusicValid,
            context.projectTimeMusic,
        ),
        bar_position_quarter_note: finite_context_value(
            state,
            ProcessContext_::StatesAndFlags_::kBarPositionValid,
            context.barPositionMusic,
        ),
        cycle_active: context_flag_is_set(state, ProcessContext_::StatesAndFlags_::kCycleActive),
        cycle_start_quarter_note: cycle_valid
            .then_some(context.cycleStartMusic)
            .filter(|value| value.is_finite()),
        cycle_end_quarter_note: cycle_valid
            .then_some(context.cycleEndMusic)
            .filter(|value| value.is_finite()),
        tempo_bpm: (context_flag_is_set(state, ProcessContext_::StatesAndFlags_::kTempoValid)
            && context.tempo.is_finite()
            && context.tempo > 0.0)
            .then_some(context.tempo),
        time_signature: time_signature_from_vst_context(state, context),
    }
}

/// Read a Lindelion plugin state envelope from a VST3 stream.
///
/// # Safety
/// `stream` must be either null or a valid VST3 `IBStream` pointer for the duration of the call.
pub unsafe fn read_plugin_state_from_stream(stream: *mut IBStream) -> Option<PluginState> {
    let stream = ComRef::from_raw(stream)?;
    let mut header = [0; STATE_HEADER_BYTES];
    if !read_exact(&stream, &mut header) {
        return None;
    }
    if header[..4] != STATE_MAGIC && header[..4] != LEGACY_STATE_MAGIC {
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

/// Write a Lindelion plugin state envelope to a VST3 stream.
///
/// # Safety
/// `stream` must be either null or a valid writable VST3 `IBStream` pointer for the duration of
/// the call.
pub unsafe fn write_plugin_state_to_stream(stream: *mut IBStream, state: PluginState) -> bool {
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

/// Convert and normalize a VST3 event into a Lindelion MIDI event.
///
/// # Safety
/// `event` must contain the union field implied by its VST3 event type.
pub unsafe fn vst_event_to_midi(
    event: Event,
    normalizer: MidiEventNormalizer<'_>,
) -> Option<MidiEvent> {
    normalizer.normalize(vst_event_to_host_midi(event)?)
}

/// Convert a VST3 event into an unsanitized host MIDI event.
///
/// # Safety
/// `event` must contain the union field implied by its VST3 event type.
pub unsafe fn vst_event_to_host_midi(event: Event) -> Option<HostMidiEvent> {
    match event.r#type as Event_::EventTypes {
        Event_::EventTypes_::kNoteOnEvent => {
            let note = event.__field0.noteOn;
            Some(HostMidiEvent::NoteOn {
                channel: i32::from(note.channel),
                note: i32::from(note.pitch),
                velocity: note.velocity,
            })
        }
        Event_::EventTypes_::kNoteOffEvent => {
            let note = event.__field0.noteOff;
            Some(HostMidiEvent::NoteOff {
                channel: i32::from(note.channel),
                note: i32::from(note.pitch),
                velocity: note.velocity,
            })
        }
        Event_::EventTypes_::kPolyPressureEvent => {
            let pressure = event.__field0.polyPressure;
            Some(HostMidiEvent::PolyPressure {
                channel: i32::from(pressure.channel),
                note: i32::from(pressure.pitch),
                pressure: pressure.pressure,
            })
        }
        Event_::EventTypes_::kLegacyMIDICCOutEvent => {
            legacy_midi_cc_to_host_event(event.__field0.midiCCOut)
        }
        _ => None,
    }
}

fn legacy_midi_cc_to_host_event(event: LegacyMIDICCOutEvent) -> Option<HostMidiEvent> {
    let channel = i32::from(event.channel);
    match u32::from(event.controlNumber) {
        ControllerNumbers_::kAfterTouch => Some(HostMidiEvent::ChannelPressure {
            channel,
            value: i32::from(event.value),
        }),
        ControllerNumbers_::kPitchBend => Some(HostMidiEvent::PitchBend {
            channel,
            lsb: i32::from(event.value),
            msb: i32::from(event.value2),
        }),
        control_number => Some(HostMidiEvent::ContinuousController {
            channel,
            controller: control_number,
            value: i32::from(event.value),
        }),
    }
}

fn context_flag_is_set(state: u32, flag: ProcessContext_::StatesAndFlags) -> bool {
    state & flag != 0
}

fn finite_context_value(
    state: u32,
    flag: ProcessContext_::StatesAndFlags,
    value: f64,
) -> Option<f64> {
    (context_flag_is_set(state, flag) && value.is_finite()).then_some(value)
}

fn time_signature_from_vst_context(state: u32, context: &ProcessContext) -> Option<TimeSignature> {
    if !context_flag_is_set(state, ProcessContext_::StatesAndFlags_::kTimeSigValid)
        || context.timeSigNumerator <= 0
        || context.timeSigDenominator <= 0
    {
        return None;
    }

    Some(TimeSignature::new(
        context.timeSigNumerator,
        context.timeSigDenominator,
    ))
}

unsafe fn clear_vst_output_bus(output: &mut AudioBusBuffers, sample_count: usize) {
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

pub type Vst3CreateInstance = fn() -> ComPtr<FUnknown>;

#[derive(Debug, Clone, Copy)]
pub struct Vst3ClassRegistration {
    pub cid: TUID,
    pub category: &'static str,
    pub name: &'static str,
    pub class_flags: u32,
    pub subcategories: &'static str,
    pub create: Vst3CreateInstance,
}

impl Vst3ClassRegistration {
    pub const fn audio_processor(
        cid: TUID,
        name: &'static str,
        subcategories: &'static str,
        create: Vst3CreateInstance,
    ) -> Self {
        Self {
            cid,
            category: "Audio Module Class",
            name,
            class_flags: ComponentFlags_::kDistributable,
            subcategories,
            create,
        }
    }

    pub const fn edit_controller(
        cid: TUID,
        name: &'static str,
        create: Vst3CreateInstance,
    ) -> Self {
        Self {
            cid,
            category: "Component Controller Class",
            name,
            class_flags: 0,
            subcategories: "",
            create,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Vst3PluginFactory {
    descriptor: &'static PluginDescriptor,
    classes: &'static [Vst3ClassRegistration],
    sdk_version: &'static str,
}

impl Vst3PluginFactory {
    pub const fn new(
        descriptor: &'static PluginDescriptor,
        classes: &'static [Vst3ClassRegistration],
    ) -> Self {
        Self {
            descriptor,
            classes,
            sdk_version: "VST 3.8.0",
        }
    }

    pub const fn class_count(&self) -> usize {
        self.classes.len()
    }

    pub fn class(&self, index: i32) -> Option<Vst3ClassRegistration> {
        let index = usize::try_from(index).ok()?;
        self.classes.get(index).copied()
    }
}

impl Class for Vst3PluginFactory {
    type Interfaces = (IPluginFactory, IPluginFactory2, IPluginFactory3);
}

impl IPluginFactoryTrait for Vst3PluginFactory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        copy_cstring(self.descriptor.vendor, &mut info.vendor);
        copy_cstring(self.descriptor.url, &mut info.url);
        copy_cstring(self.descriptor.email, &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as i32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        self.classes.len().min(i32::MAX as usize) as i32
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = self.class(index) else {
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
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = ptr::null_mut();

        let requested_cid = *(cid as *const TUID);
        let Some(class) = self
            .classes
            .iter()
            .copied()
            .find(|class| class.cid == requested_cid)
        else {
            return kInvalidArgument;
        };

        let instance = (class.create)();
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

impl IPluginFactory2Trait for Vst3PluginFactory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = self.class(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info2(self.descriptor, self.sdk_version, &mut *info);
        kResultOk
    }
}

impl IPluginFactory3Trait for Vst3PluginFactory {
    unsafe fn getClassInfoUnicode(&self, index: i32, info: *mut PClassInfoW) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(class) = self.class(index) else {
            return kInvalidArgument;
        };
        class.fill_class_info_w(self.descriptor, self.sdk_version, &mut *info);
        kResultOk
    }

    unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
}

impl Vst3ClassRegistration {
    fn fill_class_info(self, info: &mut PClassInfo) {
        info.cid = self.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category, &mut info.category);
        copy_cstring(self.name, &mut info.name);
    }

    fn fill_class_info2(
        self,
        descriptor: &PluginDescriptor,
        sdk_version: &str,
        info: &mut PClassInfo2,
    ) {
        info.cid = self.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category, &mut info.category);
        copy_cstring(self.name, &mut info.name);
        info.classFlags = self.class_flags;
        copy_cstring(self.subcategories, &mut info.subCategories);
        copy_cstring(descriptor.vendor, &mut info.vendor);
        copy_cstring(descriptor.version, &mut info.version);
        copy_cstring(sdk_version, &mut info.sdkVersion);
    }

    fn fill_class_info_w(
        self,
        descriptor: &PluginDescriptor,
        sdk_version: &str,
        info: &mut PClassInfoW,
    ) {
        info.cid = self.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_cstring(self.category, &mut info.category);
        copy_wstring(self.name, &mut info.name);
        info.classFlags = self.class_flags;
        copy_cstring(self.subcategories, &mut info.subCategories);
        copy_wstring(descriptor.vendor, &mut info.vendor);
        copy_wstring(descriptor.version, &mut info.version);
        copy_wstring(sdk_version, &mut info.sdkVersion);
    }
}

pub fn plugin_factory_ptr(factory: Vst3PluginFactory) -> *mut IPluginFactory {
    ComWrapper::new(factory)
        .to_com_ptr::<IPluginFactory>()
        .expect("Vst3PluginFactory must expose IPluginFactory")
        .into_raw()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedSizePlugViewSize {
    pub width: i32,
    pub height: i32,
}

impl FixedSizePlugViewSize {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    pub fn view_rect(self) -> ViewRect {
        ViewRect {
            left: 0,
            top: 0,
            right: self.width.max(0),
            bottom: self.height.max(0),
        }
    }

    fn clamp_rect(self, rect: ViewRect) -> ViewRect {
        ViewRect {
            left: rect.left,
            top: rect.top,
            right: rect.left + self.width.max(0),
            bottom: rect.top + self.height.max(0),
        }
    }
}

pub trait FixedSizePlugViewDelegate {
    /// # Safety
    /// `parent` must be a valid platform view pointer for the host platform and `size` must
    /// describe the attached view bounds.
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult;

    /// # Safety
    /// The host must call this only while the delegated platform view is still owned by the plug
    /// view.
    unsafe fn removed(&self) -> tresult {
        kResultOk
    }
}

pub struct FixedSizePlugView<D> {
    delegate: D,
    frame: Cell<*mut IPlugFrame>,
    size: Cell<ViewRect>,
    fixed_size: FixedSizePlugViewSize,
}

impl<D> FixedSizePlugView<D> {
    pub fn new(delegate: D, fixed_size: FixedSizePlugViewSize) -> Self {
        Self {
            delegate,
            frame: Cell::new(ptr::null_mut()),
            size: Cell::new(fixed_size.view_rect()),
            fixed_size,
        }
    }
}

impl<D: 'static> Class for FixedSizePlugView<D> {
    type Interfaces = (IPlugView,);
}

impl<D: FixedSizePlugViewDelegate> IPlugViewTrait for FixedSizePlugView<D> {
    unsafe fn isPlatformTypeSupported(&self, r#type: FIDString) -> tresult {
        #[cfg(target_os = "macos")]
        {
            if is_ns_view_platform(r#type) {
                kResultTrue
            } else {
                kResultFalse
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = r#type;
            kResultFalse
        }
    }

    unsafe fn attached(&self, parent: *mut c_void, r#type: FIDString) -> tresult {
        if parent.is_null() {
            return kInvalidArgument;
        }
        if self.isPlatformTypeSupported(r#type) != kResultTrue {
            return kResultFalse;
        }

        #[cfg(target_os = "macos")]
        {
            self.delegate.attached(parent, self.size.get())
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = parent;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        self.delegate.removed()
    }

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyDown(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyUp(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        *size = self.size.get();
        kResultOk
    }

    unsafe fn onSize(&self, newSize: *mut ViewRect) -> tresult {
        if newSize.is_null() {
            return kInvalidArgument;
        }
        let size = self.fixed_size.clamp_rect(*newSize);
        self.size.set(size);
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
        kResultFalse
    }

    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        if rect.is_null() {
            return kInvalidArgument;
        }
        *rect = self.fixed_size.view_rect();
        kResultOk
    }
}

#[cfg(target_os = "macos")]
unsafe fn is_ns_view_platform(platform: FIDString) -> bool {
    !platform.is_null() && CStr::from_ptr(platform as *const c_char).to_bytes() == b"NSView"
}

#[macro_export]
macro_rules! export_vst3_entrypoints {
    ($factory:expr) => {
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
        pub extern "C" fn bundleEntry(_bundle_ref: *mut ::std::ffi::c_void) -> bool {
            true
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn bundleExit() -> bool {
            true
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn BundleEntry(bundle_ref: *mut ::std::ffi::c_void) -> bool {
            bundleEntry(bundle_ref)
        }

        #[cfg(target_os = "macos")]
        #[unsafe(no_mangle)]
        pub extern "C" fn BundleExit() -> bool {
            bundleExit()
        }

        #[cfg(target_os = "linux")]
        #[unsafe(no_mangle)]
        pub extern "system" fn ModuleEntry(_library_handle: *mut ::std::ffi::c_void) -> bool {
            true
        }

        #[cfg(target_os = "linux")]
        #[unsafe(no_mangle)]
        pub extern "system" fn ModuleExit() -> bool {
            true
        }

        #[unsafe(no_mangle)]
        pub extern "system" fn GetPluginFactory() -> *mut ::vst3::Steinberg::IPluginFactory {
            $crate::vst3::plugin_factory_ptr($factory)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::{Interface, uid};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestMessage {
        PatchUpdate,
        TelemetryRequest,
    }

    impl PluginMessageType for TestMessage {
        fn id(self) -> &'static str {
            match self {
                Self::PatchUpdate => "lindelion.test.patch_update",
                Self::TelemetryRequest => "lindelion.test.telemetry_request",
            }
        }

        fn from_id(id: &str) -> Option<Self> {
            match id {
                "lindelion.test.patch_update" => Some(Self::PatchUpdate),
                "lindelion.test.telemetry_request" => Some(Self::TelemetryRequest),
                _ => None,
            }
        }
    }

    #[test]
    fn typed_message_roundtrips_payload() {
        let expected = TypedPluginMessage::new(TestMessage::PatchUpdate, b"patch".to_vec());
        let message = PluginMessage::from_typed(expected.clone())
            .to_com_ptr::<IMessage>()
            .unwrap();

        let decoded = unsafe { decode_typed_message::<TestMessage>(message.as_ptr()) };

        assert_eq!(decoded, Ok(Some(expected)));
    }

    #[test]
    fn unknown_message_ids_are_ignored() {
        let message = PluginMessage::with_payload("lindelion.test.unknown", Vec::new())
            .to_com_ptr::<IMessage>()
            .unwrap();

        let decoded = unsafe { decode_typed_message::<TestMessage>(message.as_ptr()) };

        assert_eq!(decoded, Ok(None));
    }

    #[test]
    fn malformed_message_payload_returns_error_instead_of_panicking() {
        let message = ComWrapper::new(MessageWithoutAttributes::new(TestMessage::PatchUpdate.id()))
            .to_com_ptr::<IMessage>()
            .unwrap();

        let decoded = unsafe { decode_typed_message::<TestMessage>(message.as_ptr()) };

        assert_eq!(decoded, Err(PluginMessageDecodeError::MissingPayload));
    }

    #[test]
    fn string_helpers_null_terminate_truncated_strings() {
        let mut text = [1 as c_char; 4];
        copy_cstring("abcd", &mut text);

        assert_eq!(text[3], 0);

        let mut wide = [1 as TChar; 4];
        copy_wstring("abcd", &mut wide);

        assert_eq!(wide[3], 0);
    }

    #[test]
    fn string_helpers_roundtrip_ascii_and_unicode_without_overflow() {
        let mut text = [1 as c_char; 16];
        copy_cstring("Lindelion", &mut text);

        assert_eq!(c_string(&text), "Lindelion");
        assert_eq!(text[10], 1);

        let mut wide = [1 as TChar; 16];
        copy_wstring("Résonateur", &mut wide);

        assert_eq!(wide_string(&wide), "Résonateur");
        assert_eq!(wide[10], 0);
        assert_eq!(wide[11], 1);
    }

    #[test]
    fn vst_process_data_projects_mono_audio_input_buffer() {
        let mono = [0.25_f32, -0.5];
        let mut channels = [mono.as_ptr() as *mut Sample32];
        let mut input_bus = AudioBusBuffers {
            numChannels: 1,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: channels.as_mut_ptr(),
            },
        };
        let data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: 2,
            numInputs: 1,
            numOutputs: 0,
            inputs: &mut input_bus,
            outputs: ptr::null_mut(),
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        let input = unsafe { audio_input_buffer_from_vst_process_data(&data) };

        assert_eq!(input.len(), 2);
        assert_eq!(input.mono_sample(0), 0.25);
        assert_eq!(input.mono_sample(1), -0.5);
    }

    #[test]
    fn vst_process_data_projects_stereo_audio_input_buffer() {
        let left = [0.0_f32, 0.5];
        let right = [1.0_f32, -0.25];
        let mut channels = [
            left.as_ptr() as *mut Sample32,
            right.as_ptr() as *mut Sample32,
        ];
        let mut input_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: channels.as_mut_ptr(),
            },
        };
        let data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: 2,
            numInputs: 1,
            numOutputs: 0,
            inputs: &mut input_bus,
            outputs: ptr::null_mut(),
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        let input = unsafe { audio_input_buffer_from_vst_process_data(&data) };

        assert_eq!(input.len(), 2);
        assert_eq!(input.mono_sample(0), 0.5);
        assert_eq!(input.mono_sample(1), 0.125);
    }

    #[test]
    fn vst_process_data_rejects_invalid_audio_input_buffers() {
        let left = [0.0_f32, 0.5];
        let mut channels = [left.as_ptr() as *mut Sample32];
        let mut input_bus = AudioBusBuffers {
            numChannels: 1,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: channels.as_mut_ptr(),
            },
        };
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample64 as i32,
            numSamples: 2,
            numInputs: 1,
            numOutputs: 0,
            inputs: &mut input_bus,
            outputs: ptr::null_mut(),
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        assert!(unsafe { audio_input_buffer_from_vst_process_data(&data) }.is_empty());

        data.symbolicSampleSize = SymbolicSampleSizes_::kSample32 as i32;
        data.inputs = ptr::null_mut();
        assert!(unsafe { audio_input_buffer_from_vst_process_data(&data) }.is_empty());

        let mut null_channel_bus = AudioBusBuffers {
            numChannels: 1,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: ptr::null_mut(),
            },
        };
        data.inputs = &mut null_channel_bus;
        assert!(unsafe { audio_input_buffer_from_vst_process_data(&data) }.is_empty());
    }

    #[test]
    fn vst_process_data_projects_stereo_output_buffer() {
        let mut left = [1.0_f32, 2.0];
        let mut right = [3.0_f32, 4.0];
        let mut channels = [left.as_mut_ptr(), right.as_mut_ptr()];
        let mut output_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: channels.as_mut_ptr(),
            },
        };
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: 2,
            numInputs: 0,
            numOutputs: 1,
            inputs: ptr::null_mut(),
            outputs: &mut output_bus,
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        let buffer = unsafe { stereo_output_buffers_from_vst_process_data(&mut data) }
            .expect("stereo output should project");
        buffer.left[0] = -1.0;
        buffer.right[1] = -4.0;

        assert_eq!(left, [-1.0, 2.0]);
        assert_eq!(right, [3.0, -4.0]);
    }

    #[test]
    fn clear_vst_outputs_clears_valid_32_bit_outputs() {
        let mut left = [1.0_f32, 2.0];
        let mut right = [3.0_f32, 4.0];
        let mut channels = [left.as_mut_ptr(), right.as_mut_ptr()];
        let mut output_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: channels.as_mut_ptr(),
            },
        };
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: 2,
            numInputs: 0,
            numOutputs: 1,
            inputs: ptr::null_mut(),
            outputs: &mut output_bus,
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        unsafe { clear_vst_outputs(&mut data) };

        assert_eq!(left, [0.0, 0.0]);
        assert_eq!(right, [0.0, 0.0]);
    }

    #[test]
    fn vst_process_context_projects_full_transport_context() {
        let mut context = unsafe { std::mem::zeroed::<ProcessContext>() };
        context.state = full_transport_flags();
        context.projectTimeSamples = 4_096;
        context.projectTimeMusic = 12.0;
        context.barPositionMusic = 16.0;
        context.cycleStartMusic = 8.0;
        context.cycleEndMusic = 24.0;
        context.tempo = 128.0;
        context.timeSigNumerator = 7;
        context.timeSigDenominator = 8;

        let transport = unsafe { transport_context_from_vst_process_context(&context) };

        assert_full_transport(transport);
    }

    #[test]
    fn vst_process_context_rejects_null_and_invalid_transport_values() {
        assert_eq!(
            unsafe { transport_context_from_vst_process_context(ptr::null()) },
            TransportContext::default()
        );

        let mut context = unsafe { std::mem::zeroed::<ProcessContext>() };
        context.state = invalid_transport_flags();
        context.projectTimeSamples = -1;
        context.projectTimeMusic = f64::NAN;
        context.barPositionMusic = f64::INFINITY;
        context.cycleStartMusic = f64::NAN;
        context.cycleEndMusic = f64::INFINITY;
        context.tempo = -120.0;
        context.timeSigNumerator = 0;
        context.timeSigDenominator = 0;

        let transport = unsafe { transport_context_from_vst_process_context(&context) };

        assert_eq!(transport.sample_position, None);
        assert_eq!(transport.project_quarter_note, None);
        assert_eq!(transport.bar_position_quarter_note, None);
        assert_eq!(transport.cycle_start_quarter_note, None);
        assert_eq!(transport.cycle_end_quarter_note, None);
        assert_eq!(transport.tempo_bpm, None);
        assert_eq!(transport.time_signature, None);
    }

    #[test]
    fn factory_enumerates_registered_classes_through_ipluginfactory() {
        let factory = unsafe { ComPtr::from_raw(plugin_factory_ptr(test_vst3_factory())).unwrap() };

        assert_eq!(unsafe { factory.countClasses() }, 2);

        let mut processor = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(0, &mut processor) },
            kResultOk
        );
        assert_eq!(processor.cid, TEST_PROCESSOR_CID);
        assert_eq!(c_string(&processor.category), "Audio Module Class");
        assert_eq!(c_string(&processor.name), "Lindelion Test Processor");

        let mut controller = unsafe { std::mem::zeroed::<PClassInfo>() };
        assert_eq!(
            unsafe { factory.getClassInfo(1, &mut controller) },
            kResultOk
        );
        assert_eq!(controller.cid, TEST_CONTROLLER_CID);
        assert_eq!(c_string(&controller.category), "Component Controller Class");
        assert_eq!(c_string(&controller.name), "Lindelion Test Controller");
    }

    #[test]
    fn factory_dispatches_class_creation_by_cid() {
        let factory = test_vst3_factory();
        let mut obj = ptr::null_mut::<c_void>();

        assert_eq!(
            unsafe {
                factory.createInstance(
                    TEST_PROCESSOR_CID.as_ptr(),
                    IPluginBase::IID.as_ptr().cast(),
                    &mut obj,
                )
            },
            kResultOk
        );

        let plugin_base = unsafe { ComPtr::from_raw(obj.cast::<IPluginBase>()).unwrap() };
        assert_eq!(
            unsafe { plugin_base.initialize(ptr::null_mut()) },
            kResultOk
        );

        let mut missing = std::ptr::dangling_mut::<c_void>();
        assert_eq!(
            unsafe {
                factory.createInstance(
                    TEST_UNKNOWN_CID.as_ptr(),
                    IPluginBase::IID.as_ptr().cast(),
                    &mut missing,
                )
            },
            kInvalidArgument
        );
        assert!(missing.is_null());
    }

    #[test]
    fn fixed_size_plug_view_reports_and_enforces_declared_size() {
        let view =
            FixedSizePlugView::new(TestPlugViewDelegate, FixedSizePlugViewSize::new(320, 180));

        let mut size = unsafe { std::mem::zeroed::<ViewRect>() };
        assert_eq!(unsafe { view.getSize(&mut size) }, kResultOk);
        assert_rect(size, 0, 0, 320, 180);

        let mut requested = rect(12, 24, 640, 480);
        assert_eq!(unsafe { view.onSize(&mut requested) }, kResultOk);
        assert_eq!(unsafe { view.getSize(&mut size) }, kResultOk);
        assert_rect(size, 12, 24, 332, 204);

        assert_eq!(unsafe { view.canResize() }, kResultFalse);

        let mut constrained = rect(8, 16, 100, 100);
        assert_eq!(
            unsafe { view.checkSizeConstraint(&mut constrained) },
            kResultOk
        );
        assert_rect(constrained, 0, 0, 320, 180);
    }

    struct MessageWithoutAttributes {
        message_id: RefCell<StdCString>,
    }

    impl MessageWithoutAttributes {
        fn new(id: &str) -> Self {
            Self {
                message_id: RefCell::new(StdCString::new(id).unwrap()),
            }
        }
    }

    impl Class for MessageWithoutAttributes {
        type Interfaces = (IMessage,);
    }

    impl IMessageTrait for MessageWithoutAttributes {
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
            ptr::null_mut()
        }
    }

    static TEST_DESCRIPTOR: PluginDescriptor =
        PluginDescriptor::instrument("Lindelion Test", *b"lindelion_test!!");

    const TEST_PROCESSOR_CID: TUID = uid(0x98E5D65D, 0x3B32489D, 0x89498A31, 0x4544F110);
    const TEST_CONTROLLER_CID: TUID = uid(0x2B77C756, 0x2E144A2A, 0xB05B702D, 0x797DD064);
    const TEST_UNKNOWN_CID: TUID = uid(0x530C977B, 0xB1004DB7, 0xB24EF0FE, 0xF7F1A040);

    const TEST_CLASSES: &[Vst3ClassRegistration] = &[
        Vst3ClassRegistration::audio_processor(
            TEST_PROCESSOR_CID,
            "Lindelion Test Processor",
            "Instrument|Synth",
            create_test_component,
        ),
        Vst3ClassRegistration::edit_controller(
            TEST_CONTROLLER_CID,
            "Lindelion Test Controller",
            create_test_component,
        ),
    ];

    fn test_vst3_factory() -> Vst3PluginFactory {
        Vst3PluginFactory::new(&TEST_DESCRIPTOR, TEST_CLASSES)
    }

    fn create_test_component() -> ComPtr<FUnknown> {
        ComWrapper::new(TestPluginBase)
            .to_com_ptr::<FUnknown>()
            .expect("TestPluginBase must expose FUnknown")
    }

    struct TestPluginBase;

    impl Class for TestPluginBase {
        type Interfaces = (IPluginBase,);
    }

    impl IPluginBaseTrait for TestPluginBase {
        unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
            kResultOk
        }

        unsafe fn terminate(&self) -> tresult {
            kResultOk
        }
    }

    struct TestPlugViewDelegate;

    impl FixedSizePlugViewDelegate for TestPlugViewDelegate {
        unsafe fn attached(&self, _parent: *mut c_void, _size: ViewRect) -> tresult {
            kResultOk
        }
    }

    fn c_string(buffer: &[c_char]) -> String {
        unsafe {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }

    fn wide_string(buffer: &[TChar]) -> String {
        let len = unsafe { len_wstring(buffer.as_ptr()) };
        let chars = buffer[..len].to_vec();
        String::from_utf16(&chars).unwrap()
    }

    fn full_transport_flags() -> u32 {
        ProcessContext_::StatesAndFlags_::kPlaying
            | ProcessContext_::StatesAndFlags_::kRecording
            | ProcessContext_::StatesAndFlags_::kProjectTimeMusicValid
            | ProcessContext_::StatesAndFlags_::kTempoValid
            | ProcessContext_::StatesAndFlags_::kTimeSigValid
            | ProcessContext_::StatesAndFlags_::kBarPositionValid
            | ProcessContext_::StatesAndFlags_::kCycleActive
            | ProcessContext_::StatesAndFlags_::kCycleValid
    }

    fn invalid_transport_flags() -> u32 {
        ProcessContext_::StatesAndFlags_::kProjectTimeMusicValid
            | ProcessContext_::StatesAndFlags_::kBarPositionValid
            | ProcessContext_::StatesAndFlags_::kCycleValid
            | ProcessContext_::StatesAndFlags_::kTempoValid
            | ProcessContext_::StatesAndFlags_::kTimeSigValid
    }

    fn assert_full_transport(transport: TransportContext) {
        assert_eq!(
            transport,
            TransportContext {
                playing: true,
                recording: true,
                sample_position: Some(4_096),
                project_quarter_note: Some(12.0),
                bar_position_quarter_note: Some(16.0),
                cycle_active: true,
                cycle_start_quarter_note: Some(8.0),
                cycle_end_quarter_note: Some(24.0),
                tempo_bpm: Some(128.0),
                time_signature: Some(TimeSignature::new(7, 8)),
            }
        );
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ViewRect {
        ViewRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn assert_rect(rect: ViewRect, left: i32, top: i32, right: i32, bottom: i32) {
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (left, top, right, bottom)
        );
    }
}
