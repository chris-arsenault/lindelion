//! Movement II — "Battle": fast battle theme in B minor at 168 BPM, picking
//! up Movement I's B-minor ending.
//!
//! | Bars    | Section | Material |
//! | ------- | ------- | -------- |
//! | 1–8     | Intro   | Bass ostinato alone, tom and ride layer in, crash swell |
//! | 9–24    | A       | Motto-in-diminution riff over i–bVI–bVII–i / i–bVI–iv–V |
//! | 25–40   | A'      | Riff sequenced higher, pumping string stabs, bell accents |
//! | 41–56   | B       | Royal Road (IVM7–V7–iii7–vi) in D major, lyrical augmented motto |
//! | 57–72   | Hijaz   | F# Phrygian-dominant episode, augmented-2nd riff over a drone |
//! | 73–80   | Shock   | The riff restated +1 in C minor; stop-time hits on V (F#) |
//! | 81–96   | A''     | Full-ensemble A return with bell counter-accents |
//! | 97–108  | Rideout | bVI–bVII–i anthem (G–A–Bm), hemiola in the last cycle |
//! | 109–116 | Coda    | Dream-arpeggio sixteenth ladder, unison tresillo stabs on B |
//! | 117–120 | Tail    | Ring-out |
//!
//! The drive layer is a tresillo (3+3+2 eighths) accent pattern; section seams
//! at bars 24 and 96 drop their last eighth (the riff truncates at beat 3.5
//! and the next phrase anticipates on 4.5) — the prog-meter hiccup written
//! against a steady bar grid.

use super::battle_lead::{tube_harmony, tube_lead};
use crate::composition::{FLAT, PhraseNote, TrackSpec, peak_mix, push_phrase, rms_mix, track};
use crate::render::Voice;
use crate::score::{Chord, Meter, Note, Part};

pub(crate) const METER: Meter = Meter::new(168.0, 4.0);
pub(crate) const TOTAL_BARS: usize = 120;

/// The bed sits back through the intro/A, drops for the lyrical B section,
/// and opens fully from the A return to the end.
const BED_RIDE: &[(f32, f32)] = &[
    (8.0, -2.0),
    (9.0, -1.5),
    (41.0, -3.0),
    (57.0, -2.0),
    (73.0, -1.0),
    (81.0, 0.0),
];

pub(crate) fn tracks(meter: Meter) -> Vec<TrackSpec> {
    let bed = |target: f32, pan: f32| rms_mix(target, pan, 90.0, BED_RIDE);
    vec![
        track(
            "tube-lead",
            Voice::TubeBattle,
            tube_lead(meter),
            rms_mix(-16.0, 0.0, 90.0, FLAT),
        ),
        track(
            "tube-harmony",
            Voice::TubeDark,
            tube_harmony(meter),
            bed(-21.0, -0.25),
        ),
        track(
            "string-chord-low",
            Voice::StringBow,
            string_chord_voice(meter, 0),
            bed(-27.0, -0.7),
        ),
        track(
            "string-chord-mid",
            Voice::StringBow,
            string_chord_voice(meter, 1),
            bed(-27.0, 0.15),
        ),
        track(
            "string-chord-high",
            Voice::StringBow,
            string_chord_voice(meter, 2),
            bed(-27.0, 0.7),
        ),
        track(
            "string-bass",
            Voice::StringPick,
            string_bass(meter),
            rms_mix(-18.5, 0.0, 35.0, FLAT),
        ),
        track(
            "modal-arp",
            Voice::ModalKeys,
            modal_arp(meter),
            bed(-25.0, 0.5),
        ),
        track(
            "modal-bells",
            Voice::ModalBells,
            modal_bells(meter),
            rms_mix(-21.0, -0.5, 90.0, FLAT),
        ),
        track(
            "cymbal-ride",
            Voice::CymbalRide,
            cymbal_ride(meter),
            bed(-24.0, 0.4),
        ),
        track(
            "cymbal-tom",
            Voice::CymbalTom,
            cymbal_tom(meter),
            rms_mix(-20.0, -0.1, 35.0, FLAT),
        ),
        track(
            "cymbal-crash",
            Voice::CymbalCrash,
            cymbal_crash(meter),
            peak_mix(-8.0, 0.0, 130.0),
        ),
    ]
}

