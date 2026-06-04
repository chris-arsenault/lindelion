//! Reed-aperture A/B audition cases. These compare the current memoryless reed aperture against
//! a finite-inertia aperture model with the same phrase, velocity, bell, bore, and body settings.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeReedAperture};

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

macro_rules! reed_aperture_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $aperture:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_reed_aperture",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubePhrase {
                polyphony: 1,
                retrigger: false,
                bell: true,
                reed_aperture: $aperture,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_REED_APERTURE_CASES: [CatalogCase; 4] = [
    reed_aperture_case!(
        "tube_reed_aperture_v100_current",
        "Tube Reed Aperture C4-C5 Velocity 100 (current instant aperture)",
        "12_tube_reed_aperture/tube_reed_aperture_v100_current.wav",
        [
            "tube",
            "reed-aperture",
            "current",
            "instant-aperture",
            "C4-C5",
            "velocity-100"
        ],
        TubeReedAperture::Instant,
        SCALE_V100
    ),
    reed_aperture_case!(
        "tube_reed_aperture_v100_inertial",
        "Tube Reed Aperture C4-C5 Velocity 100 (inertial aperture)",
        "12_tube_reed_aperture/tube_reed_aperture_v100_inertial.wav",
        [
            "tube",
            "reed-aperture",
            "new",
            "inertial-aperture",
            "C4-C5",
            "velocity-100"
        ],
        TubeReedAperture::Inertial,
        SCALE_V100
    ),
    reed_aperture_case!(
        "tube_reed_aperture_v127_current",
        "Tube Reed Aperture C4-C5 Velocity 127 (current instant aperture)",
        "12_tube_reed_aperture/tube_reed_aperture_v127_current.wav",
        [
            "tube",
            "reed-aperture",
            "current",
            "instant-aperture",
            "C4-C5",
            "velocity-127"
        ],
        TubeReedAperture::Instant,
        SCALE_V127
    ),
    reed_aperture_case!(
        "tube_reed_aperture_v127_inertial",
        "Tube Reed Aperture C4-C5 Velocity 127 (inertial aperture)",
        "12_tube_reed_aperture/tube_reed_aperture_v127_inertial.wav",
        [
            "tube",
            "reed-aperture",
            "new",
            "inertial-aperture",
            "C4-C5",
            "velocity-127"
        ],
        TubeReedAperture::Inertial,
        SCALE_V127
    ),
];
