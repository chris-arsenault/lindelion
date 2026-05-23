use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr, slice,
};

use lindelion_plugin_shell::vst3::{copy_wstring, len_wstring};
use lindelion_sample_library::{
    FileSampleLibrary, LibraryPaths, SampleLibrary, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    PARAMETERS, ResonatorSynthPatch, ResonatorTelemetry, parameter_binding,
    parameter_binding_by_index, parameter_binding_index, patch_io, patch_parameter_plain_value,
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