// --- Harmony -----------------------------------------------------------------

const BM: Chord = Chord {
    bass: 47,
    voices: [59, 62, 66],
};
const G: Chord = Chord {
    bass: 43,
    voices: [59, 62, 67],
};
const A: Chord = Chord {
    bass: 45,
    voices: [61, 64, 69],
};
const EM: Chord = Chord {
    bass: 40,
    voices: [59, 64, 67],
};
const FSH: Chord = Chord {
    bass: 42,
    voices: [58, 61, 66],
};
const GM7: Chord = Chord {
    bass: 43,
    voices: [59, 62, 66],
};
const A7: Chord = Chord {
    bass: 45,
    voices: [61, 64, 67],
};
const FSM7: Chord = Chord {
    bass: 42,
    voices: [57, 61, 64],
};
const CM: Chord = Chord {
    bass: 48,
    voices: [60, 63, 67],
};
const AB: Chord = Chord {
    bass: 44,
    voices: [60, 63, 68],
};
const BB: Chord = Chord {
    bass: 46,
    voices: [58, 62, 65],
};

/// One A-section harmonic period: i–bVI–bVII–i then i–bVI–iv–V.
const A_PERIOD: [Chord; 8] = [BM, G, A, BM, BM, G, EM, FSH];
/// Royal Road (IVM7–V7–iii7–vi in D major): the JRPG lyrical lift.
const ROYAL_ROAD: [Chord; 4] = [GM7, A7, FSM7, BM];

/// The chord per bar. The battle is written directly in concert pitch (no
/// section transposes); the Hijaz episode alternates bII color over a V drone.
fn harmony() -> Vec<(usize, Chord)> {
    let mut bars: Vec<(usize, Chord)> = Vec::new();
    let mut push = |start: usize, chords: &[Chord]| {
        for (index, chord) in chords.iter().enumerate() {
            bars.push((start + index, *chord));
        }
    };
    push(9, &A_PERIOD);
    push(17, &A_PERIOD);
    push(25, &A_PERIOD);
    push(33, &A_PERIOD);
    push(41, &ROYAL_ROAD);
    push(45, &ROYAL_ROAD);
    push(49, &ROYAL_ROAD);
    push(53, &[GM7, A7, EM, FSH]);
    push(
        57,
        &[
            FSH, FSH, G, G, FSH, FSH, G, G, FSH, FSH, G, G, FSH, G, FSH, FSH,
        ],
    );
    push(73, &[CM, AB, BB, CM, CM, AB, FSH, FSH]);
    push(81, &A_PERIOD);
    push(89, &A_PERIOD);
    push(97, &[G, A, BM, BM, G, A, BM, BM, G, A, BM, FSH]);
    push(109, &[BM, G, A, BM, BM, BM, BM, BM]);
    bars
}

// --- Strings -----------------------------------------------------------------

/// Pumping quarter-note stabs with a written-in velocity duck on the off
/// quarters (the sidechain-pump as composition); long pads in the B section;
/// silence in the stop-time bars.
fn string_chord_voice(meter: Meter, voice_index: usize) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord) in harmony() {
        let tone = chord.voices[voice_index];
        match bar {
            41..=56 => part.hit(bar, 1.0, 3.7, tone, 0.52),
            79 | 80 => stop_time_hits(&mut part, bar, tone, 0.78),
            113..=115 => stop_time_hits(&mut part, bar, tone, 0.82),
            116 => part.hit(bar, 1.0, 6.0, tone, 0.8),
            _ => pump_bar(&mut part, bar, tone),
        }
    }
    part.into_notes()
}

