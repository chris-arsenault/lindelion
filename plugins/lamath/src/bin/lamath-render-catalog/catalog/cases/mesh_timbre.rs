//! Mesh timbre cases: a single struck note auditioned across the Mesh's timbre controls.
//! The Mesh is a fixed-pitch struck idiophone whose character comes from grid cell count
//! (density), boundary damping (decay), and edge/strike condition — not from pitch. These
//! sweep that space: three character presets (triangle → ride → crash) plus single-axis
//! sweeps so density and decay can each be heard in isolation. Auditioned by ear.

use super::super::{CatalogCase, MeshVoicing, PatchRecipe, RenderSchedule, ScheduledNote};

const STRIKE_DURATION_SECONDS: f32 = 3.5;
const V100: f32 = 100.0 / 127.0;

/// One strike, held so the full decay tail rings out.
const SINGLE_STRIKE: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 3.0,
    note: 60,
    velocity: V100,
}];

macro_rules! mesh_timbre_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $voicing:expr) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "mesh_timbre",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::MeshVoicing($voicing),
            schedule: RenderSchedule {
                duration_seconds: STRIKE_DURATION_SECONDS,
                notes: &SINGLE_STRIKE,
            },
        }
    };
}

pub(super) const MESH_TIMBRE_CASES: [CatalogCase; 9] = [
    mesh_timbre_case!(
        "mesh_timbre_triangle",
        "Mesh Timbre Triangle",
        "10_mesh_timbre/mesh_timbre_triangle.wav",
        ["mesh", "timbre", "triangle", "sparse"],
        MeshVoicing::Triangle
    ),
    mesh_timbre_case!(
        "mesh_timbre_ride",
        "Mesh Timbre Ride Gong",
        "10_mesh_timbre/mesh_timbre_ride.wav",
        ["mesh", "timbre", "ride", "gong", "ab"],
        MeshVoicing::Ride
    ),
    mesh_timbre_case!(
        "mesh_timbre_kit_ride",
        "Mesh Timbre Kit Ride",
        "10_mesh_timbre/mesh_timbre_kit_ride.wav",
        ["mesh", "timbre", "ride", "kit", "hard-contact", "ab"],
        MeshVoicing::KitRide
    ),
    mesh_timbre_case!(
        "mesh_timbre_crash",
        "Mesh Timbre Crash Gong",
        "10_mesh_timbre/mesh_timbre_crash.wav",
        ["mesh", "timbre", "crash", "dense", "gong", "ab"],
        MeshVoicing::Crash
    ),
    mesh_timbre_case!(
        "mesh_timbre_kit_crash",
        "Mesh Timbre Kit Crash",
        "10_mesh_timbre/mesh_timbre_kit_crash.wav",
        ["mesh", "timbre", "crash", "kit", "hard-contact", "ab"],
        MeshVoicing::KitCrash
    ),
    mesh_timbre_case!(
        "mesh_timbre_density_sparse",
        "Mesh Timbre Density Sparse",
        "10_mesh_timbre/mesh_timbre_density_sparse.wav",
        ["mesh", "timbre", "density", "sparse"],
        MeshVoicing::DensitySparse
    ),
    mesh_timbre_case!(
        "mesh_timbre_density_dense",
        "Mesh Timbre Density Dense",
        "10_mesh_timbre/mesh_timbre_density_dense.wav",
        ["mesh", "timbre", "density", "dense"],
        MeshVoicing::DensityDense
    ),
    mesh_timbre_case!(
        "mesh_timbre_decay_short",
        "Mesh Timbre Decay Short",
        "10_mesh_timbre/mesh_timbre_decay_short.wav",
        ["mesh", "timbre", "decay", "short"],
        MeshVoicing::DecayShort
    ),
    mesh_timbre_case!(
        "mesh_timbre_decay_long",
        "Mesh Timbre Decay Long",
        "10_mesh_timbre/mesh_timbre_decay_long.wav",
        ["mesh", "timbre", "decay", "long"],
        MeshVoicing::DecayLong
    ),
];
