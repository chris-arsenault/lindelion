use std::path::{Path, PathBuf};

use crate::editor_surface::{
    CompleteSurfaceHost, EditorControlKind, EditorParameterBinding, EditorSurfaceHostError,
    EditorSurfaceSlot,
};

pub const RESONATOR_EDITOR_WIDTH: i32 = 1240;
pub const RESONATOR_EDITOR_HEIGHT: i32 = 760;
pub const RESONATOR_EDITOR_PARAMETER_BINDING_COUNT: usize = ResonatorEditorSurfaceSlot::ALL.len();

pub type ResonatorEditorControlKind = EditorControlKind;
pub type ResonatorEditorParameterBinding = EditorParameterBinding<ResonatorEditorSurfaceSlot>;
pub type ResonatorEditorHostError = EditorSurfaceHostError<ResonatorEditorSurfaceSlot>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResonatorEditorSurfaceSlot {
    Master,
    Cutoff,
    Saturation,
    Pan,
    Resonance,
    FilterMode,
    Routing,
    ResonatorMix,
    RetriggerResonators,
    ResonatorAModel,
    ResonatorAPreset,
    ResonatorABrightness,
    ResonatorADecay,
    ResonatorALoopFilter,
    ResonatorALoopGain,
    ResonatorANonlinearity,
    ResonatorAWaveguideStyle,
    ResonatorABoundaryReflection,
    ResonatorBModel,
    ResonatorBPreset,
    ResonatorBBrightness,
    ResonatorBDecay,
    ResonatorBLoopFilter,
    ResonatorBLoopGain,
    ResonatorBNonlinearity,
    ResonatorBWaveguideStyle,
    ResonatorBBoundaryReflection,
    AmpAttack,
    AmpRelease,
    LfoRate,
    LfoShape,
    Mod1Enabled,
    Mod1Source,
    Mod1Destination,
    Mod1Amount,
    AudioInputMode,
    AudioExpressionEnable,
    AudioExpressionPitchRange,
    AudioExpressionPressureFloor,
    AudioExpressionPressureCeiling,
    AudioExpressionBrightnessFloor,
    AudioExpressionBrightnessCeiling,
    AudioNoteOnsetSensitivity,
    AudioNoteReleaseFloor,
    AudioNoteMinimumLength,
    AudioNotePitchConfidence,
    AudioNoteVelocityAmount,
    LiveExcitationMode,
    LiveExcitationGain,
    LiveExcitationLatchWindow,
    LiveExcitationLatchPreRoll,
    LiveExcitationLatchFade,
}