fn pump_bar(part: &mut Part, bar: usize, tone: u8) {
    const PUMP: [f32; 4] = [0.66, 0.46, 0.56, 0.46];
    for (beat, velocity) in PUMP.iter().enumerate() {
        part.hit(bar, 1.0 + beat as f32, 0.8, tone, *velocity);
    }
}

/// Tresillo hits with silence between: the stop-time figure.
pub(super) fn stop_time_hits(part: &mut Part, bar: usize, tone: u8, velocity: f32) {
    part.hit(bar, 1.0, 0.6, tone, velocity);
    part.hit(bar, 2.5, 0.6, tone, velocity - 0.04);
    part.hit(bar, 4.0, 0.6, tone, velocity);
}

fn string_bass(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    // Intro: the ostinato alone on the B pedal, building velocity.
    for bar in 1..=8 {
        ostinato_bar(&mut part, bar, BM.bass, 0.5 + 0.03 * (bar - 1) as f32);
    }
    for (bar, chord) in harmony() {
        match bar {
            41..=56 => {
                // B section: half-note roots with an eighth pickup.
                part.hit(bar, 1.0, 1.9, chord.bass, 0.62);
                part.hit(bar, 3.0, 1.4, chord.bass, 0.56);
                part.hit(bar, 4.5, 0.4, chord.bass, 0.5);
            }
            79 | 80 => stop_time_hits(&mut part, bar, chord.bass, 0.82),
            97..=104 => quarter_pulse(&mut part, bar, chord.bass, 0.72),
            105..=108 => hemiola_bass(&mut part, bar, chord.bass),
            113..=115 => stop_time_hits(&mut part, bar, chord.bass, 0.85),
            116 => part.hit(bar, 1.0, 6.0, chord.bass, 0.85),
            _ => ostinato_bar(&mut part, bar, chord.bass, 0.66),
        }
    }
    part.into_notes()
}

/// The driving eighth ostinato: root pedal with the fifth on beat 3 and the
/// flat seventh pushed on the tresillo accent of beat 4.
fn ostinato_bar(part: &mut Part, bar: usize, root: u8, velocity: f32) {
    const SLOTS: [(f32, i32, f32); 8] = [
        (1.0, 0, 0.10),
        (1.5, 0, -0.06),
        (2.0, 0, 0.0),
        (2.5, 0, 0.08),
        (3.0, 7, -0.02),
        (3.5, 0, -0.06),
        (4.0, -2, 0.10),
        (4.5, 0, -0.04),
    ];
    for (beat, offset, accent) in SLOTS {
        let note = (i32::from(root) + offset).clamp(0, 127) as u8;
        part.hit(bar, beat, 0.4, note, (velocity + accent).clamp(0.0, 1.0));
    }
}

fn quarter_pulse(part: &mut Part, bar: usize, root: u8, velocity: f32) {
    for beat in 0..4 {
        part.hit(
            bar,
            1.0 + beat as f32,
            0.85,
            root,
            velocity + if beat == 0 { 0.06 } else { 0.0 },
        );
    }
}

/// Rideout hemiola: dotted-quarter attacks crossing the barline (3 against 2).
fn hemiola_bass(part: &mut Part, bar: usize, root: u8) {
    for slot in 0..3 {
        part.hit(bar, 1.0 + slot as f32 * 1.5, 1.2, root, 0.78);
    }
}

// --- Modal keys (the sequencer layer) --------------------------------------------

fn modal_arp(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord) in harmony() {
        match bar {
            // The episode belongs to the drone; the arp re-enters for the climb.
            57..=68 => {}
            79 | 80 => {}
            109..=112 => ladder_bar(&mut part, bar, chord, 0.62 + 0.04 * (bar - 109) as f32),
            113..=116 => {}
            41..=56 => arp_eighths(&mut part, bar, chord, 0.5),
            _ => arp_sixteenth_pairs(&mut part, bar, chord, 0.56),
        }
    }
    part.into_notes()
}

