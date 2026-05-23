use std::path::{Path, PathBuf};

pub const RESONATOR_EDITOR_WIDTH: i32 = 960;
pub const RESONATOR_EDITOR_HEIGHT: i32 = 640;
pub const RESONATOR_EDITOR_PARAMETER_BINDING_COUNT: usize = ResonatorEditorSurfaceSlot::ALL.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResonatorEditorSurfaceSlot {
    Master,
    Cutoff,
    Saturation,
    Pan,
    Resonance,
    FilterMode,
    Routing,
    RetriggerResonators,
    ResonatorAModel,
    ResonatorAPreset,
    ResonatorABrightness,
    ResonatorADecay,
    ResonatorAWaveguideStyle,
    ResonatorABoundaryReflection,
    ResonatorBModel,
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
}

impl ResonatorEditorSurfaceSlot {
    pub const ALL: [Self; 28] = [
        Self::Master,
        Self::Cutoff,
        Self::Saturation,
        Self::Pan,
        Self::Resonance,
        Self::FilterMode,
        Self::Routing,
        Self::RetriggerResonators,
        Self::ResonatorAModel,
        Self::ResonatorAPreset,
        Self::ResonatorABrightness,
        Self::ResonatorADecay,
        Self::ResonatorAWaveguideStyle,
        Self::ResonatorABoundaryReflection,
        Self::ResonatorBModel,
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
            Self::RetriggerResonators => 7,
            Self::ResonatorAModel => 8,
            Self::ResonatorAPreset => 9,
            Self::ResonatorABrightness => 10,
            Self::ResonatorADecay => 11,
            Self::ResonatorAWaveguideStyle => 12,
            Self::ResonatorABoundaryReflection => 13,
            Self::ResonatorBModel => 14,
            Self::ResonatorBLoopFilter => 15,
            Self::ResonatorBLoopGain => 16,
            Self::ResonatorBNonlinearity => 17,
            Self::ResonatorBWaveguideStyle => 18,
            Self::ResonatorBBoundaryReflection => 19,
            Self::AmpAttack => 20,
            Self::AmpRelease => 21,
            Self::LfoRate => 22,
            Self::LfoShape => 23,
            Self::Mod1Enabled => 24,
            Self::Mod1Source => 25,
            Self::Mod1Destination => 26,
            Self::Mod1Amount => 27,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResonatorEditorControlKind {
    Knob,
    Slider,
    Binary {
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResonatorEditorParameterBinding {
    id: u32,
    slot: ResonatorEditorSurfaceSlot,
    label: &'static str,
    control: ResonatorEditorControlKind,
}

impl ResonatorEditorParameterBinding {
    pub const fn new(
        id: u32,
        slot: ResonatorEditorSurfaceSlot,
        label: &'static str,
        control: ResonatorEditorControlKind,
    ) -> Self {
        Self {
            id,
            slot,
            label,
            control,
        }
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub const fn slot(self) -> ResonatorEditorSurfaceSlot {
        self.slot
    }

    pub const fn label(self) -> &'static str {
        self.label
    }

    pub const fn control(self) -> ResonatorEditorControlKind {
        self.control
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResonatorEditorTelemetry {
    pub left_peak: f32,
    pub right_peak: f32,
    pub left_rms: f32,
    pub right_rms: f32,
    pub active_voices: f32,
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
            label: format!("Slot {}", index + 1),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResonatorEditorHostError {
    DuplicateSlot(ResonatorEditorSurfaceSlot),
    MissingSlot(ResonatorEditorSurfaceSlot),
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorHost {
    context: usize,
    parameter_bindings:
        [Option<ResonatorEditorParameterBinding>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT],
    callbacks: ResonatorEditorCallbacks,
}

impl ResonatorEditorHost {
    pub fn new(
        context: usize,
        bindings: impl IntoIterator<Item = ResonatorEditorParameterBinding>,
        callbacks: ResonatorEditorCallbacks,
    ) -> Result<Self, ResonatorEditorHostError> {
        let mut parameter_bindings = [None; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT];
        for binding in bindings {
            let index = binding.slot().index();
            if parameter_bindings[index].is_some() {
                return Err(ResonatorEditorHostError::DuplicateSlot(binding.slot()));
            }
            parameter_bindings[index] = Some(binding);
        }

        for slot in ResonatorEditorSurfaceSlot::ALL {
            if parameter_bindings[slot.index()].is_none() {
                return Err(ResonatorEditorHostError::MissingSlot(slot));
            }
        }

        Ok(Self {
            context,
            parameter_bindings,
            callbacks,
        })
    }

    pub const fn parameter_bindings(
        self,
    ) -> [Option<ResonatorEditorParameterBinding>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT] {
        self.parameter_bindings
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
