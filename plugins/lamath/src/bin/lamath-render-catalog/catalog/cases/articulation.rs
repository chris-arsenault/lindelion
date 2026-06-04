//! Articulation cases. The driven wind Tube phrases a C-major scale (C4→C5) under different
//! articulations (tonguing/legato/slur — ADR-0032), and the struck Mesh idiophone is played
//! across the articulations a single sustained strike can't show: a melodic scale, overlapping
//! (legato) strikes, a rapid same-note restrike, and a chord rendered polyphonically versus
//! monophonically (one voice-stealing body). These are audition cases — the Mesh is a struck
//! plate, judged by ear, not an objective pitch target.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule};
use notes::{
    CHORD_CMAJ, EXPRESSIVE_16, RANDOM_16, RESTRIKE_C4, SCALE_LEGATO, SCALE_TONGUED,
    SLOW_CLIMB_C2_C6,
};

mod notes;

const PHRASE_DURATION_SECONDS: f32 = 3.0;

macro_rules! articulation_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $recipe:expr, $notes:ident) => {
        articulation_case!(
            $id,
            $title,
            $relative_wav,
            [$($tag),+],
            $recipe,
            $notes,
            PHRASE_DURATION_SECONDS
        )
    };
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $recipe:expr, $notes:ident, $duration:expr) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "articulation",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: $recipe,
            schedule: RenderSchedule {
                duration_seconds: $duration,
                notes: &$notes,
            },
        }
    };
}

/// Mesh note-sequence phrasing: poly so overlapping rings sum, retrigger so each note re-strikes.
const MESH_PHRASE: PatchRecipe = PatchRecipe::MeshPhrase {
    polyphony: 16,
    retrigger: true,
};

// The Tube is monophonic (normalization forces polyphony 1), so articulation is *how it changes
// notes*, not overlapping voices: tongued separates and re-strikes the bore (the excitation kick);
// slurred overlaps so the reed keeps blowing and the bore frequency glides between pitches.
pub(super) const ARTICULATION_CASES: [CatalogCase; 10] = [
    articulation_case!(
        "tube_scale_tongued_c4_c5",
        "Tube Scale Tongued C4-C5",
        "09_articulation/tube_scale_tongued_c4_c5.wav",
        ["articulation", "tube", "scale", "tongued", "C4-C5"],
        PatchRecipe::TubePhrase {
            polyphony: 1,
            retrigger: true,
            bell: true
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
            retrigger: false,
            bell: true
        },
        SCALE_LEGATO
    ),
    // Mesh idiophone articulations (struck plate, auditioned by ear).
    articulation_case!(
        "mesh_scale_c4_c5",
        "Mesh Scale C4-C5",
        "09_articulation/mesh_scale_c4_c5.wav",
        ["articulation", "mesh", "scale", "C4-C5"],
        PatchRecipe::MeshPhrase {
            polyphony: 16,
            retrigger: true
        },
        SCALE_TONGUED
    ),
    articulation_case!(
        "mesh_scale_overlap_c4_c5",
        "Mesh Scale Overlap C4-C5",
        "09_articulation/mesh_scale_overlap_c4_c5.wav",
        ["articulation", "mesh", "scale", "overlap", "legato"],
        PatchRecipe::MeshPhrase {
            polyphony: 16,
            retrigger: true
        },
        SCALE_LEGATO
    ),
    articulation_case!(
        "mesh_rapid_restrike_c4",
        "Mesh Rapid Restrike C4",
        "09_articulation/mesh_rapid_restrike_c4.wav",
        ["articulation", "mesh", "restrike", "C4"],
        PatchRecipe::MeshPhrase {
            polyphony: 1,
            retrigger: false
        },
        RESTRIKE_C4
    ),
    articulation_case!(
        "mesh_chord_polyphonic",
        "Mesh Chord Polyphonic",
        "09_articulation/mesh_chord_polyphonic.wav",
        ["articulation", "mesh", "chord", "polyphonic"],
        PatchRecipe::MeshPhrase {
            polyphony: 16,
            retrigger: true
        },
        CHORD_CMAJ
    ),
    articulation_case!(
        "mesh_chord_monophonic",
        "Mesh Chord Monophonic",
        "09_articulation/mesh_chord_monophonic.wav",
        ["articulation", "mesh", "chord", "monophonic"],
        PatchRecipe::MeshPhrase {
            polyphony: 1,
            retrigger: true
        },
        CHORD_CMAJ
    ),
    articulation_case!(
        "mesh_slow_climb_c2_c6",
        "Mesh Slow Climb C2-C6",
        "09_articulation/mesh_slow_climb_c2_c6.wav",
        ["articulation", "mesh", "climb", "C2-C6"],
        MESH_PHRASE,
        SLOW_CLIMB_C2_C6,
        6.5
    ),
    articulation_case!(
        "mesh_random_16",
        "Mesh Random 16 Notes",
        "09_articulation/mesh_random_16.wav",
        ["articulation", "mesh", "random"],
        MESH_PHRASE,
        RANDOM_16,
        4.5
    ),
    articulation_case!(
        "mesh_expressive_16",
        "Mesh Expressive 16 Notes",
        "09_articulation/mesh_expressive_16.wav",
        ["articulation", "mesh", "expressive"],
        MESH_PHRASE,
        EXPRESSIVE_16,
        6.0
    ),
];
