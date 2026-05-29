use std::path::{Path, PathBuf};

use crate::{
    PadId, WaveformPoint,
    editor_surface::{
        CompleteSurfaceHost, EditorControlKind, EditorParameterBinding, EditorSurfaceHostError,
        EditorSurfaceSlot,
    },
};

pub const LINNOD_EDITOR_WIDTH: i32 = 1180;
pub const LINNOD_EDITOR_HEIGHT: i32 = 820;
pub const LINNOD_EDITOR_PARAMETER_BINDING_COUNT: usize = LinnodEditorSurfaceSlot::ALL.len();

pub type LinnodEditorControlKind = EditorControlKind;
pub type LinnodEditorParameterBinding = EditorParameterBinding<LinnodEditorSurfaceSlot>;
pub type LinnodEditorHostError = EditorSurfaceHostError<LinnodEditorSurfaceSlot>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinnodEditorSurfaceSlot {
    MasterGain,
    DetectionSensitivity,
    TuningReference,
}

impl LinnodEditorSurfaceSlot {
    pub const ALL: [Self; 3] = [
        Self::MasterGain,
        Self::DetectionSensitivity,
        Self::TuningReference,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::MasterGain => 0,
            Self::DetectionSensitivity => 1,
            Self::TuningReference => 2,
        }
    }
}

impl EditorSurfaceSlot for LinnodEditorSurfaceSlot {
    const ALL: &'static [Self] = &LinnodEditorSurfaceSlot::ALL;

