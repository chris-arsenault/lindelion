//! Tube articulation-slot cases: the same two-register probe played through
//! each of the eight articulation slots, so their note-start physics (attack
//! overpressure weight and decay, seed transient, onset breath turbulence,
//! slurred entry) can be A/B'd by ear against the neutral Tongue slot.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote};

const PROBE_DURATION_SECONDS: f32 = 3.4;
const V100: f32 = 100.0 / 127.0;

/// The same gesture in both registers — two separated G3s below the break,
/// then two separated D5s above it — so an articulation's attack reads
/// against its immediate repeat and across the register break.
const ARTICULATION_PROBE: [ScheduledNote; 4] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.55,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.70,
        end_seconds: 1.25,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.40,
        end_seconds: 1.95,
        note: 74,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.10,
        end_seconds: 2.65,
        note: 74,
        velocity: V100,
    },
];

macro_rules! slot_case {
    ($slot:literal, $id:literal, $title:literal, $wav:literal, $tag:literal) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "articulation",
            relative_wav: $wav,
            tags: &["articulation", "tube", "slot", $tag],
            patch_recipe: PatchRecipe::TubeArticulationPhrase { slot: $slot },
            schedule: RenderSchedule {
                duration_seconds: PROBE_DURATION_SECONDS,
                notes: &ARTICULATION_PROBE,
            },
        }
    };
}

pub(super) const TUBE_ARTICULATION_SLOT_CASES: [CatalogCase; 8] = [
    slot_case!(
        0,
        "tube_articulation_slot_tongue",
        "Tube Articulation Slot Tongue",
        "09_articulation/tube_articulation_slot_tongue.wav",
        "tongue"
    ),
    slot_case!(
        1,
        "tube_articulation_slot_sforzando",
        "Tube Articulation Slot Sforzando",
        "09_articulation/tube_articulation_slot_sforzando.wav",
        "sforzando"
    ),
    slot_case!(
        2,
        "tube_articulation_slot_legato",
        "Tube Articulation Slot Legato",
        "09_articulation/tube_articulation_slot_legato.wav",
        "legato"
    ),
    slot_case!(
        3,
        "tube_articulation_slot_staccato",
        "Tube Articulation Slot Staccato",
        "09_articulation/tube_articulation_slot_staccato.wav",
        "staccato"
    ),
    slot_case!(
        4,
        "tube_articulation_slot_marcato",
        "Tube Articulation Slot Marcato",
        "09_articulation/tube_articulation_slot_marcato.wav",
        "marcato"
    ),
    slot_case!(
        5,
        "tube_articulation_slot_breath",
        "Tube Articulation Slot Breath",
        "09_articulation/tube_articulation_slot_breath.wav",
        "breath"
    ),
    slot_case!(
        6,
        "tube_articulation_slot_accent",
        "Tube Articulation Slot Accent",
        "09_articulation/tube_articulation_slot_accent.wav",
        "accent"
    ),
    slot_case!(
        7,
        "tube_articulation_slot_slur",
        "Tube Articulation Slot Slur",
        "09_articulation/tube_articulation_slot_slur.wav",
        "slur"
    ),
];
