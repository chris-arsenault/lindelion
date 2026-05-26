pub mod analysis;
pub mod analysis_job;
pub mod parameters;
pub mod patch;
mod patch_detection;
pub mod patch_io;
mod plugin;
mod runtime;
pub mod tuning;
mod vst3_entry;
pub mod worker;

pub use analysis::{LinnodSourceAnalyzer, SlicePitchSummary, SourceAnalysis, SourceAnalysisError};
pub use analysis_job::{
    SourceAnalysisCache, SourceAnalysisJob, SourceAnalysisJobResult, SourceAnalysisSequence,
    SourceAnalysisStatus, SourceLoadError, SourceLoadRequest,
};
pub use parameters::PARAMETERS;
pub use patch::{
    ChokeGroupId, DetectionEdit, EnvelopeConfig, LinnodPatch, OutputConfig, PadAssignment, PadEdit,
    PadId, PitchOffset, PlaybackConfig, PlaybackEdit, PlaybackMode, SliceEdit, SliceParams,
    TriggerMode, TuningConfig, default_pad_assignments, normalize_pad_assignments,
    pad_assignment_for_note, slice_index_for_pad,
};
pub use plugin::{DESCRIPTOR, Linnod};
pub use tuning::{
    SliceTuneTarget, SliceTuningInfo, slice_tuning_info, snap_all_slices_to_scale,
    snap_slice_to_scale, tune_all_slices_to_nearest_notes, tune_slice_to_nearest_note,
};
pub use worker::{LinnodWorker, LinnodWorkerQueue, LinnodWorkerResult, SourceAnalysisJobRunner};

pub use lindelion_plugin_metadata::LINNOD_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
