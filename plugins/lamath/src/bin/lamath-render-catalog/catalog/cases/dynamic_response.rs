use super::super::{ContactRecipe, DriverRecipe, PatchRecipe, ResonatorFamily, SourceBodyDepth};
use super::CatalogCaseSpec;

macro_rules! driver_case_spec {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $family:ident, $driver:ident) => {
        CatalogCaseSpec {
            id: $id,
            title: $title,
            group_id: "drivers",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::Driver {
                family: ResonatorFamily::$family,
                driver: DriverRecipe::$driver,
            },
            note: 60,
            velocity: 100,
        }
    };
}

macro_rules! contact_case_spec {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $family:ident, $contact:ident) => {
        CatalogCaseSpec {
            id: $id,
            title: $title,
            group_id: "contact",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::Contact {
                family: ResonatorFamily::$family,
                contact: ContactRecipe::$contact,
            },
            note: 60,
            velocity: 100,
        }
    };
}

macro_rules! source_body_case_spec {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $depth:ident, $velocity:literal) => {
        CatalogCaseSpec {
            id: $id,
            title: $title,
            group_id: "source_body_balance",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::SourceBodyBalance {
                depth: SourceBodyDepth::$depth,
            },
            note: 60,
            velocity: $velocity,
        }
    };
}

pub(super) const DRIVER_CASE_SPECS: [CatalogCaseSpec; 8] = [
    driver_case_spec!(
        "driver_string_sample_c4_v100",
        "Driver String Sample C4 Velocity 100",
        "03_drivers/driver_string_sample_c4_v100.wav",
        ["drivers", "string", "sample", "C4", "velocity-100"],
        String,
        Sample
    ),
    driver_case_spec!(
        "driver_string_pick_soft_c4_v100",
        "Driver String Pick Soft C4 Velocity 100",
        "03_drivers/driver_string_pick_soft_c4_v100.wav",
        ["drivers", "string", "pick-soft", "C4", "velocity-100"],
        String,
        PickSoft
    ),
    driver_case_spec!(
        "driver_string_pick_hard_c4_v100",
        "Driver String Pick Hard C4 Velocity 100",
        "03_drivers/driver_string_pick_hard_c4_v100.wav",
        ["drivers", "string", "pick-hard", "C4", "velocity-100"],
        String,
        PickHard
    ),
    driver_case_spec!(
        "driver_string_bow_smooth_c4_v100",
        "Driver String Bow Smooth C4 Velocity 100",
        "03_drivers/driver_string_bow_smooth_c4_v100.wav",
        ["drivers", "string", "bow-smooth", "C4", "velocity-100"],
        String,
        BowSmooth
    ),
    driver_case_spec!(
        "driver_string_bow_scratch_c4_v100",
        "Driver String Bow Scratch C4 Velocity 100",
        "03_drivers/driver_string_bow_scratch_c4_v100.wav",
        ["drivers", "string", "bow-scratch", "C4", "velocity-100"],
        String,
        BowScratch
    ),
    driver_case_spec!(
        "driver_tube_sample_c4_v100",
        "Driver Tube Sample C4 Velocity 100",
        "03_drivers/driver_tube_sample_c4_v100.wav",
        ["drivers", "tube", "sample", "C4", "velocity-100"],
        Tube,
        Sample
    ),
    driver_case_spec!(
        "driver_tube_reed_soft_c4_v100",
        "Driver Tube Reed Soft C4 Velocity 100",
        "03_drivers/driver_tube_reed_soft_c4_v100.wav",
        ["drivers", "tube", "reed-soft", "C4", "velocity-100"],
        Tube,
        ReedSoft
    ),
    driver_case_spec!(
        "driver_tube_reed_hard_c4_v100",
        "Driver Tube Reed Hard C4 Velocity 100",
        "03_drivers/driver_tube_reed_hard_c4_v100.wav",
        ["drivers", "tube", "reed-hard", "C4", "velocity-100"],
        Tube,
        ReedHard
    ),
];

