//! Sustained low-register Tube pitch probes below the register break.

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

const D3: [ScheduledNote; 1] = note!(50);
const G3: [ScheduledNote; 1] = note!(55);
const C4: [ScheduledNote; 1] = note!(60);
const E4: [ScheduledNote; 1] = note!(64);
const G4: [ScheduledNote; 1] = note!(67);
const G_SHARP4: [ScheduledNote; 1] = note!(68);

macro_rules! probe_case {
    ($id:literal, $title:literal, $wav:literal, $note_tag:literal, $notes:ident) => {
        CatalogCase {
            id: $id,
            title: $title,
            group_id: "tube_low_register_pitch",
            relative_wav: $wav,
            tags: &["tube", "pitch-probe", "low-register", "sustain", $note_tag],
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

pub(super) const TUBE_LOW_REGISTER_PITCH_CASES: [CatalogCase; 6] = [
    probe_case!(
        "tube_low_register_pitch_d3_m050",
        "Tube Low-Register Pitch Probe D3 / MIDI 50",
        "20_tube_low_register_pitch/tube_low_register_pitch_d3_m050.wav",
        "D3",
        D3
    ),
    probe_case!(
        "tube_low_register_pitch_g3_m055",
        "Tube Low-Register Pitch Probe G3 / MIDI 55",
        "20_tube_low_register_pitch/tube_low_register_pitch_g3_m055.wav",
        "G3",
        G3
    ),
    probe_case!(
        "tube_low_register_pitch_c4_m060",
        "Tube Low-Register Pitch Probe C4 / MIDI 60",
        "20_tube_low_register_pitch/tube_low_register_pitch_c4_m060.wav",
        "C4",
        C4
    ),
    probe_case!(
        "tube_low_register_pitch_e4_m064",
        "Tube Low-Register Pitch Probe E4 / MIDI 64",
        "20_tube_low_register_pitch/tube_low_register_pitch_e4_m064.wav",
        "E4",
        E4
    ),
    probe_case!(
        "tube_low_register_pitch_g4_m067",
        "Tube Low-Register Pitch Probe G4 / MIDI 67",
        "20_tube_low_register_pitch/tube_low_register_pitch_g4_m067.wav",
        "G4",
        G4
    ),
    probe_case!(
        "tube_low_register_pitch_g_sharp4_m068",
        "Tube Low-Register Pitch Probe G#4 / MIDI 68",
        "20_tube_low_register_pitch/tube_low_register_pitch_g_sharp4_m068.wav",
        "G#4",
        G_SHARP4
    ),
];
