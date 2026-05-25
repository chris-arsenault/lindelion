use lindelion_midi::QuantizeSettings;
use lindelion_plugin_shell::{
    ParameterApplyDispatcher, ParameterBinding as RegistryParameterBinding, ParameterCodec,
    ParameterEditorBindingProjection, ParameterFormatter, ParameterId, ParameterInfo,
    ParameterPatchPath, ParameterRange, ParameterRegistry,
};
use lindelion_ui::glirdir_vizia::{
    GlirdirEditorControlKind, GlirdirEditorParameterBinding, GlirdirEditorSurfaceSlot,
};

use crate::{
    audition::DEFAULT_AUDITION_VOLUME,
    patch::{
        AnalysisSettings, AuditionSettings, CaptureSettings, DEFAULT_ANALYSIS_CONFIDENCE_THRESHOLD,
        DEFAULT_ANALYSIS_NOTE_MS, DEFAULT_ONSET_SENSITIVITY, GlirdirPatch,
        MAX_ANALYSIS_CONFIDENCE_THRESHOLD, MAX_ANALYSIS_NOTE_MS, MAX_ONSET_SENSITIVITY,
        MIN_ANALYSIS_CONFIDENCE_THRESHOLD, MIN_ANALYSIS_NOTE_MS, MIN_ONSET_SENSITIVITY,
    },
};

