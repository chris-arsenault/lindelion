//! Bell-radiation shape A/B cases. These keep the accepted strong h3 body and nominal bell tuning
//! available, then compare the current 2nd-order bell highpass against a gentler first-order
//! radiation transfer outside the oscillator loop.

use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeBellLevel, TubeRadiationShape,
};

const PHRASE_DURATION_SECONDS: f32 = 3.4;

macro_rules! c4_c5_scale {
    ($vel:expr) => {
        [
            ScheduledNote {
                start_seconds: 0.00,
                end_seconds: 0.45,
                note: 60,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 0.38,
                end_seconds: 0.83,
                note: 62,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 0.76,
                end_seconds: 1.21,
                note: 64,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 1.14,
                end_seconds: 1.59,
                note: 65,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 1.52,
                end_seconds: 1.97,
                note: 67,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 1.90,
                end_seconds: 2.35,
                note: 69,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 2.28,
                end_seconds: 2.73,
                note: 71,
                velocity: $vel,
            },
            ScheduledNote {
                start_seconds: 2.66,
                end_seconds: 3.20,
                note: 72,
                velocity: $vel,
            },
        ]
    };
}

const SCALE_V100: [ScheduledNote; 8] = c4_c5_scale!(100.0 / 127.0);
const SCALE_V127: [ScheduledNote; 8] = c4_c5_scale!(1.0);

macro_rules! radiation_shape_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $bell:expr, $shape:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_radiation_shape",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubeRadiationShapePhrase {
                bell: $bell,
                shape: $shape,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_RADIATION_SHAPE_CASES: [CatalogCase; 8] = [
    radiation_shape_case!(
        "tube_radiation_v100_current_nominal",
        "Tube Radiation C4-C5 Velocity 100 (current radiation, nominal bell)",
        "17_tube_radiation_shape/tube_radiation_v100_current_nominal.wav",
        ["tube", "radiation-shape", "current", "bell-nominal", "C4-C5", "velocity-100"],
        TubeBellLevel::Nominal,
        TubeRadiationShape::Current,
        SCALE_V100
    ),
    radiation_shape_case!(
        "tube_radiation_v100_gentle_nominal",
        "Tube Radiation C4-C5 Velocity 100 (gentle radiation, nominal bell)",
        "17_tube_radiation_shape/tube_radiation_v100_gentle_nominal.wav",
        ["tube", "radiation-shape", "gentle", "bell-nominal", "C4-C5", "velocity-100"],
        TubeBellLevel::Nominal,
        TubeRadiationShape::Gentle,
        SCALE_V100
    ),
    radiation_shape_case!(
        "tube_radiation_v100_current_full",
        "Tube Radiation C4-C5 Velocity 100 (current radiation, full bell)",
        "17_tube_radiation_shape/tube_radiation_v100_current_full.wav",
        ["tube", "radiation-shape", "current", "bell-full", "C4-C5", "velocity-100"],
        TubeBellLevel::Full,
        TubeRadiationShape::Current,
        SCALE_V100
    ),
    radiation_shape_case!(
        "tube_radiation_v100_gentle_full",
        "Tube Radiation C4-C5 Velocity 100 (gentle radiation, full bell)",
        "17_tube_radiation_shape/tube_radiation_v100_gentle_full.wav",
        ["tube", "radiation-shape", "gentle", "bell-full", "C4-C5", "velocity-100"],
        TubeBellLevel::Full,
        TubeRadiationShape::Gentle,
        SCALE_V100
    ),
    radiation_shape_case!(
        "tube_radiation_v127_current_nominal",
        "Tube Radiation C4-C5 Velocity 127 (current radiation, nominal bell)",
        "17_tube_radiation_shape/tube_radiation_v127_current_nominal.wav",
        ["tube", "radiation-shape", "current", "bell-nominal", "C4-C5", "velocity-127"],
        TubeBellLevel::Nominal,
        TubeRadiationShape::Current,
        SCALE_V127
    ),
    radiation_shape_case!(
        "tube_radiation_v127_gentle_nominal",
        "Tube Radiation C4-C5 Velocity 127 (gentle radiation, nominal bell)",
        "17_tube_radiation_shape/tube_radiation_v127_gentle_nominal.wav",
        ["tube", "radiation-shape", "gentle", "bell-nominal", "C4-C5", "velocity-127"],
        TubeBellLevel::Nominal,
        TubeRadiationShape::Gentle,
        SCALE_V127
    ),
    radiation_shape_case!(
        "tube_radiation_v127_current_full",
        "Tube Radiation C4-C5 Velocity 127 (current radiation, full bell)",
        "17_tube_radiation_shape/tube_radiation_v127_current_full.wav",
        ["tube", "radiation-shape", "current", "bell-full", "C4-C5", "velocity-127"],
        TubeBellLevel::Full,
        TubeRadiationShape::Current,
        SCALE_V127
    ),
    radiation_shape_case!(
        "tube_radiation_v127_gentle_full",
        "Tube Radiation C4-C5 Velocity 127 (gentle radiation, full bell)",
        "17_tube_radiation_shape/tube_radiation_v127_gentle_full.wav",
        ["tube", "radiation-shape", "gentle", "bell-full", "C4-C5", "velocity-127"],
        TubeBellLevel::Full,
        TubeRadiationShape::Gentle,
        SCALE_V127
    ),
];
