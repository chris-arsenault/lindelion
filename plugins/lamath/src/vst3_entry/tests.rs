#![allow(clippy::cognitive_complexity)]

use std::ptr;

use super::*;
use crate::{ResonatorSidechainTelemetry, ResonatorSynthPatch, ResonatorTelemetry, patch_io};
use lindelion_plugin_shell::{
    ControlEvent, MidiEvent, MidiEventNormalizer,
    vst3::{PluginMessage, PluginMessageDecodeError, PluginMessageType, len_wstring},
};
use lindelion_sample_library::SampleReference;
use vst3::{Steinberg::Vst::*, Steinberg::*};

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
fn processor_exposes_optional_sidechain_audio_bus_and_midi_input() {
    let processor = ResonatorVst3Processor::new();

    assert_eq!(unsafe { processor.getBusCount(audio(), input()) }, 1);
    assert_eq!(unsafe { processor.getBusCount(audio(), output()) }, 1);
    assert_eq!(unsafe { processor.getBusCount(event(), input()) }, 1);
    assert_eq!(unsafe { processor.getBusCount(event(), output()) }, 0);

    let mut sidechain = unsafe { std::mem::zeroed::<BusInfo>() };
    assert_eq!(
        unsafe { processor.getBusInfo(audio(), input(), 0, &mut sidechain) },
        kResultOk
    );
    assert_eq!(sidechain.channelCount, 2);
    assert_eq!(wide_string(&sidechain.name), "Sidechain Input");
    assert_eq!(sidechain.busType, BusTypes_::kAux as BusType);
    assert_eq!(sidechain.flags & BusInfo_::BusFlags_::kDefaultActive, 0);

    let mut output_bus = unsafe { std::mem::zeroed::<BusInfo>() };
    assert_eq!(
        unsafe { processor.getBusInfo(audio(), output(), 0, &mut output_bus) },
        kResultOk
    );
    assert_eq!(output_bus.channelCount, 2);
    assert_eq!(wide_string(&output_bus.name), "Output");
    assert_eq!(output_bus.busType, BusTypes_::kMain as BusType);
    assert_eq!(output_bus.flags, BusInfo_::BusFlags_::kDefaultActive);

    let mut midi_bus = unsafe { std::mem::zeroed::<BusInfo>() };
    assert_eq!(
        unsafe { processor.getBusInfo(event(), input(), 0, &mut midi_bus) },
        kResultOk
    );
    assert_eq!(midi_bus.channelCount, 1);
    assert_eq!(wide_string(&midi_bus.name), "MIDI Input");
}

#[test]
fn bus_arrangements_accept_optional_mono_or_stereo_sidechain_and_stereo_output() {
    let processor = ResonatorVst3Processor::new();
    let mut stereo_output = SpeakerArr::kStereo;
    let mut mono_output = SpeakerArr::kMono;
    let mut empty_input = SpeakerArr::kEmpty;
    let mut mono_input = SpeakerArr::kMono;
    let mut stereo_input = SpeakerArr::kStereo;
    let mut surround_input = SpeakerArr::k51;

    assert_eq!(
        unsafe { processor.setBusArrangements(ptr::null_mut(), 0, &mut stereo_output, 1) },
        kResultTrue
    );

    assert_eq!(
        unsafe { processor.setBusArrangements(&mut empty_input, 1, &mut stereo_output, 1) },
        kResultTrue
    );
    let mut active_input = SpeakerArr::kStereo;
    assert_eq!(
        unsafe { processor.getBusArrangement(input(), 0, &mut active_input) },
        kResultOk
    );
    assert_eq!(active_input, SpeakerArr::kEmpty);

    assert_eq!(
        unsafe { processor.setBusArrangements(&mut mono_input, 1, &mut stereo_output, 1) },
        kResultTrue
    );
    assert_eq!(
        unsafe { processor.getBusArrangement(input(), 0, &mut active_input) },
        kResultOk
    );
    assert_eq!(active_input, SpeakerArr::kMono);

    assert_eq!(
        unsafe { processor.setBusArrangements(&mut stereo_input, 1, &mut stereo_output, 1) },
        kResultTrue
    );
    assert_eq!(
        unsafe { processor.getBusArrangement(input(), 0, &mut active_input) },
        kResultOk
    );
    assert_eq!(active_input, SpeakerArr::kStereo);

    assert_eq!(
        unsafe { processor.setBusArrangements(&mut surround_input, 1, &mut stereo_output, 1) },
        kResultFalse
    );
    assert_eq!(
        unsafe { processor.setBusArrangements(&mut stereo_input, 1, &mut mono_output, 1) },
        kResultFalse
    );
    assert_eq!(
        unsafe { processor.setBusArrangements(ptr::null_mut(), 1, &mut stereo_output, 1) },
        kResultFalse
    );
}

#[test]
fn processor_projects_sidechain_audio_input_through_process_context() {
    let processor = ResonatorVst3Processor::new();
    setup_processor(&processor, 48_000.0, 4);
    let sidechain_left = [0.5_f32, f32::NAN, -0.5, 0.25];
    let sidechain_right = [0.0_f32, 0.5, 0.25, 0.75];
    let mut input_channels = [
        sidechain_left.as_ptr() as *mut Sample32,
        sidechain_right.as_ptr() as *mut Sample32,
    ];
    let mut input_bus = audio_bus(2, input_channels.as_mut_ptr());
    let mut left = [1.0_f32; 4];
    let mut right = [-1.0_f32; 4];
    let mut output_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
    let mut output_bus = audio_bus(2, output_channels.as_mut_ptr());
    let mut data = process_data(4, Some(&mut input_bus), Some(&mut output_bus));

    assert_eq!(unsafe { processor.process(&mut data) }, kResultOk);

    assert_eq!(
        processor.synth.borrow().sidechain_input_for_test(),
        &[0.25, 0.0, -0.125, 0.5]
    );
    assert_eq!(left, [0.0; 4]);
    assert_eq!(right, [0.0; 4]);
}

