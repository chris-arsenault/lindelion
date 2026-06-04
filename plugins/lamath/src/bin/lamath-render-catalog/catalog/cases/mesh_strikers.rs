//! Mesh striker cases. These audition the contact impulse bank against the same resonator body:
//! single hits for direct A/B, repeated hits for feedback/build behavior, crash builds, and
//! overlap phrases on the older gong-style body settings.

use super::super::{
    CatalogCase, MeshStriker, MeshVoicing, PatchRecipe, RenderSchedule, ScheduledNote,
};

const SINGLE_DURATION_SECONDS: f32 = 3.4;
const REPEATED_DURATION_SECONDS: f32 = 4.2;
const PHRASE_DURATION_SECONDS: f32 = 4.8;

const V45: f32 = 45.0 / 127.0;
const V65: f32 = 65.0 / 127.0;
const V80: f32 = 80.0 / 127.0;
const V100: f32 = 100.0 / 127.0;
const V112: f32 = 112.0 / 127.0;
const V127: f32 = 1.0;

const SINGLE_C4: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.00,
    end_seconds: 0.50,
    note: 60,
    velocity: V100,
}];

const REPEATED_C4: [ScheduledNote; 12] = [
    repeated_hit(0.00, V100),
    repeated_hit(0.16, V100),
    repeated_hit(0.32, V100),
    repeated_hit(0.48, V100),
    repeated_hit(0.64, V100),
    repeated_hit(0.80, V100),
    repeated_hit(0.96, V100),
    repeated_hit(1.12, V100),
    repeated_hit(1.28, V100),
    repeated_hit(1.44, V100),
    repeated_hit(1.60, V100),
    ScheduledNote {
        start_seconds: 1.76,
        end_seconds: 3.75,
        note: 60,
        velocity: V100,
    },
];

const CRASH_BUILD_C4: [ScheduledNote; 10] = [
    repeated_hit(0.00, V45),
    repeated_hit(0.22, V65),
    repeated_hit(0.44, V80),
    repeated_hit(0.66, V100),
    repeated_hit(0.88, V112),
    repeated_hit(1.10, V127),
    repeated_hit(1.32, V127),
    repeated_hit(1.54, V127),
    repeated_hit(1.76, V127),
    ScheduledNote {
        start_seconds: 1.98,
        end_seconds: 3.95,
        note: 60,
        velocity: V127,
    },
];

const BRUSH_GHOSTS: [ScheduledNote; 16] = [
    brush_hit(0.00, 60, V45),
    brush_hit(0.12, 60, V65),
    brush_hit(0.24, 62, V45),
    brush_hit(0.36, 60, V80),
    brush_hit(0.48, 64, V45),
    brush_hit(0.60, 60, V65),
    brush_hit(0.72, 62, V45),
    brush_hit(0.84, 60, V100),
    brush_hit(1.02, 67, V45),
    brush_hit(1.14, 60, V65),
    brush_hit(1.26, 64, V45),
    brush_hit(1.38, 60, V80),
    brush_hit(1.50, 62, V45),
    brush_hit(1.62, 60, V65),
    brush_hit(1.74, 67, V45),
    ScheduledNote {
        start_seconds: 1.86,
        end_seconds: 3.75,
        note: 60,
        velocity: V100,
    },
];

const OVERLAP_SCALE: [ScheduledNote; 8] = [
    phrase_note(0.00, 0.42, 60, V100),
    phrase_note(0.30, 0.72, 62, V100),
    phrase_note(0.60, 1.02, 64, V100),
    phrase_note(0.90, 1.32, 65, V100),
    phrase_note(1.20, 1.62, 67, V100),
    phrase_note(1.50, 1.92, 69, V100),
    phrase_note(1.80, 2.22, 71, V100),
    ScheduledNote {
        start_seconds: 2.10,
        end_seconds: 4.30,
        note: 72,
        velocity: V100,
    },
];

const fn repeated_hit(start_seconds: f32, velocity: f32) -> ScheduledNote {
    ScheduledNote {
        start_seconds,
        end_seconds: start_seconds + 0.08,
        note: 60,
        velocity,
    }
}

const fn brush_hit(start_seconds: f32, note: u8, velocity: f32) -> ScheduledNote {
    ScheduledNote {
        start_seconds,
        end_seconds: start_seconds + 0.06,
        note,
        velocity,
    }
}

const fn phrase_note(
    start_seconds: f32,
    end_seconds: f32,
    note: u8,
    velocity: f32,
) -> ScheduledNote {
    ScheduledNote {
        start_seconds,
        end_seconds,
        note,
        velocity,
    }
}

macro_rules! mesh_striker_case {
    ($id:literal, $title:literal, $wav:literal, [$($tag:literal),+], $voicing:expr, $striker:expr, $notes:ident, $duration:expr) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "mesh_strikers",
            relative_wav: $wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::MeshStriker {
                voicing: $voicing,
                striker: $striker,
            },
            schedule: RenderSchedule {
                duration_seconds: $duration,
                notes: &$notes,
            },
        }
    };
}