fn arp_tones(chord: Chord) -> [u8; 4] {
    let [low, mid, high] = chord.voices;
    [low + 24, mid + 24, high + 24, low + 36]
}

fn arp_eighths(part: &mut Part, bar: usize, chord: Chord, velocity: f32) {
    const PATTERN: [usize; 8] = [0, 1, 2, 3, 2, 1, 0, 1];
    let tones = arp_tones(chord);
    for (slot, tone_index) in PATTERN.iter().enumerate() {
        let accent = if slot == 0 { 0.08 } else { 0.0 };
        part.hit(
            bar,
            1.0 + slot as f32 * 0.5,
            1.0,
            tones[*tone_index],
            velocity + accent,
        );
    }
}

/// The battle sequencer: sixteenth pairs on the tresillo accents, eighths
/// elsewhere — denser than the prelude's arpeggios without saturating.
fn arp_sixteenth_pairs(part: &mut Part, bar: usize, chord: Chord, velocity: f32) {
    const PATTERN: [(f32, usize, f32); 11] = [
        (1.0, 0, 0.08),
        (1.25, 1, -0.06),
        (1.5, 2, 0.0),
        (2.0, 3, 0.0),
        (2.5, 2, 0.06),
        (2.75, 1, -0.06),
        (3.0, 0, 0.0),
        (3.5, 1, 0.0),
        (4.0, 2, 0.06),
        (4.25, 3, -0.06),
        (4.5, 1, -0.02),
    ];
    let tones = arp_tones(chord);
    for (beat, tone_index, accent) in PATTERN {
        part.hit(bar, beat, 0.6, tones[tone_index], velocity + accent);
    }
}

/// Coda ladder: two-octave rising sixteenths, the Dream arpeggio in diminution.
fn ladder_bar(part: &mut Part, bar: usize, chord: Chord, velocity: f32) {
    let [low, mid, high] = chord.voices;
    let ladder = [
        low + 12,
        mid + 12,
        high + 12,
        low + 24,
        mid + 24,
        high + 24,
        low + 36,
        mid + 36,
    ];
    for slot in 0..16 {
        part.hit(
            bar,
            1.0 + slot as f32 * 0.25,
            0.7,
            ladder[slot % 8],
            velocity + 0.015 * slot as f32,
        );
    }
}

// --- Modal bells ----------------------------------------------------------------

/// Bell accents: offbeat chimes in A', a high counter-line in the A return,
/// sparkle doubling in the coda ladder bars.
fn modal_bells(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in (25..=39).step_by(2) {
        part.hit(bar, 2.5, 1.0, 86, 0.5);
        part.hit(bar, 4.0, 1.0, 83, 0.46);
    }
    const BELLS_RETURN: [PhraseNote; 12] = [
        (1, 1.0, 2.0, 83, 0.56),
        (1, 3.0, 2.0, 86, 0.58),
        (2, 1.0, 2.0, 86, 0.56),
        (2, 3.0, 2.0, 83, 0.52),
        (3, 1.0, 3.0, 85, 0.58),
        (4, 1.0, 4.0, 83, 0.54),
        (5, 1.0, 2.0, 83, 0.56),
        (5, 3.0, 2.0, 86, 0.58),
        (6, 1.0, 2.0, 88, 0.60),
        (6, 3.0, 2.0, 86, 0.56),
        (7, 1.0, 3.0, 83, 0.56),
        (8, 1.0, 4.0, 82, 0.54),
    ];
    push_phrase(&mut part, &BELLS_RETURN, 88, usize::MAX);
    for bar in 109..=112 {
        part.hit(bar, 1.0, 3.0, 95, 0.5 + 0.04 * (bar - 109) as f32);
    }
    part.hit(116, 1.0, 6.0, 95, 0.66);
    part.hit(116, 1.5, 6.0, 86, 0.6);
    part.into_notes()
}

// --- Cymbals -----------------------------------------------------------------------

