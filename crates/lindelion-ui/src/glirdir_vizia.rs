use std::path::PathBuf;

use crate::{
    WaveformPoint,
    editor_surface::{
        CompleteSurfaceHost, EditorControlKind, EditorParameterBinding, EditorSurfaceHostError,
        EditorSurfaceSlot,
    },
};

pub const GLIRDIR_EDITOR_WIDTH: i32 = 960;
pub const GLIRDIR_EDITOR_HEIGHT: i32 = 640;
pub const GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT: usize = GlirdirEditorSurfaceSlot::ALL.len();

pub type GlirdirEditorControlKind = EditorControlKind;
pub type GlirdirEditorParameterBinding = EditorParameterBinding<GlirdirEditorSurfaceSlot>;
pub type GlirdirEditorHostError = EditorSurfaceHostError<GlirdirEditorSurfaceSlot>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlirdirEditorSurfaceSlot {
    CaptureBars,
    SyncMode,
    CountIn,
    Confidence,
    OnsetSensitivity,
    MinNote,
    Root,
    Scale,
    Snap,
    Grid,
    TimingStrength,
    VelocityAmount,
    AuditionVolume,
}

impl GlirdirEditorSurfaceSlot {
    pub const ALL: [Self; 13] = [
        Self::CaptureBars,
        Self::SyncMode,
        Self::CountIn,
        Self::Confidence,
        Self::OnsetSensitivity,
        Self::MinNote,
        Self::Root,
        Self::Scale,
        Self::Snap,
        Self::Grid,
        Self::TimingStrength,
        Self::VelocityAmount,
        Self::AuditionVolume,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::CaptureBars => 0,
            Self::SyncMode => 1,
            Self::CountIn => 2,
            Self::Confidence => 3,
            Self::OnsetSensitivity => 4,
            Self::MinNote => 5,
            Self::Root => 6,
            Self::Scale => 7,
            Self::Snap => 8,
            Self::Grid => 9,
            Self::TimingStrength => 10,
            Self::VelocityAmount => 11,
            Self::AuditionVolume => 12,
        }
    }
}

impl EditorSurfaceSlot for GlirdirEditorSurfaceSlot {
    const ALL: &'static [Self] = &GlirdirEditorSurfaceSlot::ALL;

