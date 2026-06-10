use super::super::{
    BowHumanizeDepth, CatalogCase, ContactRecipe, DriverRecipe, PatchRecipe, RenderSchedule,
    ResonatorFamily, ScheduledNote, SourceBodyDepth,
};
use super::CatalogCaseSpec;

const SOURCE_BODY_REPEAT_DURATION_SECONDS: f32 = 2.2;
const BOW_SMOOTH_SUSTAIN_DURATION_SECONDS: f32 = 8.0;
const BOW_SCALE_DURATION_SECONDS: f32 = 6.25;
const BOW_SMOOTH_SUSTAIN_C4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 8.0,
    note: 60,
    velocity: 100.0 / 127.0,
}];
const BOW_SMOOTH_SUSTAIN_D4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 8.0,
    note: 62,
    velocity: 100.0 / 127.0,
}];
const BOW_SMOOTH_SUSTAIN_E4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 8.0,
    note: 64,
    velocity: 100.0 / 127.0,
}];
const BOW_SCALE_C4_C5_V100: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.58,
        note: 60,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 0.78,
        end_seconds: 1.36,
        note: 62,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 1.56,
        end_seconds: 2.14,
        note: 64,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 2.34,
        end_seconds: 2.92,
        note: 65,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 3.12,
        end_seconds: 3.70,
        note: 67,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 3.90,
        end_seconds: 4.48,
        note: 69,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 4.68,
        end_seconds: 5.26,
        note: 71,
        velocity: 100.0 / 127.0,
    },
    ScheduledNote {
        start_seconds: 5.46,
        end_seconds: 6.04,
        note: 72,
        velocity: 100.0 / 127.0,
    },
];
const SOURCE_BODY_REPEATED_C4_V127: [ScheduledNote; 5] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.18,
        note: 60,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.28,
        end_seconds: 0.46,
        note: 60,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.56,
        end_seconds: 0.74,
        note: 60,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 0.84,
        end_seconds: 1.02,
        note: 60,
        velocity: 1.0,
    },
    ScheduledNote {
        start_seconds: 1.12,
        end_seconds: 1.30,
        note: 60,
        velocity: 1.0,
    },
];

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

macro_rules! source_body_repeat_case {
    ($id:literal, $title:literal, $relative_wav:literal, [$($tag:literal),+], $depth:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "source_body_balance",
            relative_wav: $relative_wav,
            tags: &[$($tag),+],
            patch_recipe: PatchRecipe::SourceBodyBalance {
                depth: SourceBodyDepth::$depth,
            },
            schedule: RenderSchedule {
                duration_seconds: SOURCE_BODY_REPEAT_DURATION_SECONDS,
                notes: &SOURCE_BODY_REPEATED_C4_V127,
            },
        }
    };
}

pub(super) const DRIVER_CASE_SPECS: [CatalogCaseSpec; 6] = [
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
        "driver_string_bow_scratch_c4_v100",
        "Driver String Bow Scratch C4 Velocity 100",
        "03_drivers/driver_string_bow_scratch_c4_v100.wav",
        ["drivers", "string", "bow-scratch", "C4", "velocity-100"],
        String,
        BowScratch
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

pub(super) const BOW_DRIVER_PHRASE_CASES: [CatalogCase; 7] = [
    CatalogCase {
        id: "driver_string_bow_smooth_c4_v100",
        title: "Driver String Bow Smooth C4 Velocity 100",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_c4_v100.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "C4",
            "velocity-100",
            "sustain",
            "bow-single",
        ],
        patch_recipe: PatchRecipe::Driver {
            family: ResonatorFamily::String,
            driver: DriverRecipe::BowSmooth,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_smooth_d4_v100",
        title: "Driver String Bow Smooth D4 Velocity 100",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_d4_v100.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "D4",
            "velocity-100",
            "sustain",
            "bow-single",
        ],
        patch_recipe: PatchRecipe::Driver {
            family: ResonatorFamily::String,
            driver: DriverRecipe::BowSmooth,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_D4_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_smooth_e4_v100",
        title: "Driver String Bow Smooth E4 Velocity 100",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_e4_v100.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "E4",
            "velocity-100",
            "sustain",
            "bow-single",
        ],
        patch_recipe: PatchRecipe::Driver {
            family: ResonatorFamily::String,
            driver: DriverRecipe::BowSmooth,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_E4_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_smooth_scale_c4_c5_v100",
        title: "Driver String Bow Smooth Scale C4-C5 Velocity 100",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_scale_c4_c5_v100.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "scale",
            "C4-C5",
            "velocity-100",
        ],
        patch_recipe: PatchRecipe::Driver {
            family: ResonatorFamily::String,
            driver: DriverRecipe::BowSmooth,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SCALE_DURATION_SECONDS,
            notes: &BOW_SCALE_C4_C5_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_alternating_scale_c4_c5_v100",
        title: "Driver String Bow Alternating Smooth Scratch Scale C4-C5 Velocity 100",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_alternating_scale_c4_c5_v100.wav",
        tags: &[
            "drivers",
            "string",
            "bow-alternating",
            "bow-smooth",
            "bow-scratch",
            "scale",
            "C4-C5",
            "velocity-100",
        ],
        patch_recipe: PatchRecipe::StringBowAlternatingScale,
        schedule: RenderSchedule {
            duration_seconds: BOW_SCALE_DURATION_SECONDS,
            notes: &BOW_SCALE_C4_C5_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_smooth_c4_humanize_off",
        title: "Driver String Bow Smooth C4 Humanize Off",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_c4_humanize_off.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "C4",
            "velocity-100",
            "sustain",
            "bow-humanize",
        ],
        patch_recipe: PatchRecipe::StringBowHumanize {
            depth: BowHumanizeDepth::Off,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_smooth_c4_humanize_full",
        title: "Driver String Bow Smooth C4 Humanize Full",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_c4_humanize_full.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "C4",
            "velocity-100",
            "sustain",
            "bow-humanize",
        ],
        patch_recipe: PatchRecipe::StringBowHumanize {
            depth: BowHumanizeDepth::Full,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
];

// Contact (strike-onset) shaping applies only to the struck/strike-injected drivers. The Tube
// is reed-driven and terminates the mouth, so the contact stage is bypassed for it (ADR-0032) —
// hence the Tube has no contact cases (they would all render identically).
pub(super) const CONTACT_CASE_SPECS: [CatalogCaseSpec; 4] = [
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

pub(super) const SOURCE_BODY_REPEAT_CASES: [CatalogCase; 3] = [
    source_body_repeat_case!(
        "source_body_string_depth000_c4_repeated_v127",
        "Source-Body String Depth 0.0 C4 Repeated Velocity 127",
        "05_source_body_balance/source_body_string_depth000_c4_repeated_v127.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-0.0",
            "C4",
            "repeated",
            "velocity-127"
        ],
        Depth000
    ),
    source_body_repeat_case!(
        "source_body_string_depth050_c4_repeated_v127",
        "Source-Body String Depth 0.5 C4 Repeated Velocity 127",
        "05_source_body_balance/source_body_string_depth050_c4_repeated_v127.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-0.5",
            "C4",
            "repeated",
            "velocity-127"
        ],
        Depth050
    ),
    source_body_repeat_case!(
        "source_body_string_depth100_c4_repeated_v127",
        "Source-Body String Depth 1.0 C4 Repeated Velocity 127",
        "05_source_body_balance/source_body_string_depth100_c4_repeated_v127.wav",
        [
            "source-body",
            "balance",
            "string",
            "depth-1.0",
            "C4",
            "repeated",
            "velocity-127"
        ],
        Depth100
    ),
];