#[test]
fn vst_poly_pressure_maps_to_internal_poly_pressure() {
    let mut event = unsafe { std::mem::zeroed::<Event>() };
    event.r#type = Event_::EventTypes_::kPolyPressureEvent as u16;
    event.__field0.polyPressure.channel = 2;
    event.__field0.polyPressure.pitch = 64;
    event.__field0.polyPressure.pressure = 0.75;

    let mapped = unsafe {
        vst_event_to_midi(
            event,
            MidiEventNormalizer::new(
                RESONATOR_MIDI_CONTROLLER_ROUTES,
                DEFAULT_PITCH_BEND_RANGE_SEMITONES,
            ),
        )
    };

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
fn vst_legacy_midi_uses_shared_normalizer_routes_and_pitch_range() {
    let normalizer = MidiEventNormalizer::new(RESONATOR_MIDI_CONTROLLER_ROUTES, 12.0);

    let mut mod_wheel = unsafe { std::mem::zeroed::<Event>() };
    mod_wheel.r#type = Event_::EventTypes_::kLegacyMIDICCOutEvent as u16;
    mod_wheel.__field0.midiCCOut.channel = 3;
    mod_wheel.__field0.midiCCOut.controlNumber = ControllerNumbers_::kCtrlModWheel as u8;
    mod_wheel.__field0.midiCCOut.value = 127;

    assert_eq!(
        unsafe { vst_event_to_midi(mod_wheel, normalizer) },
        Some(MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 3,
            controller: 1,
            value: 1.0,
        }))
    );

    let mut pitch_bend = unsafe { std::mem::zeroed::<Event>() };
    pitch_bend.r#type = Event_::EventTypes_::kLegacyMIDICCOutEvent as u16;
    pitch_bend.__field0.midiCCOut.channel = 3;
    pitch_bend.__field0.midiCCOut.controlNumber = ControllerNumbers_::kPitchBend as u8;
    pitch_bend.__field0.midiCCOut.value = 0;
    pitch_bend.__field0.midiCCOut.value2 = 96;

    assert_eq!(
        unsafe { vst_event_to_midi(pitch_bend, normalizer) },
        Some(MidiEvent::Control(ControlEvent::PitchBend {
            channel: 3,
            semitones: 6.0,
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
    assert_eq!(format_parameter_plain_value(100, 2.0), "MIDI + Audio");
    assert_eq!(format_parameter_plain_value(101, 1.0), "On");
    assert_eq!(format_parameter_plain_value(120, 3.0), "Cont + Latch");
    assert_eq!(format_parameter_plain_value(81, 2.0), "Velocity");
    assert_eq!(format_parameter_plain_value(81, 3.0), "Pressure");
    assert_eq!(format_parameter_plain_value(81, 4.0), "Mod Wheel");
    assert_eq!(format_parameter_plain_value(81, 5.0), "Brightness");
    assert_eq!(format_parameter_plain_value(82, 0.0), "Filter Cutoff");
    assert_eq!(format_parameter_plain_value(82, 4.0), "Res B Position");
    assert_eq!(format_parameter_plain_value(86, 6.0), "LFO Rate");
}

include!("tests/message_tests.rs");

include!("tests/controller_tests.rs");

fn assert_parameter_value(values: &[f64; VST3_PARAMETER_COUNT], id: u32, plain: f32) {
    let index = parameter_index(id).unwrap();
    let expected = normalized_parameter_value(id, plain);
    assert!(
        (values[index] - expected).abs() < 1.0e-6,
        "parameter {id} was {}, expected {expected}",
        values[index]
    );
}

fn wide_string(buffer: &[TChar]) -> String {
    let len = unsafe { len_wstring(buffer.as_ptr()) };
    let chars = buffer[..len].to_vec();
    String::from_utf16(&chars).unwrap()
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

fn setup_processor(processor: &ResonatorVst3Processor, sample_rate: f64, block_size: i32) {
    let mut setup = ProcessSetup {
        processMode: ProcessModes_::kRealtime as i32,
        symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
        maxSamplesPerBlock: block_size,
        sampleRate: sample_rate,
    };
    assert_eq!(unsafe { processor.setupProcessing(&mut setup) }, kResultOk);
}

fn process_data(
    sample_count: i32,
    input_bus: Option<&mut AudioBusBuffers>,
    output_bus: Option<&mut AudioBusBuffers>,
) -> ProcessData {
    let (num_inputs, inputs) = input_bus
        .map(|bus| (1, bus as *mut AudioBusBuffers))
        .unwrap_or((0, ptr::null_mut()));
    let (num_outputs, outputs) = output_bus
        .map(|bus| (1, bus as *mut AudioBusBuffers))
        .unwrap_or((0, ptr::null_mut()));
    ProcessData {
        processMode: ProcessModes_::kRealtime as i32,
        symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
        numSamples: sample_count,
        numInputs: num_inputs,
        numOutputs: num_outputs,
        inputs,
        outputs,
        inputParameterChanges: ptr::null_mut(),
        outputParameterChanges: ptr::null_mut(),
        inputEvents: ptr::null_mut(),
        outputEvents: ptr::null_mut(),
        processContext: ptr::null_mut(),
    }
}

fn audio_bus(num_channels: i32, channel_buffers: *mut *mut Sample32) -> AudioBusBuffers {
    AudioBusBuffers {
        numChannels: num_channels,
        silenceFlags: 0,
        __field0: AudioBusBuffers__type0 {
            channelBuffers32: channel_buffers,
        },
    }
}
