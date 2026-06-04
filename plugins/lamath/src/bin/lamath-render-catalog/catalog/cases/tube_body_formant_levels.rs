//! Body-formant level audition cases. These keep the bore/reed model unchanged and only vary the
//! body path's tracked-h3 formant level, with body-only cases to prove whether the formant path has
//! enough signal before judging the full mix.

use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeBodyFormantLevel,
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

macro_rules! body_formant_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $bell:literal, $level:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_body_formant_levels",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubeBodyFormantPhrase {
                bell_enabled: $bell,
                level: $level,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_BODY_FORMANT_LEVEL_CASES: [CatalogCase; 10] = [
    body_formant_case!(
        "tube_body_formant_v100_current_full",
        "Tube Body Formant C4-C5 Velocity 100 (current full mix)",
        "15_tube_body_formant_levels/tube_body_formant_v100_current_full.wav",
        ["tube", "body-formant", "current", "full-mix", "C4-C5", "velocity-100"],
        true,
        TubeBodyFormantLevel::Current,
        SCALE_V100
    ),
    body_formant_case!(
        "tube_body_formant_v100_current_body",
        "Tube Body Formant C4-C5 Velocity 100 (current body only)",
        "15_tube_body_formant_levels/tube_body_formant_v100_current_body.wav",
        ["tube", "body-formant", "current", "body-only", "C4-C5", "velocity-100"],
        false,
        TubeBodyFormantLevel::Current,
        SCALE_V100
    ),
    body_formant_case!(
        "tube_body_formant_v100_medium_body",
        "Tube Body Formant C4-C5 Velocity 100 (medium h3 body only)",
        "15_tube_body_formant_levels/tube_body_formant_v100_medium_body.wav",
        ["tube", "body-formant", "medium", "body-only", "C4-C5", "velocity-100"],
        false,
        TubeBodyFormantLevel::Medium,
        SCALE_V100
    ),
    body_formant_case!(
        "tube_body_formant_v100_strong_body",
        "Tube Body Formant C4-C5 Velocity 100 (strong h3 body only)",
        "15_tube_body_formant_levels/tube_body_formant_v100_strong_body.wav",
        ["tube", "body-formant", "strong", "body-only", "C4-C5", "velocity-100"],
        false,
        TubeBodyFormantLevel::Strong,
        SCALE_V100
    ),
    body_formant_case!(
        "tube_body_formant_v100_strong_full",
        "Tube Body Formant C4-C5 Velocity 100 (strong h3 full mix)",
        "15_tube_body_formant_levels/tube_body_formant_v100_strong_full.wav",
        ["tube", "body-formant", "strong", "full-mix", "C4-C5", "velocity-100"],
        true,
        TubeBodyFormantLevel::Strong,
        SCALE_V100
    ),
    body_formant_case!(
        "tube_body_formant_v127_current_full",
        "Tube Body Formant C4-C5 Velocity 127 (current full mix)",
        "15_tube_body_formant_levels/tube_body_formant_v127_current_full.wav",
        ["tube", "body-formant", "current", "full-mix", "C4-C5", "velocity-127"],
        true,
        TubeBodyFormantLevel::Current,
        SCALE_V127
    ),
    body_formant_case!(
        "tube_body_formant_v127_current_body",
        "Tube Body Formant C4-C5 Velocity 127 (current body only)",
        "15_tube_body_formant_levels/tube_body_formant_v127_current_body.wav",
        ["tube", "body-formant", "current", "body-only", "C4-C5", "velocity-127"],
        false,
        TubeBodyFormantLevel::Current,
        SCALE_V127
    ),
    body_formant_case!(
        "tube_body_formant_v127_medium_body",
        "Tube Body Formant C4-C5 Velocity 127 (medium h3 body only)",
        "15_tube_body_formant_levels/tube_body_formant_v127_medium_body.wav",
        ["tube", "body-formant", "medium", "body-only", "C4-C5", "velocity-127"],
        false,
        TubeBodyFormantLevel::Medium,
        SCALE_V127
    ),
    body_formant_case!(
        "tube_body_formant_v127_strong_body",
        "Tube Body Formant C4-C5 Velocity 127 (strong h3 body only)",
        "15_tube_body_formant_levels/tube_body_formant_v127_strong_body.wav",
        ["tube", "body-formant", "strong", "body-only", "C4-C5", "velocity-127"],
        false,
        TubeBodyFormantLevel::Strong,
        SCALE_V127
    ),
    body_formant_case!(
        "tube_body_formant_v127_strong_full",
        "Tube Body Formant C4-C5 Velocity 127 (strong h3 full mix)",
        "15_tube_body_formant_levels/tube_body_formant_v127_strong_full.wav",
        ["tube", "body-formant", "strong", "full-mix", "C4-C5", "velocity-127"],
        true,
        TubeBodyFormantLevel::Strong,
        SCALE_V127
    ),
];
