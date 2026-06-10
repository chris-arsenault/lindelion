//! Sustained register-key Tube pitch probes at and above the register break.

use super::super::{
    CatalogCase, PatchRecipe, RenderSchedule, ScheduledNote, TubeReferenceArticulation,
    TubeReferenceHumanize, TubeReferenceMatchGain,
};

const DURATION_SECONDS: f32 = 3.20;
const START_SECONDS: f32 = 0.08;
const END_SECONDS: f32 = 3.05;
const V100: f32 = 100.0 / 127.0;

macro_rules! note {
    ($midi:literal) => {
        [ScheduledNote {
            start_seconds: START_SECONDS,
            end_seconds: END_SECONDS,
            note: $midi,
            velocity: V100,
        }]
    };
}

const A4: [ScheduledNote; 1] = note!(69);
const A_SHARP4: [ScheduledNote; 1] = note!(70);
const B4: [ScheduledNote; 1] = note!(71);
const C5: [ScheduledNote; 1] = note!(72);
const D5: [ScheduledNote; 1] = note!(74);
const E5: [ScheduledNote; 1] = note!(76);
const G5: [ScheduledNote; 1] = note!(79);
const C6: [ScheduledNote; 1] = note!(84);

macro_rules! probe_case {
    ($id:literal, $title:literal, $wav:literal, $note_tag:literal, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_register_key_pitch",
            relative_wav: $wav,
            tags: &["tube", "pitch-probe", "register-key", "sustain", $note_tag],
            patch_recipe: PatchRecipe::TubeReferenceMatchPhrase {
                articulation: TubeReferenceArticulation::Legato,
                gain: TubeReferenceMatchGain::RegisterKeyHighSustainVented,
                humanize: TubeReferenceHumanize::Off,
                body_enabled: true,
                reed_radiation_enabled: true,
            },
            schedule: RenderSchedule {
                duration_seconds: DURATION_SECONDS,
                notes: &$notes,
            },
        }
    };
}

pub(super) const TUBE_REGISTER_KEY_PITCH_CASES: [CatalogCase; 8] = [
    probe_case!(
        "tube_register_key_pitch_a4_m069",
        "Tube Register-Key Pitch Probe A4 / MIDI 69",
        "21_tube_register_key_pitch/tube_register_key_pitch_a4_m069.wav",
        "A4",
        A4
    ),
    probe_case!(
        "tube_register_key_pitch_a_sharp4_m070",
        "Tube Register-Key Pitch Probe A#4 / MIDI 70",
        "21_tube_register_key_pitch/tube_register_key_pitch_a_sharp4_m070.wav",
        "A#4",
        A_SHARP4
    ),
    probe_case!(
        "tube_register_key_pitch_b4_m071",
        "Tube Register-Key Pitch Probe B4 / MIDI 71",
        "21_tube_register_key_pitch/tube_register_key_pitch_b4_m071.wav",
        "B4",
        B4
    ),
    probe_case!(
        "tube_register_key_pitch_c5_m072",
        "Tube Register-Key Pitch Probe C5 / MIDI 72",
        "21_tube_register_key_pitch/tube_register_key_pitch_c5_m072.wav",
        "C5",
        C5
    ),
    probe_case!(
        "tube_register_key_pitch_d5_m074",
        "Tube Register-Key Pitch Probe D5 / MIDI 74",
        "21_tube_register_key_pitch/tube_register_key_pitch_d5_m074.wav",
        "D5",
        D5
    ),
    probe_case!(
        "tube_register_key_pitch_e5_m076",
        "Tube Register-Key Pitch Probe E5 / MIDI 76",
        "21_tube_register_key_pitch/tube_register_key_pitch_e5_m076.wav",
        "E5",
        E5
    ),
    probe_case!(
        "tube_register_key_pitch_g5_m079",
        "Tube Register-Key Pitch Probe G5 / MIDI 79",
        "21_tube_register_key_pitch/tube_register_key_pitch_g5_m079.wav",
        "G5",
        G5
    ),
    probe_case!(
        "tube_register_key_pitch_c6_m084",
        "Tube Register-Key Pitch Probe C6 / MIDI 84",
        "21_tube_register_key_pitch/tube_register_key_pitch_c6_m084.wav",
        "C6",
        C6
    ),
];
