//! Body-formant mix audition cases. These assume the tracked-h3 body path is useful but was too
//! quiet/masked in the first audition, then vary bell contribution around a stronger body setting.

use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeBellLevel, TubeBodyFormantLevel,
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

macro_rules! body_mix_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $bell:expr, $level:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_body_formant_mix",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::TubeBodyFormantMixPhrase {
                bell: $bell,
                level: $level,
            },
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_BODY_FORMANT_MIX_CASES: [CatalogCase; 10] = [
    body_mix_case!(
        "tube_body_mix_v100_current_full",
        "Tube Body Mix C4-C5 Velocity 100 (current body, full bell)",
        "16_tube_body_formant_mix/tube_body_mix_v100_current_full.wav",
        [
            "tube",
            "body-formant",
            "current",
            "bell-full",
            "C4-C5",
            "velocity-100"
        ],
        TubeBellLevel::Full,
        TubeBodyFormantLevel::Current,
        SCALE_V100
    ),
    body_mix_case!(
        "tube_body_mix_v100_current_body",
        "Tube Body Mix C4-C5 Velocity 100 (current body, bell off)",
        "16_tube_body_formant_mix/tube_body_mix_v100_current_body.wav",
        [
            "tube",
            "body-formant",
            "current",
            "bell-off",
            "C4-C5",
            "velocity-100"
        ],
        TubeBellLevel::Off,
        TubeBodyFormantLevel::Current,
        SCALE_V100
    ),
    body_mix_case!(
        "tube_body_mix_v100_strong_body",
        "Tube Body Mix C4-C5 Velocity 100 (strong h3 body, bell off)",
        "16_tube_body_formant_mix/tube_body_mix_v100_strong_body.wav",
        [
            "tube",
            "body-formant",
            "strong",
            "bell-off",
            "C4-C5",
            "velocity-100"
        ],
        TubeBellLevel::Off,
        TubeBodyFormantLevel::Strong,
        SCALE_V100
    ),
    body_mix_case!(
        "tube_body_mix_v100_strong_nominal_bell",
        "Tube Body Mix C4-C5 Velocity 100 (strong h3 body, nominal bell)",
        "16_tube_body_formant_mix/tube_body_mix_v100_strong_nominal_bell.wav",
        [
            "tube",
            "body-formant",
            "strong",
            "bell-nominal",
            "C4-C5",
            "velocity-100"
        ],
        TubeBellLevel::Nominal,
        TubeBodyFormantLevel::Strong,
        SCALE_V100
    ),
    body_mix_case!(
        "tube_body_mix_v100_strong_full_bell",
        "Tube Body Mix C4-C5 Velocity 100 (strong h3 body, full bell)",
        "16_tube_body_formant_mix/tube_body_mix_v100_strong_full_bell.wav",
        [
            "tube",
            "body-formant",
            "strong",
            "bell-full",
            "C4-C5",
            "velocity-100"
        ],
        TubeBellLevel::Full,
        TubeBodyFormantLevel::Strong,
        SCALE_V100
    ),
    body_mix_case!(
        "tube_body_mix_v127_current_full",
        "Tube Body Mix C4-C5 Velocity 127 (current body, full bell)",
        "16_tube_body_formant_mix/tube_body_mix_v127_current_full.wav",
        [
            "tube",
            "body-formant",
            "current",
            "bell-full",
            "C4-C5",
            "velocity-127"
        ],
        TubeBellLevel::Full,
        TubeBodyFormantLevel::Current,
        SCALE_V127
    ),
    body_mix_case!(
        "tube_body_mix_v127_current_body",
        "Tube Body Mix C4-C5 Velocity 127 (current body, bell off)",
        "16_tube_body_formant_mix/tube_body_mix_v127_current_body.wav",
        [
            "tube",
            "body-formant",
            "current",
            "bell-off",
            "C4-C5",
            "velocity-127"
        ],
        TubeBellLevel::Off,
        TubeBodyFormantLevel::Current,
        SCALE_V127
    ),
    body_mix_case!(
        "tube_body_mix_v127_strong_body",
        "Tube Body Mix C4-C5 Velocity 127 (strong h3 body, bell off)",
        "16_tube_body_formant_mix/tube_body_mix_v127_strong_body.wav",
        [
            "tube",
            "body-formant",
            "strong",
            "bell-off",
            "C4-C5",
            "velocity-127"
        ],
        TubeBellLevel::Off,
        TubeBodyFormantLevel::Strong,
        SCALE_V127
    ),
    body_mix_case!(
        "tube_body_mix_v127_strong_nominal_bell",
        "Tube Body Mix C4-C5 Velocity 127 (strong h3 body, nominal bell)",
        "16_tube_body_formant_mix/tube_body_mix_v127_strong_nominal_bell.wav",
        [
            "tube",
            "body-formant",
            "strong",
            "bell-nominal",
            "C4-C5",
            "velocity-127"
        ],
        TubeBellLevel::Nominal,
        TubeBodyFormantLevel::Strong,
        SCALE_V127
    ),
    body_mix_case!(
        "tube_body_mix_v127_strong_full_bell",
        "Tube Body Mix C4-C5 Velocity 127 (strong h3 body, full bell)",
        "16_tube_body_formant_mix/tube_body_mix_v127_strong_full_bell.wav",
        [
            "tube",
            "body-formant",
            "strong",
            "bell-full",
            "C4-C5",
            "velocity-127"
        ],
        TubeBellLevel::Full,
        TubeBodyFormantLevel::Strong,
        SCALE_V127
    ),
];