use super::{
    AUDITION_VOLUME_PARAMETER_ID, CAPTURE_BARS_PARAMETER_ID, CONFIDENCE_PARAMETER_ID,
    COUNT_IN_PARAMETER_ID, GRID_PARAMETER_ID, MIN_NOTE_PARAMETER_ID,
    ONSET_SENSITIVITY_PARAMETER_ID, ROOT_PARAMETER_ID, SCALE_PARAMETER_ID, SNAP_PARAMETER_ID,
    SYNC_MODE_PARAMETER_ID, TIMING_STRENGTH_PARAMETER_ID, VELOCITY_AMOUNT_PARAMETER_ID,
    codecs::{
        CaptureBars, CountInBars, RootNoteParameter, ScaleParameter, SnapModeParameter,
        SyncModeParameter, TimingGridParameter,
    },
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorParameterMetadata {
    pub(crate) slot: GlirdirEditorSurfaceSlot,
    pub(crate) label: &'static str,
    pub(crate) control: GlirdirEditorControlKind,
}

impl EditorParameterMetadata {
    const fn new(
        slot: GlirdirEditorSurfaceSlot,
        label: &'static str,
        control: GlirdirEditorControlKind,
    ) -> Self {
        Self {
            slot,
            label,
            control,
        }
    }
}

impl ParameterEditorBindingProjection<GlirdirEditorParameterBinding> for EditorParameterMetadata {
    fn project_editor_binding(self, id: ParameterId) -> GlirdirEditorParameterBinding {
        GlirdirEditorParameterBinding::new(id.0, self.slot, self.label, self.control)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterApplyKind {
    Capture,
    Analysis,
    Quantize,
    Audition,
    Ignored,
}

pub(crate) type ParameterBinding = RegistryParameterBinding<
    ParameterPath,
    ParameterApplyKind,
    (),
    (),
    ParameterFormatter,
    EditorParameterMetadata,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterPath {
    Capture(CaptureParameter),
    Analysis(AnalysisParameter),
    Quantize(QuantizeParameter),
    Audition(AuditionParameter),
}

impl ParameterPatchPath<GlirdirPatch> for ParameterPath {
    fn plain_value(self, patch: &GlirdirPatch) -> f32 {
        match self {
            Self::Capture(parameter) => parameter.plain_value(patch.capture),
            Self::Analysis(parameter) => parameter.plain_value(patch.analysis),
            Self::Quantize(parameter) => parameter.plain_value(&patch.quantize),
            Self::Audition(parameter) => parameter.plain_value(patch.audition),
        }
    }

    fn apply_plain(self, patch: &mut GlirdirPatch, value: f32) {
        match self {
            Self::Capture(parameter) => parameter.apply_plain(&mut patch.capture, value),
            Self::Analysis(parameter) => parameter.apply_plain(&mut patch.analysis, value),
            Self::Quantize(parameter) => parameter.apply_plain(&mut patch.quantize, value),
            Self::Audition(parameter) => parameter.apply_plain(&mut patch.audition, value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureParameter {
    Bars,
    SyncMode,
    CountInBars,
}

impl CaptureParameter {
    fn plain_value(self, settings: CaptureSettings) -> f32 {
        match self {
            Self::Bars => CaptureBars::from_bars(settings.bars).plain(),
            Self::SyncMode => SyncModeParameter::from_sync_mode(settings.sync_mode).plain(),
            Self::CountInBars => CountInBars::from_bars(settings.count_in_bars).plain(),
        }
    }

    fn apply_plain(self, settings: &mut CaptureSettings, value: f32) {
        match self {
            Self::Bars => settings.bars = CaptureBars::from_plain(value).bars(),
            Self::SyncMode => settings.sync_mode = SyncModeParameter::from_plain(value).sync_mode(),
            Self::CountInBars => settings.count_in_bars = CountInBars::from_plain(value).bars(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnalysisParameter {
    ConfidenceThreshold,
    OnsetSensitivity,
    MinNoteMs,
}

impl AnalysisParameter {
    fn plain_value(self, settings: AnalysisSettings) -> f32 {
        match self {
            Self::ConfidenceThreshold => settings.confidence_threshold,
            Self::OnsetSensitivity => settings.onset_sensitivity,
            Self::MinNoteMs => settings.min_note_ms,
        }
    }

    fn apply_plain(self, settings: &mut AnalysisSettings, value: f32) {
        match self {
            Self::ConfidenceThreshold => settings.confidence_threshold = value,
            Self::OnsetSensitivity => settings.onset_sensitivity = value,
            Self::MinNoteMs => settings.min_note_ms = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuantizeParameter {
    Root,
    Scale,
    Snap,
    Grid,
    TimingStrength,
    VelocityAmount,
}

impl QuantizeParameter {
    fn plain_value(self, settings: &QuantizeSettings) -> f32 {
        match self {
            Self::Root => RootNoteParameter::from_root(settings.root).plain(),
            Self::Scale => ScaleParameter::from_scale(&settings.scale).plain(),
            Self::Snap => SnapModeParameter::from_snap_mode(settings.snap_mode).plain(),
            Self::Grid => TimingGridParameter::from_grid(settings.grid).plain(),
            Self::TimingStrength => settings.timing_strength,
            Self::VelocityAmount => settings.velocity_amount,
        }
    }

    fn apply_plain(self, settings: &mut QuantizeSettings, value: f32) {
        match self {
            Self::Root => settings.root = RootNoteParameter::from_plain(value).root(),
            Self::Scale => settings.scale = ScaleParameter::from_plain(value).scale(),
            Self::Snap => settings.snap_mode = SnapModeParameter::from_plain(value).snap_mode(),
            Self::Grid => settings.grid = TimingGridParameter::from_plain(value).grid(),
            Self::TimingStrength => settings.timing_strength = value,
            Self::VelocityAmount => settings.velocity_amount = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuditionParameter {
    Volume,
}

impl AuditionParameter {
    fn plain_value(self, settings: AuditionSettings) -> f32 {
        match self {
            Self::Volume => settings.volume,
        }
    }

    fn apply_plain(self, settings: &mut AuditionSettings, value: f32) {
        match self {
            Self::Volume => settings.volume = value,
        }
    }
}

lindelion_plugin_shell::define_parameter_bindings! {
    binding: ParameterBinding;
    parameters: pub const PARAMETERS;
    bindings: pub(crate) const PARAMETER_BINDINGS;
    defaults {
        runtime: (),
        smoothing: None::<()>,
    }

    ParameterInfo::stepped(
        CAPTURE_BARS_PARAMETER_ID,
        "Capture Bars",
        "bars",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ) => {
        path: ParameterPath::Capture(CaptureParameter::Bars),
        apply: ParameterApplyKind::Capture,
        format: ParameterFormatter::Label(capture_bars_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::CaptureBars,
            "Bars",
            GlirdirEditorControlKind::Segmented {
                labels: <CaptureBars as ParameterCodec>::LABELS,
                width: 126.0,
            },
        )),
    },
    ParameterInfo::stepped(
        SYNC_MODE_PARAMETER_ID,
        "Sync Mode",
        "",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ) => {
        path: ParameterPath::Capture(CaptureParameter::SyncMode),
        apply: ParameterApplyKind::Capture,
        format: ParameterFormatter::Label(sync_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::SyncMode,
            "Sync",
            GlirdirEditorControlKind::Segmented {
                labels: <SyncModeParameter as ParameterCodec>::LABELS,
                width: 168.0,
            },
        )),
    },
    ParameterInfo::stepped(
        COUNT_IN_PARAMETER_ID,
        "Count-In",
        "bars",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ) => {
        path: ParameterPath::Capture(CaptureParameter::CountInBars),
        apply: ParameterApplyKind::Capture,
        format: ParameterFormatter::Label(count_in_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::CountIn,
            "Count",
            GlirdirEditorControlKind::Segmented {
                labels: <CountInBars as ParameterCodec>::LABELS,
                width: 126.0,
            },
        )),
    },
    ParameterInfo::continuous(
        CONFIDENCE_PARAMETER_ID,
        "Detection Confidence",
        "",
        ParameterRange::linear(
            MIN_ANALYSIS_CONFIDENCE_THRESHOLD,
            MAX_ANALYSIS_CONFIDENCE_THRESHOLD,
            DEFAULT_ANALYSIS_CONFIDENCE_THRESHOLD,
        ),
    ) => {
        path: ParameterPath::Analysis(AnalysisParameter::ConfidenceThreshold),
        apply: ParameterApplyKind::Analysis,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::Confidence,
            "Confidence",
            GlirdirEditorControlKind::Knob,
        )),
    },
    ParameterInfo::continuous(
        ONSET_SENSITIVITY_PARAMETER_ID,
        "Onset Sensitivity",
        "",
        ParameterRange::linear(
            MIN_ONSET_SENSITIVITY,
            MAX_ONSET_SENSITIVITY,
            DEFAULT_ONSET_SENSITIVITY,
        ),
    ) => {
        path: ParameterPath::Analysis(AnalysisParameter::OnsetSensitivity),
        apply: ParameterApplyKind::Analysis,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::OnsetSensitivity,
            "Onset",
            GlirdirEditorControlKind::Knob,
        )),
    },
    ParameterInfo::continuous(
        MIN_NOTE_PARAMETER_ID,
        "Minimum Note",
        "ms",
        ParameterRange::linear(
            MIN_ANALYSIS_NOTE_MS,
            MAX_ANALYSIS_NOTE_MS,
            DEFAULT_ANALYSIS_NOTE_MS,
        ),
    ) => {
        path: ParameterPath::Analysis(AnalysisParameter::MinNoteMs),
        apply: ParameterApplyKind::Analysis,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::MinNote,
            "Min Note",
            GlirdirEditorControlKind::Knob,
        )),
    },
    ParameterInfo::stepped(
        ROOT_PARAMETER_ID,
        "Root Note",
        "",
        ParameterRange::linear(0.0, 11.0, 0.0),
        11,
    ) => {
        path: ParameterPath::Quantize(QuantizeParameter::Root),
        apply: ParameterApplyKind::Quantize,
        format: ParameterFormatter::Label(root_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::Root,
            "Key",
            GlirdirEditorControlKind::Selector {
                labels: <RootNoteParameter as ParameterCodec>::LABELS,
                width: 128.0,
            },
        )),
    },
    ParameterInfo::stepped(
        SCALE_PARAMETER_ID,
        "Scale",
        "",
        ParameterRange::linear(0.0, 9.0, 0.0),
        9,
    ) => {
        path: ParameterPath::Quantize(QuantizeParameter::Scale),
        apply: ParameterApplyKind::Quantize,
        format: ParameterFormatter::Label(scale_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::Scale,
            "Scale",
            GlirdirEditorControlKind::Selector {
                labels: <ScaleParameter as ParameterCodec>::LABELS,
                width: 164.0,
            },
        )),
    },
    ParameterInfo::stepped(
        SNAP_PARAMETER_ID,
        "Snap Mode",
        "",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ) => {
        path: ParameterPath::Quantize(QuantizeParameter::Snap),
        apply: ParameterApplyKind::Quantize,
        format: ParameterFormatter::Label(snap_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::Snap,
            "Snap",
            GlirdirEditorControlKind::Segmented {
                labels: <SnapModeParameter as ParameterCodec>::LABELS,
                width: 150.0,
            },
        )),
    },
    ParameterInfo::stepped(
        GRID_PARAMETER_ID,
        "Grid",
        "",
        ParameterRange::linear(0.0, 6.0, 2.0),
        6,
    ) => {
        path: ParameterPath::Quantize(QuantizeParameter::Grid),
        apply: ParameterApplyKind::Quantize,
        format: ParameterFormatter::Label(grid_label_from_plain),
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::Grid,
            "Grid",
            GlirdirEditorControlKind::Selector {
                labels: <TimingGridParameter as ParameterCodec>::LABELS,
                width: 142.0,
            },
        )),
    },
    ParameterInfo::continuous(
        TIMING_STRENGTH_PARAMETER_ID,
        "Quantize Strength",
        "",
        ParameterRange::linear(0.0, 1.0, 1.0),
    ) => {
        path: ParameterPath::Quantize(QuantizeParameter::TimingStrength),
        apply: ParameterApplyKind::Quantize,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::TimingStrength,
            "Strength",
            GlirdirEditorControlKind::Slider { width: 216.0 },
        )),
    },
    ParameterInfo::continuous(
        VELOCITY_AMOUNT_PARAMETER_ID,
        "Velocity Amount",
        "",
        ParameterRange::linear(0.0, 1.0, 0.0),
    ) => {
        path: ParameterPath::Quantize(QuantizeParameter::VelocityAmount),
        apply: ParameterApplyKind::Quantize,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::VelocityAmount,
            "Velocity",
            GlirdirEditorControlKind::Slider { width: 216.0 },
        )),
    },
    ParameterInfo::continuous(
        AUDITION_VOLUME_PARAMETER_ID,
        "Audition Volume",
        "",
        ParameterRange::linear(0.0, 1.0, DEFAULT_AUDITION_VOLUME),
    ) => {
        path: ParameterPath::Audition(AuditionParameter::Volume),
        apply: ParameterApplyKind::Audition,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            GlirdirEditorSurfaceSlot::AuditionVolume,
            "Volume",
            GlirdirEditorControlKind::Slider { width: 216.0 },
        )),
    },
}

