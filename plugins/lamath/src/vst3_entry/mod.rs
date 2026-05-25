#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod controller;
mod editor;
mod factory;
mod messages;
mod midi;
mod processor;

#[cfg(test)]
mod tests;

use crate::{DEFAULT_PITCH_BEND_RANGE_SEMITONES, PARAMETER_BINDING_COUNT};

const MAX_BLOCK_EVENTS: usize = 128;
const SUBCATEGORY: &str = crate::VST3_BUNDLE_METADATA.vst3_sub_categories;
const PITCH_BEND_PARAMETER_ID: u32 = 10_000;
const PITCH_BEND_PARAMETER_INDEX: usize = PARAMETER_BINDING_COUNT;
const VST3_PARAMETER_COUNT: usize = PARAMETER_BINDING_COUNT + 1;
const DEFAULT_LIBRARY_DIR: &str = "Ahara";

#[cfg(any(test, target_os = "macos"))]
use controller::{EditorPatchSummary, parameter_index};
#[cfg(target_os = "macos")]
use controller::{
    EditorSampleSummary, EditorSlotSummary, EditorTelemetry, EditorWaveformPoint,
    default_library_paths,
};
use controller::{ResonatorVst3Controller, encode_telemetry};
#[cfg(test)]
use controller::{
    decode_telemetry, format_parameter_plain_value, normalized_parameter_value,
    parameter_values_from_patch, pitch_bend_normalized_from_plain,
    pitch_bend_plain_from_normalized,
};
use lindelion_plugin_shell::vst3::{
    read_plugin_state_from_stream, vst_event_to_midi, write_plugin_state_to_stream,
};
#[cfg(test)]
use messages::ResonatorMessageKind;
use messages::ResonatorPluginMessage;
use midi::{RESONATOR_MIDI_CONTROLLER_ROUTES, empty_midi_event};
use processor::ResonatorVst3Processor;
