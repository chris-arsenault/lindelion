use lindelion_midi::{RootNote, Scale, SnapMode, TimingGrid};
use lindelion_plugin_shell::{ParameterId, ParameterInfo, ParameterRange};
use lindelion_ui::glirdir_vizia::{
    GlirdirEditorControlKind, GlirdirEditorParameterBinding, GlirdirEditorSurfaceSlot,
};

use crate::patch::{CaptureBars, GlirdirPatch, SyncMode};

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

const BARS_LABELS: &[&str] = &["4", "8", "16"];
const SYNC_LABELS: &[&str] = &["Now", "Phrase", "Bar"];
const COUNT_IN_LABELS: &[&str] = &["0", "1", "2"];
const ROOT_LABELS: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const SCALE_LABELS: &[&str] = &[
    "Chrom",
    "Major",
    "Minor",
    "Harm",
    "Mel",
    "Penta Maj",
    "Penta Min",
    "Blues",
    "Dorian",
    "Mix",
];
const SNAP_LABELS: &[&str] = &["Hard", "Soft", "Off"];
const GRID_LABELS: &[&str] = &["1/4", "1/8", "1/16", "1/32", "1/4T", "1/8T", "1/16T"];

pub const PARAMETERS: &[ParameterInfo] = &[
    ParameterInfo::stepped(
        CAPTURE_BARS_PARAMETER_ID,
        "Capture Bars",
        "bars",
        ParameterRange::linear(4.0, 16.0, 4.0),
        2,
    ),
    ParameterInfo::stepped(
        SYNC_MODE_PARAMETER_ID,
        "Sync Mode",
        "",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ),
    ParameterInfo::stepped(
        COUNT_IN_PARAMETER_ID,
        "Count-In",
        "bars",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ),
    ParameterInfo::continuous(
        CONFIDENCE_PARAMETER_ID,
        "Detection Confidence",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
    ),
    ParameterInfo::continuous(
        ONSET_SENSITIVITY_PARAMETER_ID,
        "Onset Sensitivity",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
    ),
    ParameterInfo::continuous(
        MIN_NOTE_PARAMETER_ID,
        "Minimum Note",
        "ms",
        ParameterRange::linear(30.0, 300.0, 80.0),
    ),
    ParameterInfo::stepped(
        ROOT_PARAMETER_ID,
        "Root Note",
        "",
        ParameterRange::linear(0.0, 11.0, 0.0),
        11,
    ),
    ParameterInfo::stepped(
        SCALE_PARAMETER_ID,
        "Scale",
        "",
        ParameterRange::linear(0.0, 9.0, 0.0),
        9,
    ),
    ParameterInfo::stepped(
        SNAP_PARAMETER_ID,
        "Snap Mode",
        "",
        ParameterRange::linear(0.0, 2.0, 0.0),
        2,
    ),
    ParameterInfo::stepped(
        GRID_PARAMETER_ID,
        "Grid",
        "",
        ParameterRange::linear(0.0, 6.0, 2.0),
        6,
    ),
    ParameterInfo::continuous(
        TIMING_STRENGTH_PARAMETER_ID,
        "Quantize Strength",
        "",
        ParameterRange::linear(0.0, 1.0, 1.0),
    ),
    ParameterInfo::continuous(
        VELOCITY_AMOUNT_PARAMETER_ID,
        "Velocity Amount",
        "",
        ParameterRange::linear(0.0, 1.0, 0.0),
    ),
    ParameterInfo::continuous(
        AUDITION_VOLUME_PARAMETER_ID,
        "Audition Volume",
        "",
        ParameterRange::linear(0.0, 1.0, 0.35),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterApplyKind {
    Capture,
    Analysis,
    Quantize,
    Audition,
    Ignored,
}

#[derive(Debug, Clone, Copy)]
pub struct ParameterBinding {
    info: ParameterInfo,
    apply: ParameterApplyKind,
}

impl ParameterBinding {
    pub const fn new(info: ParameterInfo, apply: ParameterApplyKind) -> Self {
        Self { info, apply }
    }

    pub const fn info(self) -> ParameterInfo {
        self.info
    }

    pub const fn apply(self) -> ParameterApplyKind {
        self.apply
    }
}

pub const PARAMETER_BINDINGS: &[ParameterBinding] = &[
    ParameterBinding::new(PARAMETERS[0], ParameterApplyKind::Capture),
    ParameterBinding::new(PARAMETERS[1], ParameterApplyKind::Capture),
    ParameterBinding::new(PARAMETERS[2], ParameterApplyKind::Capture),
    ParameterBinding::new(PARAMETERS[3], ParameterApplyKind::Analysis),
    ParameterBinding::new(PARAMETERS[4], ParameterApplyKind::Analysis),
    ParameterBinding::new(PARAMETERS[5], ParameterApplyKind::Analysis),
    ParameterBinding::new(PARAMETERS[6], ParameterApplyKind::Quantize),
    ParameterBinding::new(PARAMETERS[7], ParameterApplyKind::Quantize),
    ParameterBinding::new(PARAMETERS[8], ParameterApplyKind::Quantize),
    ParameterBinding::new(PARAMETERS[9], ParameterApplyKind::Quantize),
    ParameterBinding::new(PARAMETERS[10], ParameterApplyKind::Quantize),
    ParameterBinding::new(PARAMETERS[11], ParameterApplyKind::Quantize),
    ParameterBinding::new(PARAMETERS[12], ParameterApplyKind::Audition),
];

pub const EDITOR_PARAMETER_BINDINGS: &[GlirdirEditorParameterBinding] = &[
    GlirdirEditorParameterBinding::new(
        CAPTURE_BARS_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::CaptureBars,
        "Bars",
        GlirdirEditorControlKind::Segmented {
            labels: BARS_LABELS,
            width: 126.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        SYNC_MODE_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::SyncMode,
        "Sync",
        GlirdirEditorControlKind::Segmented {
            labels: SYNC_LABELS,
            width: 168.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        COUNT_IN_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::CountIn,
        "Count",
        GlirdirEditorControlKind::Segmented {
            labels: COUNT_IN_LABELS,
            width: 126.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        CONFIDENCE_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::Confidence,
        "Confidence",
        GlirdirEditorControlKind::Slider { width: 216.0 },
    ),
    GlirdirEditorParameterBinding::new(
        ONSET_SENSITIVITY_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::OnsetSensitivity,
        "Onset",
        GlirdirEditorControlKind::Slider { width: 216.0 },
    ),
    GlirdirEditorParameterBinding::new(
        MIN_NOTE_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::MinNote,
        "Min Note",
        GlirdirEditorControlKind::Slider { width: 216.0 },
    ),
    GlirdirEditorParameterBinding::new(
        ROOT_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::Root,
        "Key",
        GlirdirEditorControlKind::Selector {
            labels: ROOT_LABELS,
            width: 128.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        SCALE_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::Scale,
        "Scale",
        GlirdirEditorControlKind::Selector {
            labels: SCALE_LABELS,
            width: 164.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        SNAP_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::Snap,
        "Snap",
        GlirdirEditorControlKind::Segmented {
            labels: SNAP_LABELS,
            width: 150.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        GRID_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::Grid,
        "Grid",
        GlirdirEditorControlKind::Selector {
            labels: GRID_LABELS,
            width: 142.0,
        },
    ),
    GlirdirEditorParameterBinding::new(
        TIMING_STRENGTH_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::TimingStrength,
        "Strength",
        GlirdirEditorControlKind::Slider { width: 216.0 },
    ),
    GlirdirEditorParameterBinding::new(
        VELOCITY_AMOUNT_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::VelocityAmount,
        "Velocity",
        GlirdirEditorControlKind::Slider { width: 216.0 },
    ),
    GlirdirEditorParameterBinding::new(
        AUDITION_VOLUME_PARAMETER_ID,
        GlirdirEditorSurfaceSlot::AuditionVolume,
        "Volume",
        GlirdirEditorControlKind::Slider { width: 216.0 },
    ),
];

pub fn parameter_binding(id: u32) -> Option<ParameterBinding> {
    PARAMETER_BINDINGS
        .iter()
        .copied()
        .find(|binding| binding.info.id == ParameterId(id))
}

pub fn editor_parameter_bindings() -> impl Iterator<Item = GlirdirEditorParameterBinding> {
    EDITOR_PARAMETER_BINDINGS.iter().copied()
}

pub fn apply_parameter_plain(patch: &mut GlirdirPatch, id: u32, plain: f32) -> ParameterApplyKind {
    let Some(binding) = parameter_binding(id) else {
        return ParameterApplyKind::Ignored;
    };
    let info = binding.info();
    let plain = info.range.denormalize(info.range.normalize(plain));

    match id {
        CAPTURE_BARS_PARAMETER_ID => patch.capture.bars = CaptureBars::from_plain(plain),
        SYNC_MODE_PARAMETER_ID => patch.capture.sync_mode = SyncMode::from_plain(plain),
        COUNT_IN_PARAMETER_ID => patch.capture.count_in_bars = plain.round().clamp(0.0, 2.0) as u8,
        CONFIDENCE_PARAMETER_ID => patch.analysis.confidence_threshold = plain,
        ONSET_SENSITIVITY_PARAMETER_ID => patch.analysis.onset_sensitivity = plain,
        MIN_NOTE_PARAMETER_ID => patch.analysis.min_note_ms = plain,
        ROOT_PARAMETER_ID => patch.quantize.root = root_from_plain(plain),
        SCALE_PARAMETER_ID => patch.quantize.scale = scale_from_plain(plain),
        SNAP_PARAMETER_ID => patch.quantize.snap_mode = snap_from_plain(plain),
        GRID_PARAMETER_ID => patch.quantize.grid = grid_from_plain(plain),
        TIMING_STRENGTH_PARAMETER_ID => patch.quantize.timing_strength = plain,
        VELOCITY_AMOUNT_PARAMETER_ID => patch.quantize.velocity_amount = plain,
        AUDITION_VOLUME_PARAMETER_ID => patch.audition.volume = plain,
        _ => return ParameterApplyKind::Ignored,
    }

    binding.apply()
}

fn root_from_plain(value: f32) -> RootNote {
    RootNote::ALL[value.round().clamp(0.0, 11.0) as usize]
}

fn scale_from_plain(value: f32) -> Scale {
    match value.round() as i32 {
        value if value <= 0 => Scale::Chromatic,
        1 => Scale::Major,
        2 => Scale::NaturalMinor,
        3 => Scale::HarmonicMinor,
        4 => Scale::MelodicMinor,
        5 => Scale::PentatonicMajor,
        6 => Scale::PentatonicMinor,
        7 => Scale::Blues,
        8 => Scale::Dorian,
        _ => Scale::Mixolydian,
    }
}

fn snap_from_plain(value: f32) -> SnapMode {
    match value.round() as i32 {
        value if value <= 0 => SnapMode::Hard,
        1 => SnapMode::Soft,
        _ => SnapMode::None,
    }
}

fn grid_from_plain(value: f32) -> TimingGrid {
    match value.round() as i32 {
        value if value <= 0 => TimingGrid::Quarter,
        1 => TimingGrid::Eighth,
        2 => TimingGrid::Sixteenth,
        3 => TimingGrid::ThirtySecond,
        4 => TimingGrid::QuarterTriplet,
        5 => TimingGrid::EighthTriplet,
        _ => TimingGrid::SixteenthTriplet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_strength_updates_patch_through_binding() {
        let mut patch = GlirdirPatch::default();

        let apply = apply_parameter_plain(&mut patch, TIMING_STRENGTH_PARAMETER_ID, 0.25);

        assert_eq!(apply, ParameterApplyKind::Quantize);
        assert_eq!(patch.quantize.timing_strength, 0.25);
    }

    #[test]
    fn scale_parameter_uses_shared_scale_type() {
        let mut patch = GlirdirPatch::default();

        apply_parameter_plain(&mut patch, SCALE_PARAMETER_ID, 2.0);

        assert_eq!(patch.quantize.scale, Scale::NaturalMinor);
    }

    #[test]
    fn every_host_parameter_resolves_to_one_binding() {
        assert_eq!(PARAMETERS.len(), PARAMETER_BINDINGS.len());

        for parameter in PARAMETERS {
            let matches = PARAMETER_BINDINGS
                .iter()
                .filter(|binding| binding.info().id == parameter.id)
                .count();
            assert_eq!(matches, 1, "parameter {:?} binding count", parameter.id);
            assert_eq!(
                parameter_binding(parameter.id.0).map(ParameterBinding::info),
                Some(*parameter)
            );
        }
    }

    #[test]
    fn every_visible_editor_control_resolves_to_one_parameter_binding() {
        assert_eq!(
            EDITOR_PARAMETER_BINDINGS.len(),
            GlirdirEditorSurfaceSlot::ALL.len()
        );

        for editor in editor_parameter_bindings() {
            let matches = PARAMETER_BINDINGS
                .iter()
                .filter(|binding| binding.info().id.0 == editor.id())
                .count();
            assert_eq!(matches, 1, "editor binding {:?}", editor.slot());
        }

        for slot in GlirdirEditorSurfaceSlot::ALL {
            let count = editor_parameter_bindings()
                .filter(|binding| binding.slot() == slot)
                .count();
            assert_eq!(count, 1, "editor slot {slot:?}");
        }
    }
}