pub(crate) const PARAMETER_REGISTRY: ParameterRegistry<ParameterBinding> =
    ParameterRegistry::new(PARAMETER_BINDINGS);
pub(crate) const PARAMETER_BINDING_COUNT: usize = PARAMETER_REGISTRY.len();

pub(crate) fn parameter_binding(id: u32) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding(id)
}

pub(crate) fn parameter_binding_by_index(index: usize) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding_by_index(index)
}

pub(crate) fn parameter_binding_index(id: u32) -> Option<usize> {
    PARAMETER_REGISTRY.binding_index(id)
}

pub(crate) fn parameter_info(id: u32) -> Option<ParameterInfo> {
    PARAMETER_REGISTRY.info(id)
}

pub(crate) fn normalized_parameter_value(id: u32, plain: f32) -> Option<f32> {
    PARAMETER_REGISTRY.normalized_value(id, plain)
}

pub(crate) fn denormalized_parameter_value(id: u32, normalized: f32) -> Option<f32> {
    PARAMETER_REGISTRY.denormalized_value(id, normalized)
}

pub(crate) fn editor_parameter_bindings() -> impl Iterator<Item = GlirdirEditorParameterBinding> {
    PARAMETER_REGISTRY.projected_editor_bindings()
}

pub(crate) fn dispatch_parameter_normalized<Dispatcher>(
    patch: &mut GlirdirPatch,
    id: u32,
    normalized: f32,
    dispatcher: &mut Dispatcher,
) -> ParameterApplyKind
where
    Dispatcher: ParameterApplyDispatcher<GlirdirPatch, ParameterApplyKind, ()>,
{
    PARAMETER_REGISTRY
        .dispatch_normalized(patch, id, normalized, dispatcher)
        .map(|outcome| outcome.apply_kind)
        .unwrap_or(ParameterApplyKind::Ignored)
}

