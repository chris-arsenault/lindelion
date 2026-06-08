//! Tube output-path contribution audit. These cases do not introduce a new model; they use the
//! existing bell/body switches to expose which current output path carries the objectionable HF.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote};

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

macro_rules! output_path_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $bell:literal, $body:literal, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_output_paths",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubePathAuditPhrase {
                bell_enabled: $bell,
                body_enabled: $body,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_OUTPUT_PATH_CASES: [CatalogCase; 8] = [
    output_path_case!(
        "tube_output_paths_v100_current",
        "Tube Output Paths C4-C5 Velocity 100 (current body + bell)",
        "14_tube_output_paths/tube_output_paths_v100_current.wav",
        [
            "tube",
            "output-paths",
            "current",
            "body-on",
            "bell-on",
            "C4-C5",
            "velocity-100"
        ],
        true,
        true,
        SCALE_V100
    ),
    output_path_case!(
        "tube_output_paths_v100_body_only",
        "Tube Output Paths C4-C5 Velocity 100 (body path only)",
        "14_tube_output_paths/tube_output_paths_v100_body_only.wav",
        [
            "tube",
            "output-paths",
            "body-only",
            "body-on",
            "bell-off",
            "C4-C5",
            "velocity-100"
        ],
        false,
        true,
        SCALE_V100
    ),
    output_path_case!(
        "tube_output_paths_v100_pickup_bell",
        "Tube Output Paths C4-C5 Velocity 100 (dry pickup + bell)",
        "14_tube_output_paths/tube_output_paths_v100_pickup_bell.wav",
        [
            "tube",
            "output-paths",
            "pickup-bell",
            "body-off",
            "bell-on",
            "C4-C5",
            "velocity-100"
        ],
        true,
        false,
        SCALE_V100
    ),
    output_path_case!(
        "tube_output_paths_v100_dry_pickup",
        "Tube Output Paths C4-C5 Velocity 100 (dry pickup only)",
        "14_tube_output_paths/tube_output_paths_v100_dry_pickup.wav",
        [
            "tube",
            "output-paths",
            "dry-pickup",
            "body-off",
            "bell-off",
            "C4-C5",
            "velocity-100"
        ],
        false,
        false,
        SCALE_V100
    ),
    output_path_case!(
        "tube_output_paths_v127_current",
        "Tube Output Paths C4-C5 Velocity 127 (current body + bell)",
        "14_tube_output_paths/tube_output_paths_v127_current.wav",
        [
            "tube",
            "output-paths",
            "current",
            "body-on",
            "bell-on",
            "C4-C5",
            "velocity-127"
        ],
        true,
        true,
        SCALE_V127
    ),
    output_path_case!(
        "tube_output_paths_v127_body_only",
        "Tube Output Paths C4-C5 Velocity 127 (body path only)",
        "14_tube_output_paths/tube_output_paths_v127_body_only.wav",
        [
            "tube",
            "output-paths",
            "body-only",
            "body-on",
            "bell-off",
            "C4-C5",
            "velocity-127"
        ],
        false,
        true,
        SCALE_V127
    ),
    output_path_case!(
        "tube_output_paths_v127_pickup_bell",
        "Tube Output Paths C4-C5 Velocity 127 (dry pickup + bell)",
        "14_tube_output_paths/tube_output_paths_v127_pickup_bell.wav",
        [
            "tube",
            "output-paths",
            "pickup-bell",
            "body-off",
            "bell-on",
            "C4-C5",
            "velocity-127"
        ],
        true,
        false,
        SCALE_V127
    ),
    output_path_case!(
        "tube_output_paths_v127_dry_pickup",
        "Tube Output Paths C4-C5 Velocity 127 (dry pickup only)",
        "14_tube_output_paths/tube_output_paths_v127_dry_pickup.wav",
        [
            "tube",
            "output-paths",
            "dry-pickup",
            "body-off",
            "bell-off",
            "C4-C5",
            "velocity-127"
        ],
        false,
        false,
        SCALE_V127
    ),
];
