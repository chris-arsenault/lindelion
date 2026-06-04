use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ResonatorFamily, ScheduledNote, SurroundingRecipe,
};

const SURROUNDING_DURATION_SECONDS: f32 = 3.0;
const C4_V127: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 0.6,
    note: 60,
    velocity: 1.0,
}];

macro_rules! surrounding_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $recipe:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "surrounding",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::Surrounding {
                family: ResonatorFamily::Modal,
                surrounding: SurroundingRecipe::$recipe,
            },
            schedule: RenderSchedule {
                duration_seconds: SURROUNDING_DURATION_SECONDS,
                notes: &C4_V127,
            },
        }
    };
}

pub(super) const SURROUNDING_CASES: [CatalogCase; 8] = [
    surrounding_case!(
        "surrounding_modal_off_c4_v127",
        "Surrounding Modal Off C4 Velocity 127",
        "06_surrounding/surrounding_modal_off_c4_v127.wav",
        ["surrounding", "modal", "off", "C4", "velocity-127"],
        Off
    ),
    surrounding_case!(
        "surrounding_modal_mechanical_c4_v127",
        "Surrounding Modal Mechanical C4 Velocity 127",
        "06_surrounding/surrounding_modal_mechanical_c4_v127.wav",
        ["surrounding", "modal", "mechanical", "C4", "velocity-127"],
        Mechanical
    ),
    surrounding_case!(
        "surrounding_modal_radiation_c4_v127",
        "Surrounding Modal Radiation C4 Velocity 127",
        "06_surrounding/surrounding_modal_radiation_c4_v127.wav",
        ["surrounding", "modal", "radiation", "C4", "velocity-127"],
        Radiation
    ),
    surrounding_case!(
        "surrounding_modal_sympathetic_c4_v127",
        "Surrounding Modal Sympathetic C4 Velocity 127",
        "06_surrounding/surrounding_modal_sympathetic_c4_v127.wav",
        ["surrounding", "modal", "sympathetic", "C4", "velocity-127"],
        Sympathetic
    ),
    surrounding_case!(
        "surrounding_modal_mechanical_radiation_c4_v127",
        "Surrounding Modal Mechanical Radiation C4 Velocity 127",
        "06_surrounding/surrounding_modal_mechanical_radiation_c4_v127.wav",
        [
            "surrounding",
            "modal",
            "mechanical",
            "radiation",
            "C4",
            "velocity-127"
        ],
        MechanicalRadiation
    ),
    surrounding_case!(
        "surrounding_modal_mechanical_sympathetic_c4_v127",
        "Surrounding Modal Mechanical Sympathetic C4 Velocity 127",
        "06_surrounding/surrounding_modal_mechanical_sympathetic_c4_v127.wav",
        [
            "surrounding",
            "modal",
            "mechanical",
            "sympathetic",
            "C4",
            "velocity-127"
        ],
        MechanicalSympathetic
    ),
    surrounding_case!(
        "surrounding_modal_radiation_sympathetic_c4_v127",
        "Surrounding Modal Radiation Sympathetic C4 Velocity 127",
        "06_surrounding/surrounding_modal_radiation_sympathetic_c4_v127.wav",
        [
            "surrounding",
            "modal",
            "radiation",
            "sympathetic",
            "C4",
            "velocity-127"
        ],
        RadiationSympathetic
    ),
    surrounding_case!(
        "surrounding_modal_all_c4_v127",
        "Surrounding Modal All C4 Velocity 127",
        "06_surrounding/surrounding_modal_all_c4_v127.wav",
        ["surrounding", "modal", "all", "C4", "velocity-127"],
        All
    ),
];
