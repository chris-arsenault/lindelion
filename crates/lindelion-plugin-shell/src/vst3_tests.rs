use super::*;
use std::{
    ffi::{CStr, c_char, c_void},
    ptr,
};

use crate::{PluginDescriptor, TimeSignature, TransportContext};
use vst3::{Class, ComPtr, ComWrapper, Interface, Steinberg::Vst::*, Steinberg::*, uid};

#[path = "vst3_tests/messages.rs"]
mod messages;
#[path = "vst3_tests/parameters.rs"]
mod parameters;
#[path = "vst3_tests/view.rs"]
mod view;

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
fn bus_info_defaults_existing_constructors_to_main_active_buses() {
    let buses = [
        Vst3BusInfo::audio_input(2, "Input"),
        Vst3BusInfo::audio_output(2, "Output"),
        Vst3BusInfo::event_input(1, "MIDI Input"),
    ];

    let audio_input = filled_bus_info(&buses, audio(), input(), 0);
    assert_eq!(audio_input.channelCount, 2);
    assert_eq!(wide_string(&audio_input.name), "Input");
    assert_eq!(audio_input.busType, BusTypes_::kMain as BusType);
    assert_eq!(audio_input.flags, BusInfo_::BusFlags_::kDefaultActive);

    let event_input = filled_bus_info(&buses, event(), input(), 0);
    assert_eq!(event_input.channelCount, 1);
    assert_eq!(wide_string(&event_input.name), "MIDI Input");
    assert_eq!(event_input.busType, BusTypes_::kMain as BusType);
    assert_eq!(event_input.flags, BusInfo_::BusFlags_::kDefaultActive);
}

#[test]
fn bus_info_supports_optional_aux_audio_input() {
    let buses = [
        Vst3BusInfo::audio_output(2, "Output"),
        Vst3BusInfo::optional_audio_input(2, "Sidechain Input"),
    ];

    assert_eq!(vst3_bus_count(&buses, audio(), input()), 1);
    assert_eq!(vst3_bus_count(&buses, audio(), output()), 1);

    let sidechain = filled_bus_info(&buses, audio(), input(), 0);
    assert_eq!(sidechain.channelCount, 2);
    assert_eq!(wide_string(&sidechain.name), "Sidechain Input");
    assert_eq!(sidechain.busType, BusTypes_::kAux as BusType);
    assert_eq!(sidechain.flags & BusInfo_::BusFlags_::kDefaultActive, 0);

    let output_bus = filled_bus_info(&buses, audio(), output(), 0);
    assert_eq!(output_bus.busType, BusTypes_::kMain as BusType);
    assert_eq!(output_bus.flags, BusInfo_::BusFlags_::kDefaultActive);
}

fn filled_bus_info(
    buses: &[Vst3BusInfo],
    media_type: MediaType,
    direction: BusDirection,
    index: i32,
) -> BusInfo {
    let mut bus = unsafe { std::mem::zeroed::<BusInfo>() };
    assert_eq!(
        unsafe { fill_vst3_bus_info(buses, media_type, direction, index, &mut bus) },
        kResultOk
    );
    bus
}

#[test]
fn mono_or_stereo_speaker_arrangement_supports_audio_input_shapes() {
    assert!(mono_or_stereo_speaker_arrangement_supported(
        SpeakerArr::kMono
    ));
    assert!(mono_or_stereo_speaker_arrangement_supported(
        SpeakerArr::kStereo
    ));
    assert!(!mono_or_stereo_speaker_arrangement_supported(
        SpeakerArr::k51
    ));
    assert!(!mono_or_stereo_speaker_arrangement_supported(0));
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

fn audio() -> MediaType {
    MediaTypes_::kAudio as MediaType
}

fn event() -> MediaType {
    MediaTypes_::kEvent as MediaType
}

fn input() -> BusDirection {
    BusDirections_::kInput as BusDirection
}

fn output() -> BusDirection {
    BusDirections_::kOutput as BusDirection
}