pub(super) const CONTACT_CASE_SPECS: [CatalogCaseSpec; 8] = [
    contact_case_spec!(
        "contact_string_tight_short_c4_v100",
        "Contact String Tight Short C4 Velocity 100",
        "04_contact/contact_string_tight_short_c4_v100.wav",
        ["contact", "string", "tight", "short", "C4", "velocity-100"],
        String,
        TightShort
    ),
    contact_case_spec!(
        "contact_string_tight_long_c4_v100",
        "Contact String Tight Long C4 Velocity 100",
        "04_contact/contact_string_tight_long_c4_v100.wav",
        ["contact", "string", "tight", "long", "C4", "velocity-100"],
        String,
        TightLong
    ),
    contact_case_spec!(
        "contact_string_wide_short_c4_v100",
        "Contact String Wide Short C4 Velocity 100",
        "04_contact/contact_string_wide_short_c4_v100.wav",
        ["contact", "string", "wide", "short", "C4", "velocity-100"],
        String,
        WideShort
    ),
    contact_case_spec!(
        "contact_string_wide_long_c4_v100",
        "Contact String Wide Long C4 Velocity 100",
        "04_contact/contact_string_wide_long_c4_v100.wav",
        ["contact", "string", "wide", "long", "C4", "velocity-100"],
        String,
        WideLong
    ),
    contact_case_spec!(
        "contact_tube_tight_short_c4_v100",
        "Contact Tube Tight Short C4 Velocity 100",
        "04_contact/contact_tube_tight_short_c4_v100.wav",
        ["contact", "tube", "tight", "short", "C4", "velocity-100"],
        Tube,
        TightShort
    ),
    contact_case_spec!(
        "contact_tube_tight_long_c4_v100",
        "Contact Tube Tight Long C4 Velocity 100",
        "04_contact/contact_tube_tight_long_c4_v100.wav",
        ["contact", "tube", "tight", "long", "C4", "velocity-100"],
        Tube,
        TightLong
    ),
    contact_case_spec!(
        "contact_tube_wide_short_c4_v100",
        "Contact Tube Wide Short C4 Velocity 100",
        "04_contact/contact_tube_wide_short_c4_v100.wav",
        ["contact", "tube", "wide", "short", "C4", "velocity-100"],
        Tube,
        WideShort
    ),
    contact_case_spec!(
        "contact_tube_wide_long_c4_v100",
        "Contact Tube Wide Long C4 Velocity 100",
        "04_contact/contact_tube_wide_long_c4_v100.wav",
        ["contact", "tube", "wide", "long", "C4", "velocity-100"],
        Tube,
        WideLong
    ),
];

pub(super) const SOURCE_BODY_CASE_SPECS: [CatalogCaseSpec; 6] = [
    source_body_case_spec!(
        "source_body_string_depth000_c4_v020",
        "Source-Body String Depth 0.0 C4 Velocity 20",
        "05_source_body_balance/source_body_string_depth000_c4_v020.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-0.0",
            "C4",
            "velocity-20"
        ],
        Depth000,
        20
    ),
    source_body_case_spec!(
        "source_body_string_depth000_c4_v127",
        "Source-Body String Depth 0.0 C4 Velocity 127",
        "05_source_body_balance/source_body_string_depth000_c4_v127.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-0.0",
            "C4",
            "velocity-127"
        ],
        Depth000,
        127
    ),
    source_body_case_spec!(
        "source_body_string_depth050_c4_v020",
        "Source-Body String Depth 0.5 C4 Velocity 20",
        "05_source_body_balance/source_body_string_depth050_c4_v020.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-0.5",
            "C4",
            "velocity-20"
        ],
        Depth050,
        20
    ),
    source_body_case_spec!(
        "source_body_string_depth050_c4_v127",
        "Source-Body String Depth 0.5 C4 Velocity 127",
        "05_source_body_balance/source_body_string_depth050_c4_v127.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-0.5",
            "C4",
            "velocity-127"
        ],
        Depth050,
        127
    ),
    source_body_case_spec!(
        "source_body_string_depth100_c4_v020",
        "Source-Body String Depth 1.0 C4 Velocity 20",
        "05_source_body_balance/source_body_string_depth100_c4_v020.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-1.0",
            "C4",
            "velocity-20"
        ],
        Depth100,
        20
    ),
    source_body_case_spec!(
        "source_body_string_depth100_c4_v127",
        "Source-Body String Depth 1.0 C4 Velocity 127",
        "05_source_body_balance/source_body_string_depth100_c4_v127.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-1.0",
            "C4",
            "velocity-127"
        ],
        Depth100,
        127
    ),
];
