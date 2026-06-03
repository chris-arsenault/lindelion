use super::{CatalogCase, PatchRecipe, RenderSchedule, ResonatorFamily, ScheduledNote};

mod articulation;
mod chords;
mod dynamic_response;
mod edges;
mod mesh_timbre;
mod surrounding;
mod tube_dynamics;

const SINGLE_NOTE_DURATION_SECONDS: f32 = 2.5;

const C2_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 36,
    velocity: 100.0 / 127.0,
}];

const C4_V020: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 60,
    velocity: 20.0 / 127.0,
}];

const C4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 60,
    velocity: 100.0 / 127.0,
}];

const C4_V127: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 60,
    velocity: 1.0,
}];

const C6_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 84,
    velocity: 100.0 / 127.0,
}];

#[derive(Debug, Clone, Copy)]
struct CatalogCaseSpec {
    id: &'static str,
    title: &'static str,
    group_id: &'static str,
    relative_wav: &'static str,
    tags: &'static [&'static str],
    patch_recipe: PatchRecipe,
    note: u8,
    velocity: u8,
}

macro_rules! case_spec {
    ($id:literal, $title:literal, $group_id:literal, $relative_wav:literal, [$($tag:literal),+], $family:ident, $note:literal, $velocity:literal) => {
        CatalogCaseSpec {
            id: $id,
            title: $title,
            group_id: $group_id,
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::SingleFamily(ResonatorFamily::$family),
            note: $note,
            velocity: $velocity,
        }
    };
}

const BASELINE_CASE_SPECS: [CatalogCaseSpec; 12] = [
    case_spec!(
        "baseline_modal_c4_v020",
        "Baseline Modal C4 Velocity 20",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_modal_c4_v020.wav",
        ["baseline", "dynamics", "modal", "C4", "velocity-20"],
        Modal,
        60,
        20
    ),
    case_spec!(
        "baseline_modal_c4_v100",
        "Baseline Modal C4 Velocity 100",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_modal_c4_v100.wav",
        ["baseline", "dynamics", "modal", "C4", "velocity-100"],
        Modal,
        60,
        100
    ),
    case_spec!(
        "baseline_modal_c4_v127",
        "Baseline Modal C4 Velocity 127",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_modal_c4_v127.wav",
        ["baseline", "dynamics", "modal", "C4", "velocity-127"],
        Modal,
        60,
        127
    ),
    case_spec!(
        "baseline_string_c4_v020",
        "Baseline String C4 Velocity 20",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_string_c4_v020.wav",
        ["baseline", "dynamics", "string", "C4", "velocity-20"],
        String,
        60,
        20
    ),
    case_spec!(
        "baseline_string_c4_v100",
        "Baseline String C4 Velocity 100",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_string_c4_v100.wav",
        ["baseline", "dynamics", "string", "C4", "velocity-100"],
        String,
        60,
        100
    ),
    case_spec!(
        "baseline_string_c4_v127",
        "Baseline String C4 Velocity 127",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_string_c4_v127.wav",
        ["baseline", "dynamics", "string", "C4", "velocity-127"],
        String,
        60,
        127
    ),
    case_spec!(
        "baseline_tube_c4_v020",
        "Baseline Tube C4 Velocity 20",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_tube_c4_v020.wav",
        ["baseline", "dynamics", "tube", "C4", "velocity-20"],
        Tube,
        60,
        20
    ),
    case_spec!(
        "baseline_tube_c4_v100",
        "Baseline Tube C4 Velocity 100",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_tube_c4_v100.wav",
        ["baseline", "dynamics", "tube", "C4", "velocity-100"],
        Tube,
        60,
        100
    ),
    case_spec!(
        "baseline_tube_c4_v127",
        "Baseline Tube C4 Velocity 127",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_tube_c4_v127.wav",
        ["baseline", "dynamics", "tube", "C4", "velocity-127"],
        Tube,
        60,
        127
    ),
    case_spec!(
        "baseline_mesh_c4_v020",
        "Baseline Mesh C4 Velocity 20",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_mesh_c4_v020.wav",
        ["baseline", "dynamics", "mesh", "C4", "velocity-20"],
        Mesh,
        60,
        20
    ),
    case_spec!(
        "baseline_mesh_c4_v100",
        "Baseline Mesh C4 Velocity 100",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_mesh_c4_v100.wav",
        ["baseline", "dynamics", "mesh", "C4", "velocity-100"],
        Mesh,
        60,
        100
    ),
    case_spec!(
        "baseline_mesh_c4_v127",
        "Baseline Mesh C4 Velocity 127",
        "baseline_dynamics",
        "01_baseline_dynamics/baseline_mesh_c4_v127.wav",
        ["baseline", "dynamics", "mesh", "C4", "velocity-127"],
        Mesh,
        60,
        127
    ),
];

