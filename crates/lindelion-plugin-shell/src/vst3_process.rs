use std::{ffi::c_void, slice};

use crate::{
    AudioBuffer, AudioInputBuffer, HostMidiEvent, MidiEvent, MidiEventNormalizer, PluginState,
    TimeSignature, TransportContext,
};
use vst3::{ComRef, Steinberg::Vst::*, Steinberg::*};

const STATE_MAGIC: [u8; 4] = *b"LPS1";
const LEGACY_STATE_MAGIC: [u8; 4] = *b"AHRS";
const STATE_HEADER_BYTES: usize = 12;
const MAX_STATE_BYTES: usize = 32 * 1_048_576;

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
