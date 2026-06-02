use super::super::{CatalogCase, EdgeRecipe, PatchRecipe, RenderSchedule, ScheduledNote};

const EDGE_DURATION_SECONDS: f32 = 3.5;

const C4_V127: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 60,
    velocity: 1.0,
}];

const C2_V127: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 36,
    velocity: 1.0,
}];

const DENSE_HARD_CHORD_V127: [ScheduledNote; 16] = [
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 48,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 49,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 50,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 51,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 52,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 53,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 54,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 55,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 56,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 57,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 58,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 59,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 60,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 61,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 62,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.0,
        end_seconds: 1.2,
        note: 63,
        velocity: 1.0,
    },
];

macro_rules! edge_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $recipe:ident, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "edges",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::Edge(EdgeRecipe::$recipe),
            schedule: RenderSchedule {
                duration_seconds: EDGE_DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const EDGE_CASES: [CatalogCase; 8] = [
    edge_case!(
        "edge_string_high_loop_gain_c4_v127",
        "Edge String High Loop Gain C4 Velocity 127",
        "08_edges/edge_string_high_loop_gain_c4_v127.wav",
        ["edge", "string", "high-loop-gain", "C4", "velocity-127"],
        StringHighLoopGain,
        C4_V127
    ),
    edge_case!(
        "edge_string_high_dispersion_c4_v127",
        "Edge String High Dispersion C4 Velocity 127",
        "08_edges/edge_string_high_dispersion_c4_v127.wav",
        ["edge", "string", "high-dispersion", "C4", "velocity-127"],
        StringHighDispersion,
        C4_V127
    ),
    edge_case!(
        "edge_string_source_body_low_c2_v127",
        "Edge String Source Body Low C2 Velocity 127",
        "08_edges/edge_string_source_body_low_c2_v127.wav",
        ["edge", "string", "source-body-low", "C2", "velocity-127"],
        StringSourceBodyLow,
        C2_V127
    ),
    edge_case!(
        "edge_tube_closed_nonlinear_c4_v127",
        "Edge Tube Closed Nonlinear C4 Velocity 127",
        "08_edges/edge_tube_closed_nonlinear_c4_v127.wav",
        ["edge", "tube", "closed", "nonlinear", "C4", "velocity-127"],
        TubeClosedNonlinear,
        C4_V127
    ),
    edge_case!(
        "edge_tube_open_nonlinear_c4_v127",
        "Edge Tube Open Nonlinear C4 Velocity 127",
        "08_edges/edge_tube_open_nonlinear_c4_v127.wav",
        ["edge", "tube", "open", "nonlinear", "C4", "velocity-127"],
        TubeOpenNonlinear,
        C4_V127
    ),
    edge_case!(
        "edge_mesh_low_damping_high_material_c4_v127",
        "Edge Mesh Low Damping High Material C4 Velocity 127",
        "08_edges/edge_mesh_low_damping_high_material_c4_v127.wav",
        [
            "edge",
            "mesh",
            "low-damping",
            "high-material",
            "C4",
            "velocity-127"
        ],
        MeshLowDampingHighMaterial,
        C4_V127
    ),
    edge_case!(
        "edge_modal_bright_long_decay_c4_v127",
        "Edge Modal Bright Long Decay C4 Velocity 127",
        "08_edges/edge_modal_bright_long_decay_c4_v127.wav",
        [
            "edge",
            "modal",
            "bright",
            "long-decay",
            "C4",
            "velocity-127"
        ],
        ModalBrightLongDecay,
        C4_V127
    ),
    edge_case!(
        "edge_string_dense_hard_chord_v127",
        "Edge String Dense Hard Chord Velocity 127",
        "08_edges/edge_string_dense_hard_chord_v127.wav",
        ["edge", "string", "dense-hard-chord", "velocity-127"],
        StringDenseHardChord,
        DENSE_HARD_CHORD_V127
    ),
];
