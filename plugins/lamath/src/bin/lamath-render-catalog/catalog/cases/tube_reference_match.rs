//! Reference/current Tube pairs against owner clarinet gestures. These are intentionally whole
//! gestures rather than steady-center snippets: the reference cases copy the fixture timing, and
//! the Tube cases use matching concert-pitch MIDI schedules.

use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeReferenceArticulation,
    TubeReferenceMatchGain,
};

const EMPTY: [ScheduledNote; 0] = [];

const LOW_E_SUSTAIN_DURATION_SECONDS: f32 = 6.50;
const REGISTER_KEY_HIGH_DURATION_SECONDS: f32 = 6.65;
const LOW_HIGH_ARTICULATION_DURATION_SECONDS: f32 = 6.90;

const V100: f32 = 100.0 / 127.0;

// Source fixture pitch run is concert D3 for written low E. Keep the pre-onset/release padding.
const TUBE_LOW_E_SUSTAIN: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.06,
    end_seconds: 6.40,
    note: 50,
    velocity: V100,
}];

// Same fingering with register key in the source fixture, measured around concert A4.
const TUBE_REGISTER_KEY_HIGH_SUSTAIN: [ScheduledNote; 1] = [ScheduledNote {
    start_seconds: 0.08,
    end_seconds: 6.55,
    note: 69,
    velocity: V100,
}];

const TUBE_LOW_HIGH_ARTICULATION: [ScheduledNote; 32] = [
    ScheduledNote {
        start_seconds: 0.150,
        end_seconds: 0.365,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.389,
        end_seconds: 0.570,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.594,
        end_seconds: 0.769,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.793,
        end_seconds: 0.994,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.018,
        end_seconds: 1.193,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.217,
        end_seconds: 1.423,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.447,
        end_seconds: 1.622,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.646,
        end_seconds: 1.862,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.886,
        end_seconds: 2.056,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.080,
        end_seconds: 2.266,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.290,
        end_seconds: 2.505,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.529,
        end_seconds: 2.700,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.724,
        end_seconds: 2.904,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.928,
        end_seconds: 3.124,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.148,
        end_seconds: 3.333,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.357,
        end_seconds: 3.558,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.582,
        end_seconds: 3.747,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.771,
        end_seconds: 3.967,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.991,
        end_seconds: 4.157,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 4.181,
        end_seconds: 4.376,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 4.400,
        end_seconds: 4.566,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 4.590,
        end_seconds: 4.800,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 4.824,
        end_seconds: 4.990,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 5.014,
        end_seconds: 5.224,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 5.248,
        end_seconds: 5.414,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 5.438,
        end_seconds: 5.628,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 5.652,
        end_seconds: 5.798,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 5.822,
        end_seconds: 6.012,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 6.036,
        end_seconds: 6.232,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 6.256,
        end_seconds: 6.456,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 6.480,
        end_seconds: 6.646,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 6.670,
        end_seconds: 6.810,
        note: 69,
        velocity: V100,
    },
];

pub(super) const TUBE_REFERENCE_MATCH_CASES: [CatalogCase; 6] = [
    reference_case(
        "tube_ref_low_e_sustain_reference",
        "Reference Clarinet Low E Sustain",
        "19_tube_reference_match/tube_ref_low_e_sustain_reference.wav",
        &["tube", "reference-match", "reference", "low-e", "sustain"],
        "testdata/audio/owner_clarinet_low_e_sustain.wav",
        LOW_E_SUSTAIN_DURATION_SECONDS,
    ),
    tube_case(
        "tube_ref_low_e_sustain_current",
        "Current Tube Low E Sustain Match (RMS-matched)",
        "19_tube_reference_match/tube_ref_low_e_sustain_current.wav",
        &["tube", "reference-match", "current", "low-e", "sustain"],
        TubeReferenceArticulation::Legato,
        TubeReferenceMatchGain::LowESustain,
        LOW_E_SUSTAIN_DURATION_SECONDS,
        &TUBE_LOW_E_SUSTAIN,
    ),
    reference_case(
        "tube_ref_register_key_high_reference",
        "Reference Clarinet Register-Key High Sustain",
        "19_tube_reference_match/tube_ref_register_key_high_reference.wav",
        &[
            "tube",
            "reference-match",
            "reference",
            "register-key",
            "sustain",
        ],
        "testdata/audio/owner_clarinet_register_key_high_sustain.wav",
        REGISTER_KEY_HIGH_DURATION_SECONDS,
    ),
    tube_case(
        "tube_ref_register_key_high_current",
        "Current Tube Register-Key High Sustain Match (RMS-matched)",
        "19_tube_reference_match/tube_ref_register_key_high_current.wav",
        &[
            "tube",
            "reference-match",
            "current",
            "register-key",
            "sustain",
        ],
        TubeReferenceArticulation::Legato,
        TubeReferenceMatchGain::RegisterKeyHighSustain,
        REGISTER_KEY_HIGH_DURATION_SECONDS,
        &TUBE_REGISTER_KEY_HIGH_SUSTAIN,
    ),
    reference_case(
        "tube_ref_low_high_articulation_reference",
        "Reference Clarinet Low/High Articulation",
        "19_tube_reference_match/tube_ref_low_high_articulation_reference.wav",
        &[
            "tube",
            "reference-match",
            "reference",
            "register-key",
            "articulation",
        ],
        "testdata/audio/owner_clarinet_low_high_articulation.wav",
        LOW_HIGH_ARTICULATION_DURATION_SECONDS,
    ),
    tube_case(
        "tube_ref_low_high_articulation_current",
        "Current Tube Low/High Articulation Match (RMS-matched)",
        "19_tube_reference_match/tube_ref_low_high_articulation_current.wav",
        &[
            "tube",
            "reference-match",
            "current",
            "register-key",
            "articulation",
        ],
        TubeReferenceArticulation::Tongue,
        TubeReferenceMatchGain::LowHighArticulation,
        LOW_HIGH_ARTICULATION_DURATION_SECONDS,
        &TUBE_LOW_HIGH_ARTICULATION,
    ),
];

const fn reference_case(
    id: &'static str,
    title: &'static str,
    relative_wav: &'static str,
    tags: &'static [&'static str],
    path: &'static str,
    duration_seconds: f32,
) -> CatalogCase {
    CatalogCase {
        id,
        title,
        group_id: "tube_reference_match",
        relative_wav,
        tags,
        patch_recipe: PatchRecipe::ReferenceWav { path },
        schedule: RenderSchedule {
            duration_seconds,
            notes: &EMPTY,
        },
    }
}

const fn tube_case(
    id: &'static str,
    title: &'static str,
    relative_wav: &'static str,
    tags: &'static [&'static str],
    articulation: TubeReferenceArticulation,
    gain: TubeReferenceMatchGain,
    duration_seconds: f32,
    notes: &'static [ScheduledNote],
) -> CatalogCase {
    CatalogCase {
        id,
        title,
        group_id: "tube_reference_match",
        relative_wav,
        tags,
        patch_recipe: PatchRecipe::TubeReferenceMatchPhrase { articulation, gain },
        schedule: RenderSchedule {
            duration_seconds,
            notes,
        },
    }
}
