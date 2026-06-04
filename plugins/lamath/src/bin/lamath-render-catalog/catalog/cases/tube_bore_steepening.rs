//! Bore-steepening A/B cases. These keep the accepted inertial aperture, strong h3 body, and bell
//! levels fixed, then toggle the amplitude-dependent allpass steepener inside the feedback path.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeBellLevel};

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

macro_rules! bore_steepening_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $bell:expr, $enabled:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_bore_steepening",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubeBoreSteepeningPhrase {
                bell: $bell,
                steepening_enabled: $enabled,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_BORE_STEEPENING_CASES: [CatalogCase; 8] = [
    bore_steepening_case!(
        "tube_steepening_v100_current_nominal",
        "Tube Bore Steepening C4-C5 Velocity 100 (current steepening, nominal bell)",
        "18_tube_bore_steepening/tube_steepening_v100_current_nominal.wav",
        ["tube", "bore-steepening", "current", "bell-nominal", "C4-C5", "velocity-100"],
        TubeBellLevel::Nominal,
        true,
        SCALE_V100
    ),
    bore_steepening_case!(
        "tube_steepening_v100_off_nominal",
        "Tube Bore Steepening C4-C5 Velocity 100 (steepening off, nominal bell)",
        "18_tube_bore_steepening/tube_steepening_v100_off_nominal.wav",
        ["tube", "bore-steepening", "off", "bell-nominal", "C4-C5", "velocity-100"],
        TubeBellLevel::Nominal,
        false,
        SCALE_V100
    ),
    bore_steepening_case!(
        "tube_steepening_v100_current_full",
        "Tube Bore Steepening C4-C5 Velocity 100 (current steepening, full bell)",
        "18_tube_bore_steepening/tube_steepening_v100_current_full.wav",
        ["tube", "bore-steepening", "current", "bell-full", "C4-C5", "velocity-100"],
        TubeBellLevel::Full,
        true,
        SCALE_V100
    ),
    bore_steepening_case!(
        "tube_steepening_v100_off_full",
        "Tube Bore Steepening C4-C5 Velocity 100 (steepening off, full bell)",
        "18_tube_bore_steepening/tube_steepening_v100_off_full.wav",
        ["tube", "bore-steepening", "off", "bell-full", "C4-C5", "velocity-100"],
        TubeBellLevel::Full,
        false,
        SCALE_V100
    ),
    bore_steepening_case!(
        "tube_steepening_v127_current_nominal",
        "Tube Bore Steepening C4-C5 Velocity 127 (current steepening, nominal bell)",
        "18_tube_bore_steepening/tube_steepening_v127_current_nominal.wav",
        ["tube", "bore-steepening", "current", "bell-nominal", "C4-C5", "velocity-127"],
        TubeBellLevel::Nominal,
        true,
        SCALE_V127
    ),
    bore_steepening_case!(
        "tube_steepening_v127_off_nominal",
        "Tube Bore Steepening C4-C5 Velocity 127 (steepening off, nominal bell)",
        "18_tube_bore_steepening/tube_steepening_v127_off_nominal.wav",
        ["tube", "bore-steepening", "off", "bell-nominal", "C4-C5", "velocity-127"],
        TubeBellLevel::Nominal,
        false,
        SCALE_V127
    ),
    bore_steepening_case!(
        "tube_steepening_v127_current_full",
        "Tube Bore Steepening C4-C5 Velocity 127 (current steepening, full bell)",
        "18_tube_bore_steepening/tube_steepening_v127_current_full.wav",
        ["tube", "bore-steepening", "current", "bell-full", "C4-C5", "velocity-127"],
        TubeBellLevel::Full,
        true,
        SCALE_V127
    ),
    bore_steepening_case!(
        "tube_steepening_v127_off_full",
        "Tube Bore Steepening C4-C5 Velocity 127 (steepening off, full bell)",
        "18_tube_bore_steepening/tube_steepening_v127_off_full.wav",
        ["tube", "bore-steepening", "off", "bell-full", "C4-C5", "velocity-127"],
        TubeBellLevel::Full,
        false,
        SCALE_V127
    ),
];
