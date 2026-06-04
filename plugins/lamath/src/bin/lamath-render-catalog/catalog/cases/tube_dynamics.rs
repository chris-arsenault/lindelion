//! Tube dynamics & bell audition cases (ADR-0032 item-B follow-up). The driven wind Tube plays
//! a connected (slurred) C4→C5 scale at three velocities (soft/medium/hard) so the velocity
//! dynamic can be heard as a *scale*. These are bell A/B cases only; separate model experiments
//! must add their own current-vs-new cases with the changed mechanism isolated.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeReedAperture};

const PHRASE_DURATION_SECONDS: f32 = 3.4;

/// C-major scale C4→C5, connected (each note overlaps the next), at one velocity. A slurred wind
/// line so the reed keeps blowing across the scale and the sustained tone is audible per step.
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

const SCALE_V020: [ScheduledNote; 8] = c4_c5_scale!(20.0 / 127.0);
const SCALE_V100: [ScheduledNote; 8] = c4_c5_scale!(100.0 / 127.0);
const SCALE_V127: [ScheduledNote; 8] = c4_c5_scale!(1.0);

macro_rules! tube_dynamics_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $bell:literal, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_dynamics",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubePhrase {
                polyphony: 1,
                retrigger: false,
                bell: $bell,
                reed_aperture: TubeReedAperture::Inertial,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_DYNAMICS_CASES: [CatalogCase; 6] = [
    tube_dynamics_case!(
        "tube_dyn_scale_v020_bell_on",
        "Tube Dynamics Scale C4-C5 Velocity 20 (bell on)",
        "11_tube_dynamics/tube_dyn_scale_v020_bell_on.wav",
        [
            "tube",
            "dynamics",
            "scale",
            "C4-C5",
            "velocity-20",
            "bell-on"
        ],
        true,
        SCALE_V020
    ),
    tube_dynamics_case!(
        "tube_dyn_scale_v020_bell_off",
        "Tube Dynamics Scale C4-C5 Velocity 20 (bell off)",
        "11_tube_dynamics/tube_dyn_scale_v020_bell_off.wav",
        [
            "tube",
            "dynamics",
            "scale",
            "C4-C5",
            "velocity-20",
            "bell-off"
        ],
        false,
        SCALE_V020
    ),
    tube_dynamics_case!(
        "tube_dyn_scale_v100_bell_on",
        "Tube Dynamics Scale C4-C5 Velocity 100 (bell on)",
        "11_tube_dynamics/tube_dyn_scale_v100_bell_on.wav",
        [
            "tube",
            "dynamics",
            "scale",
            "C4-C5",
            "velocity-100",
            "bell-on"
        ],
        true,
        SCALE_V100
    ),
    tube_dynamics_case!(
        "tube_dyn_scale_v100_bell_off",
        "Tube Dynamics Scale C4-C5 Velocity 100 (bell off)",
        "11_tube_dynamics/tube_dyn_scale_v100_bell_off.wav",
        [
            "tube",
            "dynamics",
            "scale",
            "C4-C5",
            "velocity-100",
            "bell-off"
        ],
        false,
        SCALE_V100
    ),
    tube_dynamics_case!(
        "tube_dyn_scale_v127_bell_on",
        "Tube Dynamics Scale C4-C5 Velocity 127 (bell on)",
        "11_tube_dynamics/tube_dyn_scale_v127_bell_on.wav",
        [
            "tube",
            "dynamics",
            "scale",
            "C4-C5",
            "velocity-127",
            "bell-on"
        ],
        true,
        SCALE_V127
    ),
    tube_dynamics_case!(
        "tube_dyn_scale_v127_bell_off",
        "Tube Dynamics Scale C4-C5 Velocity 127 (bell off)",
        "11_tube_dynamics/tube_dyn_scale_v127_bell_off.wav",
        [
            "tube",
            "dynamics",
            "scale",
            "C4-C5",
            "velocity-127",
            "bell-off"
        ],
        false,
        SCALE_V127
    ),
];
