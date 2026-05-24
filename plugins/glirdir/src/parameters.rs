use lindelion_midi::{QuantizeSettings, RootNote, Scale, SnapMode, TimingGrid};
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
        MIN_ANALYSIS_CONFIDENCE_THRESHOLD, MIN_ANALYSIS_NOTE_MS, MIN_ONSET_SENSITIVITY, SyncMode,
    },
};

pub const CAPTURE_BARS_PARAMETER_ID: u32 = 1;
pub const SYNC_MODE_PARAMETER_ID: u32 = 2;
pub const COUNT_IN_PARAMETER_ID: u32 = 3;
pub const CONFIDENCE_PARAMETER_ID: u32 = 10;
pub const ONSET_SENSITIVITY_PARAMETER_ID: u32 = 11;
pub const MIN_NOTE_PARAMETER_ID: u32 = 12;
pub const ROOT_PARAMETER_ID: u32 = 20;
pub const SCALE_PARAMETER_ID: u32 = 21;
pub const SNAP_PARAMETER_ID: u32 = 22;
pub const GRID_PARAMETER_ID: u32 = 23;
pub const TIMING_STRENGTH_PARAMETER_ID: u32 = 24;
pub const VELOCITY_AMOUNT_PARAMETER_ID: u32 = 25;
pub const AUDITION_VOLUME_PARAMETER_ID: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureBars {
    Four,
    Eight,
    Sixteen,
}

impl CaptureBars {
    fn from_bars(bars: u8) -> Self {
        match bars {
            0..=4 => Self::Four,
            5..=8 => Self::Eight,
            _ => Self::Sixteen,
        }
    }