fn cymbal_ride(meter: Meter) -> Vec<Note> {
    // (first bar, last bar, base velocity, per-bar crescendo)
    const EIGHTH_RUNS: [(usize, usize, f32, f32); 7] = [
        (5, 8, 0.34, 0.02),
        (9, 40, 0.4, 0.0),
        (65, 72, 0.36, 0.01),
        (73, 78, 0.44, 0.0),
        (81, 96, 0.44, 0.0),
        (97, 104, 0.42, 0.005),
        (109, 112, 0.48, 0.0),
    ];
    let mut part = Part::new(meter);
    for (first, last, base, slope) in EIGHTH_RUNS {
        for bar in first..=last {
            ride_eighths(&mut part, bar, base + slope * (bar - first) as f32);
        }
    }
    // B section: half-note time under the lyrical melody.
    for bar in 41..=56 {
        part.hit(bar, 1.0, 0.9, 60, 0.42);
        part.hit(bar, 3.0, 0.9, 60, 0.32);
    }
    // Hemiola: the ride joins the dotted-quarter cross-accent.
    for bar in 105..=108 {
        for slot in 0..3 {
            part.hit(bar, 1.0 + slot as f32 * 1.5, 0.9, 60, 0.52);
        }
    }
    part.into_notes()
}

fn ride_eighths(part: &mut Part, bar: usize, base: f32) {
    // Tresillo accents on 1, 2.5, and 4.
    const ACCENT_SLOTS: [usize; 3] = [0, 3, 6];
    for slot in 0..8 {
        let accent = if ACCENT_SLOTS.contains(&slot) {
            0.08
        } else {
            0.0
        };
        part.hit(bar, 1.0 + slot as f32 * 0.5, 0.45, 60, base + accent);
    }
}

/// The drive layer: a kick-like tresillo (1, 2.5, 4) with a beat-3 backbeat.
fn cymbal_tom(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in 3..=8 {
        tom_bar(&mut part, bar, 0.5 + 0.04 * (bar - 3) as f32);
    }
    for bar in 9..=78 {
        let velocity = match bar {
            41..=56 => 0.5,
            57..=64 => 0.58,
            _ => 0.66,
        };
        tom_bar(&mut part, bar, velocity);
    }
    stop_time_hits(&mut part, 79, 60, 0.8);
    stop_time_hits(&mut part, 80, 60, 0.82);
    for bar in 81..=108 {
        tom_bar(&mut part, bar, 0.7);
    }
    for bar in 109..=112 {
        tom_bar(&mut part, bar, 0.74);
    }
    for bar in [113, 114, 115] {
        stop_time_hits(&mut part, bar, 60, 0.84);
    }
    part.hit(116, 1.0, 1.0, 60, 0.86);
    part.into_notes()
}

fn tom_bar(part: &mut Part, bar: usize, velocity: f32) {
    part.hit(bar, 1.0, 0.5, 60, velocity);
    part.hit(bar, 2.5, 0.5, 60, velocity - 0.08);
    part.hit(bar, 3.0, 0.5, 60, velocity - 0.04);
    part.hit(bar, 4.0, 0.5, 60, velocity);
    // The seam bars push the anticipation.
    if bar == 24 || bar == 96 {
        part.hit(bar, 4.5, 0.5, 60, velocity + 0.06);
    }
}

fn cymbal_crash(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    // Swell into the A theme.
    for slot in 0..8 {
        part.hit(
            8,
            1.0 + slot as f32 * 0.5,
            0.45,
            60,
            0.10 + 0.05 * slot as f32,
        );
    }
    for (bar, velocity) in [
        (9, 0.6),
        (25, 0.62),
        (41, 0.55),
        (57, 0.6),
        (73, 0.7),
        (81, 0.72),
        (97, 0.7),
        (105, 0.62),
        (109, 0.72),
    ] {
        part.hit(bar, 1.0, 6.0, 60, velocity);
    }
    part.hit(113, 1.0, 2.0, 60, 0.7);
    part.hit(116, 1.0, 14.0, 60, 0.85);
    part.into_notes()
}
