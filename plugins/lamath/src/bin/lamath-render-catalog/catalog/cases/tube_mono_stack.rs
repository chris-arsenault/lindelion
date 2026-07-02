//! Tube monophonic-stack and register-break gesture cases: the trill-over-a-held-key return
//! (note stack) in and across the break, and a legato run up and down over the break (the dip
//! transition), at the shipped default patch.

use super::super::{CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote};

const TRILL_DURATION_SECONDS: f32 = 3.4;
const RUN_DURATION_SECONDS: f32 = 4.2;
const V100: f32 = 100.0 / 127.0;

/// G3 held throughout; A3 trilled on top three times. Each A3 release must fall back to the
/// still-held G3 (finger-lift slur), not to silence.
const LOW_TRILL: [ScheduledNote; 4] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 3.00,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.70,
        end_seconds: 1.00,
        note: 57,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.40,
        end_seconds: 1.70,
        note: 57,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.10,
        end_seconds: 2.40,
        note: 57,
        velocity: V100,
    },
];

/// G4 held below the break; B4 trilled on top — every change crosses the break, so each leg
/// passes through the dip transition and must land back on the held G4.
const BREAK_TRILL: [ScheduledNote; 3] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 3.00,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.80,
        end_seconds: 1.20,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.90,
        end_seconds: 2.30,
        note: 71,
        velocity: V100,
    },
];

/// Connected run E4 -> C5 -> E4 over the break: each overlapping change near A4 exercises the
/// register-break swell in both directions.
const BREAK_RUN: [ScheduledNote; 9] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.45,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.38,
        end_seconds: 0.83,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.76,
        end_seconds: 1.21,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.14,
        end_seconds: 1.59,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.52,
        end_seconds: 2.10,
        note: 72,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.03,
        end_seconds: 2.48,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.41,
        end_seconds: 2.86,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.79,
        end_seconds: 3.24,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.17,
        end_seconds: 3.80,
        note: 64,
        velocity: V100,
    },
];

pub(super) const TUBE_MONO_STACK_CASES: [CatalogCase; 3] = [
    CatalogCase {
        id: "tube_mono_trill_low",
        title: "Tube Mono Stack Trill Over Held G3",
        group_id: "tube_mono_stack",
        relative_wav: "22_tube_mono_stack/tube_mono_trill_low.wav",
        tags: &["tube", "mono-stack", "trill", "low-register"],
        patch_recipe: PatchRecipe::TubeArticulationPhrase { slot: 0 },
        schedule: RenderSchedule {
            duration_seconds: TRILL_DURATION_SECONDS,
            notes: &LOW_TRILL,
        },
    },
    CatalogCase {
        id: "tube_mono_trill_break",
        title: "Tube Mono Stack Trill Across The Break",
        group_id: "tube_mono_stack",
        relative_wav: "22_tube_mono_stack/tube_mono_trill_break.wav",
        tags: &["tube", "mono-stack", "trill", "register-break"],
        patch_recipe: PatchRecipe::TubeArticulationPhrase { slot: 0 },
        schedule: RenderSchedule {
            duration_seconds: TRILL_DURATION_SECONDS,
            notes: &BREAK_TRILL,
        },
    },
    CatalogCase {
        id: "tube_break_legato_run",
        title: "Tube Legato Run Over The Register Break",
        group_id: "tube_mono_stack",
        relative_wav: "22_tube_mono_stack/tube_break_legato_run.wav",
        tags: &["tube", "legato", "register-break", "run"],
        patch_recipe: PatchRecipe::TubeArticulationPhrase { slot: 0 },
        schedule: RenderSchedule {
            duration_seconds: RUN_DURATION_SECONDS,
            notes: &BREAK_RUN,
        },
    },
];
