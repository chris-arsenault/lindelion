use super::*;
use crate::test_support::{audio_note_detection_patch, sidechain_sine_note};
use lindelion_dsp_utils::{
    analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
    db_to_gain,
    math::midi_note_to_hz,
    params::StructuralChangePolicy,
};
use lindelion_plugin_shell::{
    AudioBuffer, AudioInputBuffer, AudioPlugin, ControlEvent, MidiEvent, NoteEvent, ParameterId,
    ProcessContext, ProcessMode, ProcessSetup,
};
use lindelion_sample_library::{SampleLibrary, SampleReference, SampleResolution};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

include!("plugin_tests/render_tests.rs");
include!("plugin_tests/parameter_tests.rs");
include!("plugin_tests/runtime_tests.rs");
include!("plugin_tests/render_helpers.rs");
include!("plugin_tests/expression_helpers.rs");
include!("plugin_tests/sample_helpers.rs");