pub(crate) fn apply_parameter_normalized(
    patch: &mut GlirdirPatch,
    id: u32,
    normalized: f32,
) -> ParameterApplyKind {
    PARAMETER_REGISTRY
        .apply_normalized(patch, id, normalized)
        .map(|outcome| outcome.apply_kind)
        .unwrap_or(ParameterApplyKind::Ignored)
}

pub(crate) fn format_parameter_plain_value(id: u32, plain: f32) -> String {
    PARAMETER_REGISTRY.formatted_plain_value(id, plain)
}

pub(super) fn capture_bars_label_from_plain(value: f32) -> &'static str {
    CaptureBars::label_from_plain(value)
}

pub(super) fn sync_label_from_plain(value: f32) -> &'static str {
    SyncModeParameter::label_from_plain(value)
}

pub(super) fn count_in_label_from_plain(value: f32) -> &'static str {
    CountInBars::label_from_plain(value)
}

pub(super) fn root_label_from_plain(value: f32) -> &'static str {
    RootNoteParameter::label_from_plain(value)
}

pub(super) fn scale_label_from_plain(value: f32) -> &'static str {
    ScaleParameter::label_from_plain(value)
}

pub(super) fn snap_label_from_plain(value: f32) -> &'static str {
    SnapModeParameter::label_from_plain(value)
}

pub(super) fn grid_label_from_plain(value: f32) -> &'static str {
    TimingGridParameter::label_from_plain(value)
}
