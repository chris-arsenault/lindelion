use lindelion_midi::{RootNote, Scale, SnapMode, TimingGrid};
use lindelion_plugin_shell::ParameterCodec;

use crate::patch::SyncMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureBars {
    Four,
    Eight,
    Sixteen,
}

impl CaptureBars {
    pub(super) fn from_bars(bars: u8) -> Self {
        match bars {
            0..=4 => Self::Four,
            5..=8 => Self::Eight,
            _ => Self::Sixteen,
        }
    }

    pub(super) const fn bars(self) -> u8 {
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
pub(super) enum SyncModeParameter {
    Immediate,
    PhraseBoundary,
    NextDownbeat,
}

impl SyncModeParameter {
    pub(super) fn from_sync_mode(sync_mode: SyncMode) -> Self {
        match sync_mode {
            SyncMode::Immediate => Self::Immediate,
            SyncMode::PhraseBoundary => Self::PhraseBoundary,
            SyncMode::NextDownbeat => Self::NextDownbeat,
        }
    }

    pub(super) const fn sync_mode(self) -> SyncMode {
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
pub(super) enum CountInBars {
    Zero,
    One,
    Two,
}

impl CountInBars {
    pub(super) fn from_bars(bars: u8) -> Self {
        match bars {
            1 => Self::One,
            2.. => Self::Two,
            _ => Self::Zero,
        }
    }

    pub(super) fn bars(self) -> u8 {
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
pub(super) enum RootNoteParameter {
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
    pub(super) fn from_root(root: RootNote) -> Self {
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

    pub(super) fn root(self) -> RootNote {
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
pub(super) enum ScaleParameter {
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
    pub(super) fn from_scale(scale: &Scale) -> Self {
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

    pub(super) fn scale(self) -> Scale {
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
pub(super) enum SnapModeParameter {
    Hard,
    Soft,
    Off,
}

impl SnapModeParameter {
    pub(super) fn from_snap_mode(snap_mode: SnapMode) -> Self {
        match snap_mode {
            SnapMode::Hard => Self::Hard,
            SnapMode::Soft => Self::Soft,
            SnapMode::None => Self::Off,
        }
    }

    pub(super) fn snap_mode(self) -> SnapMode {
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
pub(super) enum TimingGridParameter {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    QuarterTriplet,
    EighthTriplet,
    SixteenthTriplet,
}

impl TimingGridParameter {
    pub(super) fn from_grid(grid: TimingGrid) -> Self {
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

    pub(super) fn grid(self) -> TimingGrid {
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