impl ResonatorEditorSurfaceSlot {
    pub const ALL: [Self; 52] = [
        Self::Master,
        Self::Cutoff,
        Self::Saturation,
        Self::Pan,
        Self::Resonance,
        Self::FilterMode,
        Self::Routing,
        Self::ResonatorMix,
        Self::RetriggerResonators,
        Self::ResonatorAModel,
        Self::ResonatorAPreset,
        Self::ResonatorABrightness,
        Self::ResonatorADecay,
        Self::ResonatorALoopFilter,
        Self::ResonatorALoopGain,
        Self::ResonatorANonlinearity,
        Self::ResonatorAWaveguideStyle,
        Self::ResonatorABoundaryReflection,
        Self::ResonatorBModel,
        Self::ResonatorBPreset,
        Self::ResonatorBBrightness,
        Self::ResonatorBDecay,
        Self::ResonatorBLoopFilter,
        Self::ResonatorBLoopGain,
        Self::ResonatorBNonlinearity,
        Self::ResonatorBWaveguideStyle,
        Self::ResonatorBBoundaryReflection,
        Self::AmpAttack,
        Self::AmpRelease,
        Self::LfoRate,
        Self::LfoShape,
        Self::Mod1Enabled,
        Self::Mod1Source,
        Self::Mod1Destination,
        Self::Mod1Amount,
        Self::AudioInputMode,
        Self::AudioExpressionEnable,
        Self::AudioExpressionPitchRange,
        Self::AudioExpressionPressureFloor,
        Self::AudioExpressionPressureCeiling,
        Self::AudioExpressionBrightnessFloor,
        Self::AudioExpressionBrightnessCeiling,
        Self::AudioNoteOnsetSensitivity,
        Self::AudioNoteReleaseFloor,
        Self::AudioNoteMinimumLength,
        Self::AudioNotePitchConfidence,
        Self::AudioNoteVelocityAmount,
        Self::LiveExcitationMode,
        Self::LiveExcitationGain,
        Self::LiveExcitationLatchWindow,
        Self::LiveExcitationLatchPreRoll,
        Self::LiveExcitationLatchFade,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Master => 0,
            Self::Cutoff => 1,
            Self::Saturation => 2,
            Self::Pan => 3,
            Self::Resonance => 4,
            Self::FilterMode => 5,
            Self::Routing => 6,
            Self::ResonatorMix => 7,
            Self::RetriggerResonators => 8,
            Self::ResonatorAModel => 9,
            Self::ResonatorAPreset => 10,
            Self::ResonatorABrightness => 11,
            Self::ResonatorADecay => 12,
            Self::ResonatorALoopFilter => 13,
            Self::ResonatorALoopGain => 14,
            Self::ResonatorANonlinearity => 15,
            Self::ResonatorAWaveguideStyle => 16,
            Self::ResonatorABoundaryReflection => 17,
            Self::ResonatorBModel => 18,
            Self::ResonatorBPreset => 19,
            Self::ResonatorBBrightness => 20,
            Self::ResonatorBDecay => 21,
            Self::ResonatorBLoopFilter => 22,
            Self::ResonatorBLoopGain => 23,
            Self::ResonatorBNonlinearity => 24,
            Self::ResonatorBWaveguideStyle => 25,
            Self::ResonatorBBoundaryReflection => 26,
            Self::AmpAttack => 27,
            Self::AmpRelease => 28,
            Self::LfoRate => 29,
            Self::LfoShape => 30,
            Self::Mod1Enabled => 31,
            Self::Mod1Source => 32,
            Self::Mod1Destination => 33,
            Self::Mod1Amount => 34,
            Self::AudioInputMode => 35,
            Self::AudioExpressionEnable => 36,
            Self::AudioExpressionPitchRange => 37,
            Self::AudioExpressionPressureFloor => 38,
            Self::AudioExpressionPressureCeiling => 39,
            Self::AudioExpressionBrightnessFloor => 40,
            Self::AudioExpressionBrightnessCeiling => 41,
            Self::AudioNoteOnsetSensitivity => 42,
            Self::AudioNoteReleaseFloor => 43,
            Self::AudioNoteMinimumLength => 44,
            Self::AudioNotePitchConfidence => 45,
            Self::AudioNoteVelocityAmount => 46,
            Self::LiveExcitationMode => 47,
            Self::LiveExcitationGain => 48,
            Self::LiveExcitationLatchWindow => 49,
            Self::LiveExcitationLatchPreRoll => 50,
            Self::LiveExcitationLatchFade => 51,
        }
    }
}

impl EditorSurfaceSlot for ResonatorEditorSurfaceSlot {
    const ALL: &'static [Self] = &ResonatorEditorSurfaceSlot::ALL;