    fn index(self) -> usize {
        LinnodEditorSurfaceSlot::index(self)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinnodEditorSourceStatus {
    #[default]
    Idle,
    PendingLoad,
    Analyzing,
    Ready,
    MissingSource,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinnodEditorStatus {
    pub source_status: LinnodEditorSourceStatus,
    pub has_source: bool,
    pub has_analysis: bool,
    pub marker_count: usize,
    pub selected_slice_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LinnodEditorTelemetry {
    pub left_peak: f32,
    pub right_peak: f32,
    pub active_voices: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinnodEditorTriggerMode {
    #[default]
    Pad,
    Chromatic,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinnodEditorPitchShiftAlgorithm {
    #[default]
    SpectralPeak,
    Varispeed,
    TimeStretch,
    ResampleStretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinnodEditorPlaybackMode {
    #[default]
    OneShot,
    Gated,
    Looped,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinnodEditorEnvelope {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

impl Default for LinnodEditorEnvelope {
    fn default() -> Self {
        Self {
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 50.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LinnodEditorPlaybackConfig {
    pub mode: LinnodEditorPlaybackMode,
    pub envelope: LinnodEditorEnvelope,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinnodEditorAutoTuneConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinnodEditorMarkerKind {
    Auto,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinnodEditorMarker {
    pub position_samples: usize,
    pub kind: LinnodEditorMarkerKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinnodEditorSliceSummary {
    pub index: usize,
    pub name: String,
    pub start_sample: usize,
    pub end_sample: usize,
    pub start_offset_ms: f32,
    pub end_offset_ms: f32,
    pub detected_f0_hz: Option<f32>,
    pub detected_midi_note: Option<f32>,
    pub nearest_midi_note: Option<u8>,
    pub nearest_scale_midi_note: Option<u8>,
    pub nearest_midi_note_hz: Option<f32>,
    pub nearest_scale_midi_note_hz: Option<f32>,
    pub cents_deviation: Option<f32>,
    pub root_target_f0_hz: Option<f32>,
    pub gain_db: f32,
    pub pan: f32,
    pub pitch_semitones: i32,
    pub pitch_cents: f32,
    pub reverse: bool,
    pub use_playback_override: bool,
    pub playback_mode: LinnodEditorPlaybackMode,
    pub envelope: LinnodEditorEnvelope,
    pub use_auto_tune_override: bool,
    pub auto_tune_enabled: bool,
    pub filter_cutoff_hz: f32,
}

impl LinnodEditorSliceSummary {
    pub fn empty(index: usize) -> Self {
        Self {
            index,
            name: format!("Slice {}", index + 1),
            start_sample: 0,
            end_sample: 0,
            start_offset_ms: 0.0,
            end_offset_ms: 0.0,
            detected_f0_hz: None,
            detected_midi_note: None,
            nearest_midi_note: None,
            nearest_scale_midi_note: None,
            nearest_midi_note_hz: None,
            nearest_scale_midi_note_hz: None,
            cents_deviation: None,
            root_target_f0_hz: None,
            gain_db: 0.0,
            pan: 0.0,
            pitch_semitones: 0,
            pitch_cents: 0.0,
            reverse: false,
            use_playback_override: false,
            playback_mode: LinnodEditorPlaybackMode::OneShot,
            envelope: LinnodEditorEnvelope::default(),
            use_auto_tune_override: false,
            auto_tune_enabled: false,
            filter_cutoff_hz: 20_000.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinnodEditorPadSummary {
    pub pad: PadId,
    pub midi_note: u8,
    pub slice_index: usize,
    pub choke_group: Option<u8>,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinnodEditorDetectionAlgorithm {
    #[default]
    SuperFlux,
    ComplexFlux,
    SpectralSparsity,
    PitchStability,
    EnergyTransient,
    ManualGrid,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinnodEditorDetectionConfig {
    pub algorithm: LinnodEditorDetectionAlgorithm,
    pub min_slice_ms: f32,
    pub lookback_frames: u32,
    pub max_filter_radius: u32,
    pub group_delay_weight: f32,
    pub spectral_window_size: usize,
    pub pitch_stability_threshold_cents: f32,
    pub pitch_stability_duration_ms: f32,
    pub energy_frame_size: usize,
    pub manual_grid_divisions: usize,
    pub manual_grid_offset_ms: f32,
}

impl Default for LinnodEditorDetectionConfig {
    fn default() -> Self {
        Self {
            algorithm: LinnodEditorDetectionAlgorithm::SuperFlux,
            min_slice_ms: 50.0,
            lookback_frames: 3,
            max_filter_radius: 3,
            group_delay_weight: 1.0,
            spectral_window_size: 1024,
            pitch_stability_threshold_cents: 120.0,
            pitch_stability_duration_ms: 64.0,
            energy_frame_size: 512,
            manual_grid_divisions: 16,
            manual_grid_offset_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinnodEditorPatchSummary {
    pub patch_name: String,
    pub source_label: String,
    pub source_sample_rate: u32,
    pub waveform: Vec<WaveformPoint>,
    pub markers: Vec<LinnodEditorMarker>,
    pub pads: Vec<LinnodEditorPadSummary>,
    pub slices: Vec<LinnodEditorSliceSummary>,
    pub playback: LinnodEditorPlaybackConfig,
    pub auto_tune: LinnodEditorAutoTuneConfig,
    pub detection: LinnodEditorDetectionConfig,
    pub trigger_mode: LinnodEditorTriggerMode,
    pub pitch_shift_algorithm: LinnodEditorPitchShiftAlgorithm,
    pub tuning_reference_hz: f32,
    pub tuning_root_label: String,
    pub tuning_scale_label: String,
    pub selected_slice_index: Option<usize>,
}

impl Default for LinnodEditorPatchSummary {
    fn default() -> Self {
        Self {
            patch_name: "Default".to_string(),
            source_label: "No source".to_string(),
            source_sample_rate: 48_000,
            waveform: Vec::new(),
            markers: Vec::new(),
            pads: Vec::new(),
            slices: (0..16).map(LinnodEditorSliceSummary::empty).collect(),
            playback: LinnodEditorPlaybackConfig::default(),
            auto_tune: LinnodEditorAutoTuneConfig::default(),
            detection: LinnodEditorDetectionConfig::default(),
            trigger_mode: LinnodEditorTriggerMode::Pad,
            pitch_shift_algorithm: LinnodEditorPitchShiftAlgorithm::SpectralPeak,
            tuning_reference_hz: 440.0,
            tuning_root_label: "A".to_string(),
            tuning_scale_label: "Chromatic".to_string(),
            selected_slice_index: Some(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinnodEditorDirectories {
    pub patch_directory: PathBuf,
    pub sample_directory: PathBuf,
    pub export_directory: PathBuf,
}

impl Default for LinnodEditorDirectories {
    fn default() -> Self {
        Self {
            patch_directory: PathBuf::from("."),
            sample_directory: PathBuf::from("."),
            export_directory: PathBuf::from("."),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinnodEditorCommand {
    SavePatch,
    LoadPatch,
    ExportPatchWithSamples,
    LoadSource,
    RedetectSlices,
    TuneSelectedSlice,
    TuneAllSlices,
    SnapAllSlicesToScale,
    SetTriggerMode(LinnodEditorTriggerMode),
    SetPitchShiftAlgorithm(LinnodEditorPitchShiftAlgorithm),
    SelectPad(PadId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinnodEditorMarkerEdit {
    AddUser { position_samples: usize },
    RemoveAt { position_samples: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinnodEditorPadEdit {
    ChokeGroup { pad: PadId, group: Option<u8> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinnodEditorPlaybackEdit {
    Mode { mode: LinnodEditorPlaybackMode },
    Envelope { envelope: LinnodEditorEnvelope },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinnodEditorAutoTuneEdit {
    Enabled { enabled: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinnodEditorDetectionEdit {
    Algorithm {
        algorithm: LinnodEditorDetectionAlgorithm,
    },
    MinSliceMs {
        min_slice_ms: f32,
    },
    LookbackFrames {
        lookback_frames: u32,
    },
    MaxFilterRadius {
        max_filter_radius: u32,
    },
    GroupDelayWeight {
        group_delay_weight: f32,
    },
    SpectralWindowSize {
        window_size: usize,
    },
    PitchStabilityThresholdCents {
        threshold_cents: f32,
    },
    PitchStabilityDurationMs {
        duration_ms: f32,
    },
    EnergyFrameSize {
        frame_size: usize,
    },
    ManualGridDivisions {
        divisions: usize,
    },
    ManualGridOffsetMs {
        offset_ms: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinnodEditorSliceEdit {
    Select {
        slice_index: usize,
    },
    Name {
        slice_index: usize,
        name: String,
    },
    Offsets {
        slice_index: usize,
        start_offset_ms: f32,
        end_offset_ms: f32,
    },
    Pitch {
        slice_index: usize,
        semitones: i32,
        cents: f32,
    },
    GainDb {
        slice_index: usize,
        gain_db: f32,
    },
    Pan {
        slice_index: usize,
        pan: f32,
    },
    Reverse {
        slice_index: usize,
        reverse: bool,
    },
    PlaybackOverride {
        slice_index: usize,
        enabled: bool,
    },
    PlaybackMode {
        slice_index: usize,
        mode: LinnodEditorPlaybackMode,
    },
    Envelope {
        slice_index: usize,
        envelope: LinnodEditorEnvelope,
    },
    AutoTuneOverride {
        slice_index: usize,
        enabled: bool,
    },
    AutoTuneEnabled {
        slice_index: usize,
        enabled: bool,
    },
    FilterCutoff {
        slice_index: usize,
        cutoff_hz: f32,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct LinnodEditorCommandRequest<'a> {
    pub command: LinnodEditorCommand,
    pub source_path: Option<&'a Path>,
    pub patch_save_path: Option<&'a Path>,
    pub patch_load_path: Option<&'a Path>,
    pub patch_export_directory: Option<&'a Path>,
}

#[derive(Debug, Clone)]
pub struct LinnodEditorSliceEditRequest<'a> {
    pub edit: LinnodEditorSliceEdit,
    pub name: Option<&'a str>,
}

include!("linnod_vizia/host.rs");

#[cfg(target_os = "macos")]
pub use platform::{
    LinnodEditorSize, LinnodViziaEditor, build_linnod_application, paste_source_from_clipboard,
};

#[cfg(target_os = "macos")]
#[path = "linnod_vizia/platform.rs"]
mod platform;

#[cfg(test)]
#[path = "linnod_vizia_tests.rs"]
mod tests;