    fn index(self) -> usize {
        GlirdirEditorSurfaceSlot::index(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlirdirEditorCommand {
    ArmCapture,
    ClearScratchpad,
    FinalizeCapture,
    PlayAudition,
    StopAudition,
    ToggleLoop,
    ToggleLiveEdit,
    SaveScratchpadToLibrary,
    ExportMidi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlirdirEditorMidiDrag {
    Ready { path: PathBuf },
    Requested,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlirdirEditorCaptureState {
    Idle,
    Armed,
    CountIn,
    Capturing,
    Captured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlirdirEditorAnalysisStatus {
    Idle,
    Capturing,
    CapturedPendingAnalysis,
    Analyzing,
    Ready,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlirdirEditorStatus {
    pub capture_state: GlirdirEditorCaptureState,
    pub analysis_status: GlirdirEditorAnalysisStatus,
    pub has_scratchpad: bool,
    pub has_analysis: bool,
    pub library_status: GlirdirEditorLibraryStatus,
}

impl Default for GlirdirEditorStatus {
    fn default() -> Self {
        Self {
            capture_state: GlirdirEditorCaptureState::Idle,
            analysis_status: GlirdirEditorAnalysisStatus::Idle,
            has_scratchpad: false,
            has_analysis: false,
            library_status: GlirdirEditorLibraryStatus::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlirdirEditorLibraryStatus {
    Idle,
    Saving,
    Saved,
    EmptyScratchpad,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlirdirEditorWaveformPreview {
    pub sample_rate: u32,
    pub points: Vec<WaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlirdirEditorPianoRollNote {
    pub start_tick: u32,
    pub duration_ticks: u32,
    pub midi_note: u8,
    pub velocity: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlirdirEditorPianoRollPreview {
    pub ppq: u16,
    pub bpm: u16,
    pub notes: Vec<GlirdirEditorPianoRollNote>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlirdirEditorPreview {
    pub waveform: GlirdirEditorWaveformPreview,
    pub piano_roll: GlirdirEditorPianoRollPreview,
}

impl Default for GlirdirEditorPreview {
    fn default() -> Self {
        Self {
            waveform: GlirdirEditorWaveformPreview {
                sample_rate: 48_000,
                points: Vec::new(),
            },
            piano_roll: GlirdirEditorPianoRollPreview {
                ppq: 960,
                bpm: 120,
                notes: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlirdirEditorSize {
    pub width: i32,
    pub height: i32,
}

impl Default for GlirdirEditorSize {
    fn default() -> Self {
        Self {
            width: GLIRDIR_EDITOR_WIDTH,
            height: GLIRDIR_EDITOR_HEIGHT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlirdirEditorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl GlirdirEditorRect {
    pub const fn right(self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.height
    }

    pub const fn fits(self, size: GlirdirEditorSize) -> bool {
        self.x >= 0 && self.y >= 0 && self.right() <= size.width && self.bottom() <= size.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlirdirEditorLayout {
    pub size: GlirdirEditorSize,
    pub top_bar: GlirdirEditorRect,
    pub capture_panel: GlirdirEditorRect,
    pub preview_panel: GlirdirEditorRect,
    pub quantize_panel: GlirdirEditorRect,
    pub audition_panel: GlirdirEditorRect,
    pub velocity_panel: GlirdirEditorRect,
    pub detection_panel: GlirdirEditorRect,
}

impl GlirdirEditorLayout {
    pub const fn target() -> Self {
        Self {
            size: GlirdirEditorSize {
                width: GLIRDIR_EDITOR_WIDTH,
                height: GLIRDIR_EDITOR_HEIGHT,
            },
            top_bar: GlirdirEditorRect {
                x: 14,
                y: 14,
                width: 932,
                height: 54,
            },
            capture_panel: GlirdirEditorRect {
                x: 14,
                y: 80,
                width: 300,
                height: 142,
            },
            preview_panel: GlirdirEditorRect {
                x: 326,
                y: 80,
                width: 620,
                height: 340,
            },
            quantize_panel: GlirdirEditorRect {
                x: 14,
                y: 234,
                width: 300,
                height: 186,
            },
            audition_panel: GlirdirEditorRect {
                x: 14,
                y: 432,
                width: 300,
                height: 84,
            },
            velocity_panel: GlirdirEditorRect {
                x: 326,
                y: 432,
                width: 300,
                height: 84,
            },
            detection_panel: GlirdirEditorRect {
                x: 638,
                y: 432,
                width: 308,
                height: 84,
            },
        }
    }

    pub const fn panels_fit(self) -> bool {
        self.top_bar.fits(self.size)
            && self.capture_panel.fits(self.size)
            && self.preview_panel.fits(self.size)
            && self.quantize_panel.fits(self.size)
            && self.audition_panel.fits(self.size)
            && self.velocity_panel.fits(self.size)
            && self.detection_panel.fits(self.size)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GlirdirEditorCallbacks {
    pub parameter_value: unsafe fn(usize, u32) -> f32,
    pub set_parameter: unsafe fn(usize, u32, f64),
    pub parameter_value_text: unsafe fn(usize, u32, f64) -> String,
    pub default_normalized: unsafe fn(usize, u32) -> f32,
    pub status: unsafe fn(usize) -> GlirdirEditorStatus,
    pub preview: unsafe fn(usize) -> GlirdirEditorPreview,
    pub request_status: unsafe fn(usize),
    pub handle_command: unsafe fn(usize, GlirdirEditorCommand),
    pub prepare_midi_drag: unsafe fn(usize) -> GlirdirEditorMidiDrag,
}

#[derive(Debug, Clone, Copy)]
pub struct GlirdirEditorHost {
    context: usize,
    surface: CompleteSurfaceHost<
        GlirdirEditorSurfaceSlot,
        GlirdirEditorParameterBinding,
        GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT,
    >,
    callbacks: GlirdirEditorCallbacks,
}

impl GlirdirEditorHost {
    pub fn new(
        context: usize,
        bindings: impl IntoIterator<Item = GlirdirEditorParameterBinding>,
        callbacks: GlirdirEditorCallbacks,
    ) -> Result<Self, GlirdirEditorHostError> {
        Ok(Self {
            context,
            surface: CompleteSurfaceHost::new(bindings)?,
            callbacks,
        })
    }

    pub const fn parameter_bindings(
        self,
    ) -> [Option<GlirdirEditorParameterBinding>; GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT] {
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
    pub unsafe fn status(self) -> GlirdirEditorStatus {
        unsafe { (self.callbacks.status)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn preview(self) -> GlirdirEditorPreview {
        unsafe { (self.callbacks.preview)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_status(self) {
        unsafe { (self.callbacks.request_status)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn handle_command(self, command: GlirdirEditorCommand) {
        unsafe { (self.callbacks.handle_command)(self.context, command) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn prepare_midi_drag(self) -> GlirdirEditorMidiDrag {
        unsafe { (self.callbacks.prepare_midi_drag)(self.context) }
    }
}

#[cfg(target_os = "macos")]
pub use platform::{GlirdirViziaEditor, build_glirdir_application};

#[cfg(target_os = "macos")]
#[path = "glirdir_vizia/platform.rs"]
mod platform;

#[cfg(test)]
#[path = "glirdir_vizia_tests.rs"]
mod tests;
