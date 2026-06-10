//! Bow phrase cases: the sustained/scale schedules and the bow-driver phrase
//! case list (smooth sustains, scales, humanize and phrasing A/B pairs, and
//! the Tube phrasing A/B that shares the sustain schedule).

use super::super::{
    BowHumanizeDepth, CatalogCase, DriverRecipe, PatchRecipe, PhrasingDepth, RenderSchedule,
    ResonatorFamily, ScheduledNote,
};

const BOW_SMOOTH_SUSTAIN_DURATION_SECONDS: f32 = 8.0;
const BOW_SCALE_DURATION_SECONDS: f32 = 6.25;
const BOW_SMOOTH_SUSTAIN_C4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 6.5,
    note: 60,
    velocity: 100.0 / 127.0,
}];
const BOW_SMOOTH_SUSTAIN_D4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 6.5,
    note: 62,
    velocity: 100.0 / 127.0,
}];
const BOW_SMOOTH_SUSTAIN_E4_V100: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.0,
    end_seconds: 6.5,
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

pub(super) const BOW_DRIVER_PHRASE_CASES: [CatalogCase; 11] = [
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
    CatalogCase {
        id: "driver_string_bow_smooth_c4_phrasing_off",
        title: "Driver String Bow Smooth C4 Phrasing Off",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_c4_phrasing_off.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "C4",
            "velocity-100",
            "sustain",
            "bow-phrasing",
        ],
        patch_recipe: PatchRecipe::StringBowPhrasing {
            depth: PhrasingDepth::Off,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
    CatalogCase {
        id: "driver_string_bow_smooth_c4_phrasing_full",
        title: "Driver String Bow Smooth C4 Phrasing Full",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_string_bow_smooth_c4_phrasing_full.wav",
        tags: &[
            "drivers",
            "string",
            "bow-smooth",
            "C4",
            "velocity-100",
            "sustain",
            "bow-phrasing",
        ],
        patch_recipe: PatchRecipe::StringBowPhrasing {
            depth: PhrasingDepth::Full,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
    CatalogCase {
        id: "driver_tube_reed_c4_phrasing_off",
        title: "Driver Tube Reed C4 Phrasing Off",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_tube_reed_c4_phrasing_off.wav",
        tags: &[
            "drivers",
            "tube",
            "reed",
            "C4",
            "velocity-100",
            "sustain",
            "tube-phrasing",
        ],
        patch_recipe: PatchRecipe::TubeWindPhrasing {
            depth: PhrasingDepth::Off,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
    CatalogCase {
        id: "driver_tube_reed_c4_phrasing_full",
        title: "Driver Tube Reed C4 Phrasing Full",
        group_id: "drivers",
        relative_wav: "03_drivers/driver_tube_reed_c4_phrasing_full.wav",
        tags: &[
            "drivers",
            "tube",
            "reed",
            "C4",
            "velocity-100",
            "sustain",
            "tube-phrasing",
        ],
        patch_recipe: PatchRecipe::TubeWindPhrasing {
            depth: PhrasingDepth::Full,
        },
        schedule: RenderSchedule {
            duration_seconds: BOW_SMOOTH_SUSTAIN_DURATION_SECONDS,
            notes: &BOW_SMOOTH_SUSTAIN_C4_V100,
        },
    },
];