    fn index(self) -> usize {
        ResonatorEditorSurfaceSlot::index(self)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResonatorEditorTelemetry {
    pub left_peak: f32,
    pub right_peak: f32,
    pub left_rms: f32,
    pub right_rms: f32,
    pub active_voices: f32,
    pub sidechain_required: bool,
    pub sidechain_input_detected: bool,
    pub sidechain_signal_active: bool,
    pub audio_note_detected: bool,
    pub audio_note_pitch_confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonatorEditorPatchSummary {
    pub patch_name: String,
    pub slots: [ResonatorEditorSlotSummary; 4],
    pub library_samples: Vec<ResonatorEditorSampleSummary>,
}

impl Default for ResonatorEditorPatchSummary {
    fn default() -> Self {
        Self {
            patch_name: "Default".to_string(),
            slots: std::array::from_fn(ResonatorEditorSlotSummary::empty),
            library_samples: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonatorEditorSampleSummary {
    pub label: String,
    pub detail: String,
    pub preview: Vec<ResonatorEditorWaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResonatorEditorWaveformPoint {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResonatorEditorSlotSummary {
    pub label: String,
    pub detail: String,
    pub sample_backed: bool,
    pub pitch_track: bool,
    pub looping: bool,
}

impl ResonatorEditorSlotSummary {
    pub fn empty(index: usize) -> Self {
        Self {
            label: format!("Layer {}", index + 1),
            detail: "Empty layer".to_string(),
            sample_backed: false,
            pitch_track: false,
            looping: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonatorEditorDirectories {
    pub patch_directory: PathBuf,
    pub sample_directory: PathBuf,
    pub export_directory: PathBuf,
}

impl Default for ResonatorEditorDirectories {
    fn default() -> Self {
        Self {
            patch_directory: PathBuf::from("."),
            sample_directory: PathBuf::from("."),
            export_directory: PathBuf::from("."),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorCallbacks {
    pub refresh_library: unsafe fn(usize),
    pub parameter_value: unsafe fn(usize, u32) -> f32,
    pub set_parameter: unsafe fn(usize, u32, f64),
    pub parameter_value_text: unsafe fn(usize, u32, f64) -> String,
    pub default_normalized: unsafe fn(usize, u32) -> f32,
    pub summary: unsafe fn(usize) -> ResonatorEditorPatchSummary,
    pub telemetry: unsafe fn(usize) -> ResonatorEditorTelemetry,
    pub directories: unsafe fn(usize) -> ResonatorEditorDirectories,
    pub request_telemetry: unsafe fn(usize),
    pub handle_command: for<'a> unsafe fn(usize, ResonatorEditorCommandRequest<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorCommandRequest<'a> {
    pub command: crate::UiCommand,
    pub patch_save_path: Option<&'a Path>,
    pub patch_load_path: Option<&'a Path>,
    pub patch_export_directory: Option<&'a Path>,
    pub sample_path: Option<&'a Path>,
    pub selected_library_sample: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorHost {
    context: usize,
    surface: CompleteSurfaceHost<
        ResonatorEditorSurfaceSlot,
        ResonatorEditorParameterBinding,
        RESONATOR_EDITOR_PARAMETER_BINDING_COUNT,
    >,
    callbacks: ResonatorEditorCallbacks,
}

impl ResonatorEditorHost {
    pub fn new(
        context: usize,
        bindings: impl IntoIterator<Item = ResonatorEditorParameterBinding>,
        callbacks: ResonatorEditorCallbacks,
    ) -> Result<Self, ResonatorEditorHostError> {
        Ok(Self {
            context,
            surface: CompleteSurfaceHost::new(bindings)?,
            callbacks,
        })
    }

    pub const fn parameter_bindings(
        self,
    ) -> [Option<ResonatorEditorParameterBinding>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT] {
        self.surface.parameter_bindings()
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn refresh_library(self) {
        unsafe { (self.callbacks.refresh_library)(self.context) }
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
    pub unsafe fn summary(self) -> ResonatorEditorPatchSummary {
        unsafe { (self.callbacks.summary)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn telemetry(self) -> ResonatorEditorTelemetry {
        unsafe { (self.callbacks.telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn directories(self) -> ResonatorEditorDirectories {
        unsafe { (self.callbacks.directories)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_telemetry(self) {
        unsafe { (self.callbacks.request_telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn handle_command(self, request: ResonatorEditorCommandRequest<'_>) {
        unsafe { (self.callbacks.handle_command)(self.context, request) }
    }
}

#[cfg(target_os = "macos")]
pub use platform::{ResonatorEditorSize, ResonatorViziaEditor, build_resonator_application};

#[cfg(target_os = "macos")]
#[cfg(target_os = "macos")]
#[path = "resonator_vizia/platform.rs"]
mod platform;

#[cfg(test)]
#[path = "resonator_vizia_tests.rs"]
mod tests;