pub(super) const MESH_STRIKER_CASES: [CatalogCase; 12] = [
    mesh_striker_case!(
        "mesh_striker_kit_ride_hard_stick_single",
        "Mesh Kit Ride Hard Stick Single",
        "13_mesh_strikers/mesh_striker_kit_ride_hard_stick_single.wav",
        ["mesh", "striker", "kit", "ride", "hard-stick", "single"],
        MeshVoicing::KitRide,
        MeshStriker::HardStick,
        SINGLE_C4,
        SINGLE_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_soft_mallet_single",
        "Mesh Kit Ride Soft Mallet Single",
        "13_mesh_strikers/mesh_striker_kit_ride_soft_mallet_single.wav",
        ["mesh", "striker", "kit", "ride", "soft-mallet", "single"],
        MeshVoicing::KitRide,
        MeshStriker::SoftMallet,
        SINGLE_C4,
        SINGLE_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_jazz_brush_single",
        "Mesh Kit Ride Jazz Brush Single",
        "13_mesh_strikers/mesh_striker_kit_ride_jazz_brush_single.wav",
        ["mesh", "striker", "kit", "ride", "jazz-brush", "single"],
        MeshVoicing::KitRide,
        MeshStriker::JazzBrush,
        SINGLE_C4,
        SINGLE_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_bell_stick_single",
        "Mesh Kit Ride Bell Stick Single",
        "13_mesh_strikers/mesh_striker_kit_ride_bell_stick_single.wav",
        ["mesh", "striker", "kit", "ride", "bell-stick", "single"],
        MeshVoicing::KitRide,
        MeshStriker::BellStick,
        SINGLE_C4,
        SINGLE_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_hard_stick_repeated",
        "Mesh Kit Ride Hard Stick Repeated",
        "13_mesh_strikers/mesh_striker_kit_ride_hard_stick_repeated.wav",
        ["mesh", "striker", "kit", "ride", "hard-stick", "repeated"],
        MeshVoicing::KitRide,
        MeshStriker::HardStick,
        REPEATED_C4,
        REPEATED_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_soft_mallet_repeated",
        "Mesh Kit Ride Soft Mallet Repeated",
        "13_mesh_strikers/mesh_striker_kit_ride_soft_mallet_repeated.wav",
        ["mesh", "striker", "kit", "ride", "soft-mallet", "repeated"],
        MeshVoicing::KitRide,
        MeshStriker::SoftMallet,
        REPEATED_C4,
        REPEATED_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_jazz_brush_ghosts",
        "Mesh Kit Ride Jazz Brush Ghost Notes",
        "13_mesh_strikers/mesh_striker_kit_ride_jazz_brush_ghosts.wav",
        [
            "mesh",
            "striker",
            "kit",
            "ride",
            "jazz-brush",
            "ghosts",
            "repeated"
        ],
        MeshVoicing::KitRide,
        MeshStriker::JazzBrush,
        BRUSH_GHOSTS,
        REPEATED_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_ride_bell_stick_repeated",
        "Mesh Kit Ride Bell Stick Repeated",
        "13_mesh_strikers/mesh_striker_kit_ride_bell_stick_repeated.wav",
        ["mesh", "striker", "kit", "ride", "bell-stick", "repeated"],
        MeshVoicing::KitRide,
        MeshStriker::BellStick,
        REPEATED_C4,
        REPEATED_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_crash_hard_stick_build",
        "Mesh Kit Crash Hard Stick Build",
        "13_mesh_strikers/mesh_striker_kit_crash_hard_stick_build.wav",
        [
            "mesh",
            "striker",
            "kit",
            "crash",
            "hard-stick",
            "build",
            "repeated"
        ],
        MeshVoicing::KitCrash,
        MeshStriker::HardStick,
        CRASH_BUILD_C4,
        REPEATED_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_kit_crash_soft_mallet_build",
        "Mesh Kit Crash Soft Mallet Build",
        "13_mesh_strikers/mesh_striker_kit_crash_soft_mallet_build.wav",
        [
            "mesh",
            "striker",
            "kit",
            "crash",
            "soft-mallet",
            "build",
            "repeated"
        ],
        MeshVoicing::KitCrash,
        MeshStriker::SoftMallet,
        CRASH_BUILD_C4,
        REPEATED_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_gong_ride_soft_mallet_overlap",
        "Mesh Gong Ride Soft Mallet Overlap",
        "13_mesh_strikers/mesh_striker_gong_ride_soft_mallet_overlap.wav",
        ["mesh", "striker", "gong", "ride", "soft-mallet", "overlap"],
        MeshVoicing::Ride,
        MeshStriker::SoftMallet,
        OVERLAP_SCALE,
        PHRASE_DURATION_SECONDS
    ),
    mesh_striker_case!(
        "mesh_striker_gong_crash_jazz_brush_overlap",
        "Mesh Gong Crash Jazz Brush Overlap",
        "13_mesh_strikers/mesh_striker_gong_crash_jazz_brush_overlap.wav",
        ["mesh", "striker", "gong", "crash", "jazz-brush", "overlap"],
        MeshVoicing::Crash,
        MeshStriker::JazzBrush,
        OVERLAP_SCALE,
        PHRASE_DURATION_SECONDS
    ),
];
