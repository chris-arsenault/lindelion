//! Articulation cases: the driven wind Tube phrasing a C-major scale (C4→C5) under different
//! articulations. Single sustained notes can't show a wind voice's expressivity — these reveal
//! whether tonguing, legato, and slurring render audibly distinct phrasing (ADR-0032).

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote};

const PHRASE_DURATION_SECONDS: f32 = 3.0;
const V100: f32 = 100.0 / 127.0;

/// Tongued / separato: each scale note starts after the previous one ends (a gap), so every note
/// is a fresh tongued attack.
const SCALE_TONGUED: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.22,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.30,
        end_seconds: 0.52,
        note: 62,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.60,
        end_seconds: 0.82,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.90,
        end_seconds: 1.12,
        note: 65,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.20,
        end_seconds: 1.42,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.50,
        end_seconds: 1.72,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.80,
        end_seconds: 2.02,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.10,
        end_seconds: 2.32,
        note: 72,
        velocity: V100,
    },
];

/// Legato: each note's onset arrives before the previous note ends (overlap), so the phrase
/// connects rather than separating.
const SCALE_LEGATO: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.33,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.25,
        end_seconds: 0.58,
        note: 62,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.50,
        end_seconds: 0.83,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.75,
        end_seconds: 1.08,
        note: 65,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.00,
        end_seconds: 1.33,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.25,
        end_seconds: 1.58,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.50,
        end_seconds: 1.83,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.75,
        end_seconds: 2.08,
        note: 72,
        velocity: V100,
    },
];

macro_rules! articulation_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $recipe:expr, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "articulation",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: $recipe,
            schedule: RenderSchedule {
                duration_seconds: PHRASE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

// The Tube is monophonic (normalization forces polyphony 1), so articulation is *how it changes
// notes*, not overlapping voices: tongued separates and re-strikes the bore (the excitation kick);
// slurred overlaps so the reed keeps blowing and the bore frequency glides between pitches.
pub(super) const ARTICULATION_CASES: [CatalogCase; 2] = [
    articulation_case!(
        "tube_scale_tongued_c4_c5",
        "Tube Scale Tongued C4-C5",
        "09_articulation/tube_scale_tongued_c4_c5.wav",
        ["articulation", "tube", "scale", "tongued", "C4-C5"],
        PatchRecipe::TubePhrase {
            polyphony: 1,
            retrigger: true
        },
        SCALE_TONGUED
    ),
    articulation_case!(
        "tube_scale_slur_c4_c5",
        "Tube Scale Slur C4-C5",
        "09_articulation/tube_scale_slur_c4_c5.wav",
        ["articulation", "tube", "scale", "slur", "C4-C5"],
        PatchRecipe::TubePhrase {
            polyphony: 1,
            retrigger: false
        },
        SCALE_LEGATO
    ),
];
