use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr,
};

use lindelion_plugin_shell::vst3::{
    Vst3ParameterInfo, Vst3ParameterMirror, Vst3PeerConnection, fill_vst3_parameter_info,
    notify_vst3_patch_update, parse_vst3_plain_value_string, restart_vst3_parameter_values_changed,
    write_vst3_parameter_string,
};
use lindelion_sample_library::{
    FileSampleLibrary, LibraryPaths, SampleLibrary, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use vst3::{Class, Steinberg::Vst::*, Steinberg::*, uid};

use crate::parameters::PARAMETER_REGISTRY;
use crate::{
    ResonatorSynthPatch, ResonatorTelemetry,
    normalized_parameter_value as registry_normalized_parameter_value, parameter_binding_by_index,
    parameter_binding_index, parameter_info, patch_io,
};

use super::{
    DEFAULT_LIBRARY_DIR, DEFAULT_PITCH_BEND_RANGE_SEMITONES, PITCH_BEND_PARAMETER_ID,
    PITCH_BEND_PARAMETER_INDEX, ResonatorPluginMessage, VST3_PARAMETER_COUNT, editor,
    read_plugin_state_from_stream,
};

include!("controller/core.rs");
include!("controller/editor_summary.rs");
include!("controller/interfaces.rs");
include!("controller/parameters.rs");
