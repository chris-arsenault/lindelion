mod analysis;
mod analysis_job;
mod audition;
#[cfg(test)]
mod detection_quality_tests;
mod midi_export;
mod parameters;
mod patch;
mod patch_io;
mod plugin;
mod sample_library;
mod vst3_entry;
mod worker;

pub use analysis::{AnalysisError, AnalysisResult};
pub use analysis_job::{
    AnalysisJob, AnalysisJobResult, AnalysisResultCache, AnalysisSequence, AnalysisStatus,
    RequantizeJob, run_analysis_job,
};
pub use audition::{
    AUDITION_ATTACK_MS, AUDITION_RELEASE_MS, AUDITION_VOLUME_EPSILON, AUDITION_VOLUME_SMOOTH_MS,
    DEFAULT_AUDITION_VOLUME,
};
pub use lindelion_capture::{
    CaptureEngine, CaptureEvent, CaptureSettings, CaptureState, DEFAULT_BEATS_PER_BAR,
    DEFAULT_CAPTURE_BARS, DEFAULT_CAPTURE_BPM, DOWNBEAT_EPSILON_BEATS, MAX_CAPTURE_BARS,
    MAX_COUNT_IN_BARS, MIN_CAPTURE_BPM, ScratchpadAudio, ScratchpadMetadata, SyncMode,
};
pub use lindelion_onset_detect::OnsetDetectionProfile;
pub use lindelion_phrase_analysis::NoteSegmentationConfig;
pub use parameters::{
    AUDITION_VOLUME_PARAMETER_ID, CAPTURE_BARS_PARAMETER_ID, CONFIDENCE_PARAMETER_ID,
    COUNT_IN_PARAMETER_ID, GRID_PARAMETER_ID, MIN_NOTE_PARAMETER_ID,
    ONSET_SENSITIVITY_PARAMETER_ID, PARAMETERS, ROOT_PARAMETER_ID, SCALE_PARAMETER_ID,
    SNAP_PARAMETER_ID, SYNC_MODE_PARAMETER_ID, TIMING_STRENGTH_PARAMETER_ID,
    VELOCITY_AMOUNT_PARAMETER_ID,
};
pub(crate) use parameters::{
    PARAMETER_BINDING_COUNT, ParameterApplyKind, apply_parameter_normalized,
    denormalized_parameter_value, editor_parameter_bindings, format_parameter_plain_value,
    normalized_parameter_value, parameter_binding, parameter_binding_by_index,
    parameter_binding_index, parameter_info,
};
pub use patch::{
    AnalysisSettings, AuditionSettings, DEFAULT_ANALYSIS_CONFIDENCE_THRESHOLD,
    DEFAULT_ANALYSIS_NOTE_MS, DEFAULT_ONSET_SENSITIVITY, GlirdirPatch,
    MAX_ANALYSIS_CONFIDENCE_THRESHOLD, MAX_ANALYSIS_NOTE_MS, MAX_ONSET_SENSITIVITY,
    MIN_ANALYSIS_CONFIDENCE_THRESHOLD, MIN_ANALYSIS_NOTE_MS, MIN_ONSET_SENSITIVITY,
};
pub use patch_io::{
    FORMAT_VERSION, PLUGIN_STATE_FORMAT_VERSION, PatchIoError, from_plugin_state, from_toml_str,
    load_library_patch, load_patch, save_library_patch, save_patch, to_plugin_state,
    to_toml_string,
};
pub use plugin::{DESCRIPTOR, Glirdir};
pub(crate) use worker::{GlirdirWorker, GlirdirWorkerQueue, GlirdirWorkerResult};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

pub use lindelion_plugin_metadata::GLIRDIR_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