    const fn bars(self) -> u8 {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for CaptureBars {
        max: 2;
        fallback: Self::Four;
        0 => Self::Four, "4";
        1 => Self::Eight, "8";
        2 => Self::Sixteen, "16";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncModeParameter {
    Immediate,
    PhraseBoundary,
    NextDownbeat,
}

impl SyncModeParameter {
    fn from_sync_mode(sync_mode: SyncMode) -> Self {
        match sync_mode {
            SyncMode::Immediate => Self::Immediate,
            SyncMode::PhraseBoundary => Self::PhraseBoundary,
            SyncMode::NextDownbeat => Self::NextDownbeat,
        }
    }

    const fn sync_mode(self) -> SyncMode {
        match self {
            Self::Immediate => SyncMode::Immediate,
            Self::PhraseBoundary => SyncMode::PhraseBoundary,
            Self::NextDownbeat => SyncMode::NextDownbeat,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for SyncModeParameter {
        max: 2;
        fallback: Self::Immediate;
        0 => Self::Immediate, "Now";
        1 => Self::PhraseBoundary, "Phrase";
        2 => Self::NextDownbeat, "Bar";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountInBars {
    Zero,
    One,
    Two,
}

impl CountInBars {
    fn from_bars(bars: u8) -> Self {
        match bars {
            1 => Self::One,
            2.. => Self::Two,
            _ => Self::Zero,
        }
    }

    fn bars(self) -> u8 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for CountInBars {
        max: 2;
        fallback: Self::Zero;
        0 => Self::Zero, "0";
        1 => Self::One, "1";
        2 => Self::Two, "2";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootNoteParameter {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl RootNoteParameter {
    fn from_root(root: RootNote) -> Self {
        match root {
            RootNote::C => Self::C,
            RootNote::CSharp => Self::CSharp,
            RootNote::D => Self::D,
            RootNote::DSharp => Self::DSharp,
            RootNote::E => Self::E,
            RootNote::F => Self::F,
            RootNote::FSharp => Self::FSharp,
            RootNote::G => Self::G,
            RootNote::GSharp => Self::GSharp,
            RootNote::A => Self::A,
            RootNote::ASharp => Self::ASharp,
            RootNote::B => Self::B,
        }
    }

    fn root(self) -> RootNote {
        match self {
            Self::C => RootNote::C,
            Self::CSharp => RootNote::CSharp,
            Self::D => RootNote::D,
            Self::DSharp => RootNote::DSharp,
            Self::E => RootNote::E,
            Self::F => RootNote::F,
            Self::FSharp => RootNote::FSharp,
            Self::G => RootNote::G,
            Self::GSharp => RootNote::GSharp,
            Self::A => RootNote::A,
            Self::ASharp => RootNote::ASharp,
            Self::B => RootNote::B,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for RootNoteParameter {
        max: 11;
        fallback: Self::C;
        0 => Self::C, "C";
        1 => Self::CSharp, "C#";
        2 => Self::D, "D";
        3 => Self::DSharp, "D#";
        4 => Self::E, "E";
        5 => Self::F, "F";
        6 => Self::FSharp, "F#";
        7 => Self::G, "G";
        8 => Self::GSharp, "G#";
        9 => Self::A, "A";
        10 => Self::ASharp, "A#";
        11 => Self::B, "B";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScaleParameter {
    Chromatic,
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    PentatonicMajor,
    PentatonicMinor,
    Blues,
    Dorian,
    Mixolydian,
}

impl ScaleParameter {
    fn from_scale(scale: &Scale) -> Self {
        match scale {
            Scale::Chromatic => Self::Chromatic,
            Scale::Major => Self::Major,
            Scale::NaturalMinor => Self::NaturalMinor,
            Scale::HarmonicMinor => Self::HarmonicMinor,
            Scale::MelodicMinor => Self::MelodicMinor,
            Scale::PentatonicMajor => Self::PentatonicMajor,
            Scale::PentatonicMinor => Self::PentatonicMinor,
            Scale::Blues => Self::Blues,
            Scale::Dorian => Self::Dorian,
            Scale::Mixolydian | Scale::Custom(_) => Self::Mixolydian,
        }
    }

    fn scale(self) -> Scale {
        match self {
            Self::Chromatic => Scale::Chromatic,
            Self::Major => Scale::Major,
            Self::NaturalMinor => Scale::NaturalMinor,
            Self::HarmonicMinor => Scale::HarmonicMinor,
            Self::MelodicMinor => Scale::MelodicMinor,
            Self::PentatonicMajor => Scale::PentatonicMajor,
            Self::PentatonicMinor => Scale::PentatonicMinor,
            Self::Blues => Scale::Blues,
            Self::Dorian => Scale::Dorian,
            Self::Mixolydian => Scale::Mixolydian,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for ScaleParameter {
        max: 9;
        fallback: Self::Chromatic;
        0 => Self::Chromatic, "Chrom";
        1 => Self::Major, "Major";
        2 => Self::NaturalMinor, "Minor";
        3 => Self::HarmonicMinor, "Harm";
        4 => Self::MelodicMinor, "Mel";
        5 => Self::PentatonicMajor, "Penta Maj";
        6 => Self::PentatonicMinor, "Penta Min";
        7 => Self::Blues, "Blues";
        8 => Self::Dorian, "Dorian";
        9 => Self::Mixolydian, "Mix";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapModeParameter {
    Hard,
    Soft,
    Off,
}

impl SnapModeParameter {
    fn from_snap_mode(snap_mode: SnapMode) -> Self {
        match snap_mode {
            SnapMode::Hard => Self::Hard,
            SnapMode::Soft => Self::Soft,
            SnapMode::None => Self::Off,
        }
    }

    fn snap_mode(self) -> SnapMode {
        match self {
            Self::Hard => SnapMode::Hard,
            Self::Soft => SnapMode::Soft,
            Self::Off => SnapMode::None,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for SnapModeParameter {
        max: 2;
        fallback: Self::Hard;
        0 => Self::Hard, "Hard";
        1 => Self::Soft, "Soft";
        2 => Self::Off, "Off";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingGridParameter {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    QuarterTriplet,
    EighthTriplet,
    SixteenthTriplet,
}

impl TimingGridParameter {
    fn from_grid(grid: TimingGrid) -> Self {
        match grid {
            TimingGrid::Quarter => Self::Quarter,
            TimingGrid::Eighth => Self::Eighth,
            TimingGrid::Sixteenth => Self::Sixteenth,
            TimingGrid::ThirtySecond => Self::ThirtySecond,
            TimingGrid::QuarterTriplet => Self::QuarterTriplet,
            TimingGrid::EighthTriplet => Self::EighthTriplet,
            TimingGrid::SixteenthTriplet => Self::SixteenthTriplet,
        }
    }

    fn grid(self) -> TimingGrid {
        match self {
            Self::Quarter => TimingGrid::Quarter,
            Self::Eighth => TimingGrid::Eighth,
            Self::Sixteenth => TimingGrid::Sixteenth,
            Self::ThirtySecond => TimingGrid::ThirtySecond,
            Self::QuarterTriplet => TimingGrid::QuarterTriplet,
            Self::EighthTriplet => TimingGrid::EighthTriplet,
            Self::SixteenthTriplet => TimingGrid::SixteenthTriplet,
        }
    }
}

lindelion_plugin_shell::define_parameter_codec! {
    impl ParameterCodec for TimingGridParameter {
        max: 6;
        fallback: Self::Quarter;
        0 => Self::Quarter, "1/4";
        1 => Self::Eighth, "1/8";
        2 => Self::Sixteenth, "1/16";
        3 => Self::ThirtySecond, "1/32";
        4 => Self::QuarterTriplet, "1/4T";
        5 => Self::EighthTriplet, "1/8T";
        6 => Self::SixteenthTriplet, "1/16T";
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorParameterMetadata {
    slot: GlirdirEditorSurfaceSlot,
    label: &'static str,
    control: GlirdirEditorControlKind,
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
            GlirdirEditorControlKind::Slider { width: 216.0 },
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
            GlirdirEditorControlKind::Slider { width: 216.0 },
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
            GlirdirEditorControlKind::Slider { width: 216.0 },
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

pub(crate) fn parameter_default_normalized_value_by_index(index: usize) -> Option<f32> {
    PARAMETER_REGISTRY.default_normalized_value_by_index(index)
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

pub(crate) fn patch_parameter_normalized_value(patch: &GlirdirPatch, id: u32) -> Option<f32> {
    PARAMETER_REGISTRY.normalized_patch_value(patch, id)
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

fn capture_bars_label_from_plain(value: f32) -> &'static str {
    CaptureBars::label_from_plain(value)
}

fn sync_label_from_plain(value: f32) -> &'static str {
    SyncModeParameter::label_from_plain(value)
}

fn count_in_label_from_plain(value: f32) -> &'static str {
    CountInBars::label_from_plain(value)
}

fn root_label_from_plain(value: f32) -> &'static str {
    RootNoteParameter::label_from_plain(value)
}

fn scale_label_from_plain(value: f32) -> &'static str {
    ScaleParameter::label_from_plain(value)
}

fn snap_label_from_plain(value: f32) -> &'static str {
    SnapModeParameter::label_from_plain(value)
}

fn grid_label_from_plain(value: f32) -> &'static str {
    TimingGridParameter::label_from_plain(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_strength_updates_patch_through_binding() {
        let mut patch = GlirdirPatch::default();

        let apply = parameter_binding(TIMING_STRENGTH_PARAMETER_ID)
            .unwrap()
            .apply_plain(&mut patch, 0.25);

        assert_eq!(apply, ParameterApplyKind::Quantize);
        assert_eq!(patch.quantize.timing_strength, 0.25);
    }

    #[test]
    fn scale_parameter_uses_shared_scale_type() {
        let mut patch = GlirdirPatch::default();

        parameter_binding(SCALE_PARAMETER_ID)
            .unwrap()
            .apply_plain(&mut patch, 2.0);

        assert_eq!(patch.quantize.scale, Scale::NaturalMinor);
    }

    #[test]
    fn capture_bars_middle_host_step_selects_eight_bars() {
        let mut patch = GlirdirPatch::default();
        let binding = parameter_binding(CAPTURE_BARS_PARAMETER_ID).unwrap();

        let middle_plain = binding.info().range.denormalize(0.5);
        let apply = binding.apply_plain(&mut patch, middle_plain);

        assert_eq!(apply, ParameterApplyKind::Capture);
        assert_eq!(patch.capture.bars, 8);
        assert_eq!(binding.plain_value(&patch), 1.0);
        assert_eq!(binding.format_plain_value(middle_plain), "8");
    }

    #[test]
    fn every_host_parameter_resolves_to_one_binding() {
        assert_eq!(PARAMETERS.len(), PARAMETER_BINDINGS.len());
        assert_eq!(PARAMETER_BINDING_COUNT, PARAMETER_BINDINGS.len());

        for (index, parameter) in PARAMETERS.iter().enumerate() {
            let matches = PARAMETER_BINDINGS
                .iter()
                .filter(|binding| binding.info().id == parameter.id)
                .count();
            assert_eq!(matches, 1, "parameter {:?} binding count", parameter.id);
            assert_eq!(
                parameter_binding(parameter.id.0).map(|binding| binding.info()),
                Some(*parameter)
            );
            assert_eq!(parameter_binding_index(parameter.id.0), Some(index));
            assert_eq!(
                parameter_binding_by_index(index).map(|binding| binding.info()),
                Some(*parameter)
            );
        }
    }

    #[test]
    fn every_binding_round_trips_patch_get_set() {
        for binding in PARAMETER_BINDINGS {
            let mut patch = GlirdirPatch::default();
            let value = non_default_probe_value(binding.info().range);

            let apply = binding.apply_plain(&mut patch, value);

            assert_eq!(apply, binding.apply());
            let actual = binding.plain_value(&patch);
            assert!(
                (actual - value).abs() < 0.001,
                "parameter {} ({}) round-tripped as {actual}, expected {value}",
                binding.id().0,
                binding.info().name
            );
        }
    }

    #[test]
    fn formatters_are_owned_by_bindings() {
        assert_eq!(
            parameter_binding(CAPTURE_BARS_PARAMETER_ID)
                .unwrap()
                .format_plain_value(1.0),
            "8"
        );
        assert_eq!(
            parameter_binding(SYNC_MODE_PARAMETER_ID)
                .unwrap()
                .format_plain_value(2.0),
            "Bar"
        );
        assert_eq!(
            parameter_binding(SCALE_PARAMETER_ID)
                .unwrap()
                .format_plain_value(2.0),
            "Minor"
        );
        assert_eq!(
            parameter_binding(GRID_PARAMETER_ID)
                .unwrap()
                .format_plain_value(4.0),
            "1/4T"
        );
        assert_eq!(
            parameter_binding(TIMING_STRENGTH_PARAMETER_ID)
                .unwrap()
                .format_plain_value(0.25),
            "0.25"
        );
    }

    #[test]
    fn every_visible_editor_control_resolves_to_one_parameter_binding() {
        let editor_bindings = editor_parameter_bindings().collect::<Vec<_>>();
        assert_eq!(editor_bindings.len(), GlirdirEditorSurfaceSlot::ALL.len());

        for editor in editor_bindings.iter().copied() {
            let matches = PARAMETER_BINDINGS
                .iter()
                .filter(|binding| binding.id().0 == editor.id())
                .count();
            assert_eq!(matches, 1, "editor binding {:?}", editor.slot());

            let registry_binding = parameter_binding(editor.id()).expect("projected binding id");
            let metadata = registry_binding
                .editor()
                .expect("projected binding editor metadata");
            assert_eq!(metadata.slot, editor.slot());
            assert_eq!(metadata.label, editor.label());
            assert_eq!(metadata.control, editor.control());
        }

        for slot in GlirdirEditorSurfaceSlot::ALL {
            let count = editor_bindings
                .iter()
                .filter(|binding| binding.slot() == slot)
                .count();
            assert_eq!(count, 1, "editor slot {slot:?}");
        }
    }

    #[test]
    fn enum_codecs_round_trip() {
        assert_codec_roundtrip(&[CaptureBars::Four, CaptureBars::Eight, CaptureBars::Sixteen]);
        assert_codec_roundtrip(&[
            SyncModeParameter::Immediate,
            SyncModeParameter::PhraseBoundary,
            SyncModeParameter::NextDownbeat,
        ]);
        assert_codec_roundtrip(&[CountInBars::Zero, CountInBars::One, CountInBars::Two]);
        assert_codec_roundtrip(&[
            RootNoteParameter::C,
            RootNoteParameter::CSharp,
            RootNoteParameter::D,
            RootNoteParameter::DSharp,
            RootNoteParameter::E,
            RootNoteParameter::F,
            RootNoteParameter::FSharp,
            RootNoteParameter::G,
            RootNoteParameter::GSharp,
            RootNoteParameter::A,
            RootNoteParameter::ASharp,
            RootNoteParameter::B,
        ]);
        assert_codec_roundtrip(&[
            ScaleParameter::Chromatic,
            ScaleParameter::Major,
            ScaleParameter::NaturalMinor,
            ScaleParameter::HarmonicMinor,
            ScaleParameter::MelodicMinor,
            ScaleParameter::PentatonicMajor,
            ScaleParameter::PentatonicMinor,
            ScaleParameter::Blues,
            ScaleParameter::Dorian,
            ScaleParameter::Mixolydian,
        ]);
        assert_codec_roundtrip(&[
            SnapModeParameter::Hard,
            SnapModeParameter::Soft,
            SnapModeParameter::Off,
        ]);
        assert_codec_roundtrip(&[
            TimingGridParameter::Quarter,
            TimingGridParameter::Eighth,
            TimingGridParameter::Sixteenth,
            TimingGridParameter::ThirtySecond,
            TimingGridParameter::QuarterTriplet,
            TimingGridParameter::EighthTriplet,
            TimingGridParameter::SixteenthTriplet,
        ]);
    }

    fn non_default_probe_value(range: ParameterRange) -> f32 {
        if (range.default - range.min).abs() > 0.001 {
            range.min
        } else {
            range.max
        }
    }

    fn assert_codec_roundtrip<T>(values: &[T])
    where
        T: ParameterCodec + std::fmt::Debug + PartialEq,
    {
        assert_eq!(values.len(), T::LABELS.len());
        assert_eq!(T::MAX_INDEX as usize + 1, values.len());

        for (index, value) in values.iter().copied().enumerate() {
            assert_eq!(value.to_index(), index as u32);
            assert_eq!(T::from_plain(value.plain()), value);
            assert_eq!(T::from_index(index as u32), value);
            assert!(!value.label().is_empty());
        }
    }
}
