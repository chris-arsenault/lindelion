use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ResonatorFamily, ScheduledNote, SurroundingRecipe,
};

const CHORD_DURATION_SECONDS: f32 = 3.0;
const V100: f32 = 100.0 / 127.0;

const MAJOR_TRIAD_V100: [ScheduledNote; 3] = [
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 0.9,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 0.9,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 0.9,
        note: 67,
        velocity: V100,
    },
];

const OPEN_FIFTH_OCTAVE_V100: [ScheduledNote; 5] = [
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 48,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 72,
        velocity: V100,
    },
];

const DENSE_CLUSTER_V100: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 61,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 66,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.0,
        note: 72,
        velocity: V100,
    },
];

const REPEATED_STRIKES_V100: [ScheduledNote; 4] = [
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 0.18,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.35,
        end_seconds: 0.53,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.70,
        end_seconds: 0.88,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.05,
        end_seconds: 1.23,
        note: 60,
        velocity: V100,
    },
];

macro_rules! chord_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $recipe:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "chords",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: $recipe,
            schedule: RenderSchedule {
                duration_seconds: CHORD_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const CHORD_CASES: [CatalogCase; 6] = [
    chord_case!(
        "chord_string_major_triad_v100",
        "Chord String Major Triad Velocity 100",
        "07_chords/chord_string_major_triad_v100.wav",
        ["chord", "string", "major-triad", "velocity-100"],
        PatchRecipe::SingleFamily(ResonatorFamily::String),
        MAJOR_TRIAD_V100
    ),
    chord_case!(
        "chord_string_open_fifth_octave_v100",
        "Chord String Open Fifth Octave Velocity 100",
        "07_chords/chord_string_open_fifth_octave_v100.wav",
        ["chord", "string", "open-fifth", "octave", "velocity-100"],
        PatchRecipe::SingleFamily(ResonatorFamily::String),
        OPEN_FIFTH_OCTAVE_V100
    ),
    chord_case!(
        "chord_string_dense_cluster_v100",
        "Chord String Dense Cluster Velocity 100",
        "07_chords/chord_string_dense_cluster_v100.wav",
        ["chord", "string", "dense-cluster", "velocity-100"],
        PatchRecipe::SingleFamily(ResonatorFamily::String),
        DENSE_CLUSTER_V100
    ),
    chord_case!(
        "chord_modal_repeated_strikes_v100",
        "Chord Modal Repeated Strikes Velocity 100",
        "07_chords/chord_modal_repeated_strikes_v100.wav",
        ["chord", "modal", "repeated-strikes", "velocity-100"],
        PatchRecipe::SingleFamily(ResonatorFamily::Modal),
        REPEATED_STRIKES_V100
    ),
    chord_case!(
        "chord_modal_major_triad_sympathetic_off_v100",
        "Chord Modal Major Triad Sympathetic Off Velocity 100",
        "07_chords/chord_modal_major_triad_sympathetic_off_v100.wav",
        [
            "chord",
            "modal",
            "major-triad",
            "sympathetic-off",
            "velocity-100"
        ],
        PatchRecipe::Surrounding {
            family: ResonatorFamily::Modal,
            surrounding: SurroundingRecipe::Off,
        },
        MAJOR_TRIAD_V100
    ),
    chord_case!(
        "chord_modal_major_triad_sympathetic_on_v100",
        "Chord Modal Major Triad Sympathetic On Velocity 100",
        "07_chords/chord_modal_major_triad_sympathetic_on_v100.wav",
        [
            "chord",
            "modal",
            "major-triad",
            "sympathetic-on",
            "velocity-100"
        ],
        PatchRecipe::Surrounding {
            family: ResonatorFamily::Modal,
            surrounding: SurroundingRecipe::Sympathetic,
        },
        MAJOR_TRIAD_V100
    ),
];
