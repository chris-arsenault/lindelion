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
pub enum LinnodEditorPlaybackMode {
    #[default]
    OneShot,
    Gated,
    Looped,
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
    pub cents_deviation: Option<f32>,
    pub gain_db: f32,
    pub pan: f32,
    pub pitch_semitones: i32,
    pub pitch_cents: f32,
    pub reverse: bool,
    pub playback_mode: LinnodEditorPlaybackMode,
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
            cents_deviation: None,
            gain_db: 0.0,
            pan: 0.0,
            pitch_semitones: 0,
            pitch_cents: 0.0,
            reverse: false,
            playback_mode: LinnodEditorPlaybackMode::OneShot,
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
    pub detection: LinnodEditorDetectionConfig,
    pub trigger_mode: LinnodEditorTriggerMode,
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
            detection: LinnodEditorDetectionConfig::default(),
            trigger_mode: LinnodEditorTriggerMode::Pad,
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
    PlaybackMode {
        slice_index: usize,
        mode: LinnodEditorPlaybackMode,
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

#[derive(Debug, Clone, Copy)]
pub struct LinnodEditorCallbacks {
    pub parameter_value: unsafe fn(usize, u32) -> f32,
    pub set_parameter: unsafe fn(usize, u32, f64),
    pub parameter_value_text: unsafe fn(usize, u32, f64) -> String,
    pub default_normalized: unsafe fn(usize, u32) -> f32,
    pub status: unsafe fn(usize) -> LinnodEditorStatus,
    pub telemetry: unsafe fn(usize) -> LinnodEditorTelemetry,
    pub summary: unsafe fn(usize) -> LinnodEditorPatchSummary,
    pub directories: unsafe fn(usize) -> LinnodEditorDirectories,
    pub request_status: unsafe fn(usize),
    pub request_telemetry: unsafe fn(usize),
    pub handle_command: for<'a> unsafe fn(usize, LinnodEditorCommandRequest<'a>),
    pub edit_marker: unsafe fn(usize, LinnodEditorMarkerEdit),
    pub edit_pad: unsafe fn(usize, LinnodEditorPadEdit),
    pub edit_detection: unsafe fn(usize, LinnodEditorDetectionEdit),
    pub edit_slice: unsafe fn(usize, LinnodEditorSliceEdit),
}

#[derive(Debug, Clone, Copy)]
pub struct LinnodEditorHost {
    context: usize,
    surface: CompleteSurfaceHost<
        LinnodEditorSurfaceSlot,
        LinnodEditorParameterBinding,
        LINNOD_EDITOR_PARAMETER_BINDING_COUNT,
    >,
    callbacks: LinnodEditorCallbacks,
}

impl LinnodEditorHost {
    pub fn new(
        context: usize,
        bindings: impl IntoIterator<Item = LinnodEditorParameterBinding>,
        callbacks: LinnodEditorCallbacks,
    ) -> Result<Self, LinnodEditorHostError> {
        Ok(Self {
            context,
            surface: CompleteSurfaceHost::new(bindings)?,
            callbacks,
        })
    }

    pub const fn parameter_bindings(
        self,
    ) -> [Option<LinnodEditorParameterBinding>; LINNOD_EDITOR_PARAMETER_BINDING_COUNT] {
        self.surface.parameter_bindings()
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn parameter_value(self, id: u32) -> f32 {
        unsafe { (self.callbacks.parameter_value)(self.context, id) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn set_parameter(self, id: u32, normalized: f64) {
        unsafe { (self.callbacks.set_parameter)(self.context, id, normalized) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn parameter_value_text(self, id: u32, normalized: f64) -> String {
        unsafe { (self.callbacks.parameter_value_text)(self.context, id, normalized) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn default_normalized(self, id: u32) -> f32 {
        unsafe { (self.callbacks.default_normalized)(self.context, id) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn status(self) -> LinnodEditorStatus {
        unsafe { (self.callbacks.status)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn telemetry(self) -> LinnodEditorTelemetry {
        unsafe { (self.callbacks.telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn summary(self) -> LinnodEditorPatchSummary {
        unsafe { (self.callbacks.summary)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn directories(self) -> LinnodEditorDirectories {
        unsafe { (self.callbacks.directories)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_status(self) {
        unsafe { (self.callbacks.request_status)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_telemetry(self) {
        unsafe { (self.callbacks.request_telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn handle_command(self, request: LinnodEditorCommandRequest<'_>) {
        unsafe { (self.callbacks.handle_command)(self.context, request) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_marker(self, edit: LinnodEditorMarkerEdit) {
        unsafe { (self.callbacks.edit_marker)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_pad(self, edit: LinnodEditorPadEdit) {
        unsafe { (self.callbacks.edit_pad)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_detection(self, edit: LinnodEditorDetectionEdit) {
        unsafe { (self.callbacks.edit_detection)(self.context, edit) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn edit_slice(self, edit: LinnodEditorSliceEdit) {
        unsafe { (self.callbacks.edit_slice)(self.context, edit) }
    }
}

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
