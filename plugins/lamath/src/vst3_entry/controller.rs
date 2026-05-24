use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr, slice,
};

use lindelion_plugin_shell::vst3::{Vst3PeerConnection, copy_wstring, len_wstring};
use lindelion_sample_library::{
    FileSampleLibrary, LibraryPaths, SampleLibrary, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    PARAMETER_BINDING_COUNT, ResonatorSynthPatch, ResonatorTelemetry,
    normalized_parameter_value as registry_normalized_parameter_value, parameter_binding_by_index,
    parameter_binding_index, parameter_default_normalized_value_by_index, parameter_info, patch_io,
    patch_parameter_normalized_value,
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