const REGISTER_CASE_SPECS: [CatalogCaseSpec; 12] = [
    case_spec!(
        "register_modal_c2_v100",
        "Register Modal C2 Velocity 100",
        "register_range",
        "02_register_range/register_modal_c2_v100.wav",
        ["register", "range", "modal", "C2", "velocity-100"],
        Modal,
        36,
        100
    ),
    case_spec!(
        "register_modal_c4_v100",
        "Register Modal C4 Velocity 100",
        "register_range",
        "02_register_range/register_modal_c4_v100.wav",
        ["register", "range", "modal", "C4", "velocity-100"],
        Modal,
        60,
        100
    ),
    case_spec!(
        "register_modal_c6_v100",
        "Register Modal C6 Velocity 100",
        "register_range",
        "02_register_range/register_modal_c6_v100.wav",
        ["register", "range", "modal", "C6", "velocity-100"],
        Modal,
        84,
        100
    ),
    case_spec!(
        "register_string_c2_v100",
        "Register String C2 Velocity 100",
        "register_range",
        "02_register_range/register_string_c2_v100.wav",
        ["register", "range", "string", "C2", "velocity-100"],
        String,
        36,
        100
    ),
    case_spec!(
        "register_string_c4_v100",
        "Register String C4 Velocity 100",
        "register_range",
        "02_register_range/register_string_c4_v100.wav",
        ["register", "range", "string", "C4", "velocity-100"],
        String,
        60,
        100
    ),
    case_spec!(
        "register_string_c6_v100",
        "Register String C6 Velocity 100",
        "register_range",
        "02_register_range/register_string_c6_v100.wav",
        ["register", "range", "string", "C6", "velocity-100"],
        String,
        84,
        100
    ),
    case_spec!(
        "register_tube_c2_v100",
        "Register Tube C2 Velocity 100",
        "register_range",
        "02_register_range/register_tube_c2_v100.wav",
        ["register", "range", "tube", "C2", "velocity-100"],
        Tube,
        36,
        100
    ),
    case_spec!(
        "register_tube_c4_v100",
        "Register Tube C4 Velocity 100",
        "register_range",
        "02_register_range/register_tube_c4_v100.wav",
        ["register", "range", "tube", "C4", "velocity-100"],
        Tube,
        60,
        100
    ),
    case_spec!(
        "register_tube_c6_v100",
        "Register Tube C6 Velocity 100",
        "register_range",
        "02_register_range/register_tube_c6_v100.wav",
        ["register", "range", "tube", "C6", "velocity-100"],
        Tube,
        84,
        100
    ),
    case_spec!(
        "register_mesh_c2_v100",
        "Register Mesh C2 Velocity 100",
        "register_range",
        "02_register_range/register_mesh_c2_v100.wav",
        ["register", "range", "mesh", "C2", "velocity-100"],
        Mesh,
        36,
        100
    ),
    case_spec!(
        "register_mesh_c4_v100",
        "Register Mesh C4 Velocity 100",
        "register_range",
        "02_register_range/register_mesh_c4_v100.wav",
        ["register", "range", "mesh", "C4", "velocity-100"],
        Mesh,
        60,
        100
    ),
    case_spec!(
        "register_mesh_c6_v100",
        "Register Mesh C6 Velocity 100",
        "register_range",
        "02_register_range/register_mesh_c6_v100.wav",
        ["register", "range", "mesh", "C6", "velocity-100"],
        Mesh,
        84,
        100
    ),
];

pub(crate) fn catalog_cases() -> Vec<CatalogCase> {
    BASELINE_CASE_SPECS
        .iter()
        .chain(REGISTER_CASE_SPECS.iter())
        .chain(dynamic_response::DRIVER_CASE_SPECS.iter())
        .chain(dynamic_response::CONTACT_CASE_SPECS.iter())
        .chain(dynamic_response::SOURCE_BODY_CASE_SPECS.iter())
        .copied()
        .map(CatalogCaseSpec::catalog_case)
        .chain(surrounding::SURROUNDING_CASES.iter().cloned())
        .chain(chords::CHORD_CASES.iter().cloned())
        .chain(edges::EDGE_CASES.iter().cloned())
        .chain(articulation::ARTICULATION_CASES.iter().cloned())
        .chain(mesh_timbre::MESH_TIMBRE_CASES.iter().cloned())
        .chain(tube_dynamics::TUBE_DYNAMICS_CASES.iter().cloned())
        .collect()
}

fn single_note_schedule(note: u8, velocity: u8) -> &'static [ScheduledNote] {
    match (note, velocity) {
        (36, 100) => &C2_V100,
        (60, 20) => &C4_V020,
        (60, 100) => &C4_V100,
        (60, 127) => &C4_V127,
        (84, 100) => &C6_V100,
        _ => unreachable!(
            "unsupported single-note catalog schedule: note {note} velocity {velocity}"
        ),
    }
}

impl CatalogCaseSpec {
    fn catalog_case(self) -> CatalogCase {
        CatalogCase {
            id: self.id,
            title: self.title,
            group_id: self.group_id,
            relative_wav: self.relative_wav,
            tags: self.tags,
            patch_recipe: self.patch_recipe,
            schedule: RenderSchedule {
                duration_seconds: SINGLE_NOTE_DURATION_SECONDS,
                notes: single_note_schedule(self.note, self.velocity),
            },
        }
    }
}
