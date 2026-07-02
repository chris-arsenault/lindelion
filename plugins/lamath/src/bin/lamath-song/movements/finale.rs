//! Movement V — "Finale": the boss and the apotheosis. A minor → B major,
//! 116 BPM: slower and heavier than the battle, menace from weight — slow
//! harmonic rhythm under a relentless sixteenth surface.
//!
//! | Bars   | Section | Material |
//! | ------ | ------- | -------- |
//! | 1–16   | Menace  | Hexatonic-pole oscillation (Am ↔ D♭ major, no common tones), motto fragments low |
//! | 17–32  | Toccata | Rapid keys figuration over the Andalusian descent (Am–G–F–E) |
//! | 33–40  | Double harmonic | The motto re-intervaled (A–B♭–C#: the bent step), Neapolitan sway over the drone |
//! | 41–56  | Recall  | The adagio's aria returns, consoling, over Movement I's own harmony, with the waltz pulse in 3-against-4 augmentation |
//! | 57–88  | Apotheosis | B major, fortissimo: the motto's contour verbatim with the major third — the withheld Picardy and the final Lift paid at once; bVI–bVII–I arrivals |
//! | 89–100 | Coda    | Written augmentation over the tonic pedal, Mixolydian ♭VII plagal color, one Lydian #4 inflection, final cadence |
//! | 101–104| Tail    | Ring-out |

use super::finale_lead::{LEAD_APOTHEOSIS, LEAD_APOTHEOSIS_TWO, tube_harmony, tube_lead};
use crate::composition::{FLAT, TrackSpec, peak_mix, rms_mix, track};
use crate::render::Voice;
use crate::score::{Chord, Meter, Note, Part};

pub(crate) const METER: Meter = Meter::new(116.0, 4.0);
pub(crate) const TOTAL_BARS: usize = 104;

/// Weight arrives in stages: menace held back, the recall hushed, the
/// apotheosis and coda fully open.
const BED_RIDE: &[(f32, f32)] = &[
    (16.0, -2.0),
    (17.0, -1.0),
    (40.0, -1.0),
    (41.0, -3.0),
    (53.0, -2.0),
    (57.0, 0.0),
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
            string_voice(meter, 0),
            bed(-27.0, -0.7),
        ),
        track(
            "string-chord-mid",
            Voice::StringBow,
            string_voice(meter, 1),
            bed(-27.0, 0.15),
        ),
        track(
            "string-chord-high",
            Voice::StringBow,
            string_voice(meter, 2),
            bed(-27.0, 0.7),
        ),
        track(
            "string-bass",
            Voice::StringPick,
            string_bass(meter),
            rms_mix(-18.5, 0.0, 30.0, FLAT),
        ),
        track(
            "modal-arp",
            Voice::ModalKeys,
            modal_arp(meter),
            bed(-24.5, 0.5),
        ),
        track(
            "modal-bells",
            Voice::ModalBells,
            modal_bells(meter),
            rms_mix(-20.5, -0.5, 90.0, FLAT),
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
            rms_mix(-19.5, -0.1, 30.0, FLAT),
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

const AM: Chord = Chord {
    bass: 45,
    voices: [57, 60, 64],
};
/// Open fifth on A — the double-harmonic episode's drone floor leaves the
/// third out so the maqam line's C# never grinds against a C natural.
const A5: Chord = Chord {
    bass: 45,
    voices: [57, 64, 69],
};
/// The hexatonic pole of A minor: D♭ major shares no tones at all — the
/// uncanny oscillation of the menace section.
const DB: Chord = Chord {
    bass: 37,
    voices: [61, 65, 68],
};
const F: Chord = Chord {
    bass: 41,
    voices: [57, 60, 65],
};
const C: Chord = Chord {
    bass: 48,
    voices: [55, 60, 64],
};
const G_LOW: Chord = Chord {
    bass: 43,
    voices: [55, 59, 62],
};
const E: Chord = Chord {
    bass: 40,
    voices: [56, 59, 64],
};
/// Neapolitan B♭ over the double-harmonic drone.
const BB: Chord = Chord {
    bass: 46,
    voices: [58, 62, 65],
};
const BMAJ: Chord = Chord {
    bass: 47,
    voices: [59, 63, 66],
};
const G: Chord = Chord {
    bass: 43,
    voices: [59, 62, 67],
};
const A: Chord = Chord {
    bass: 45,
    voices: [61, 64, 69],
};
const EMAJ: Chord = Chord {
    bass: 40,
    voices: [59, 64, 68],
};

/// The chord per bar, all in concert pitch.
fn harmony() -> Vec<(usize, Chord)> {
    let mut bars: Vec<(usize, Chord)> = Vec::new();
    let mut push = |start: usize, chords: &[Chord]| {
        for (index, chord) in chords.iter().enumerate() {
            bars.push((start + index, *chord));
        }
    };
    // Menace: the pole oscillation, tightening, then the first dominant.
    push(1, &[AM, AM, DB, DB, AM, AM, DB, DB]);
    push(9, &[AM, DB, AM, DB, F, F, E, E]);
    // Toccata: the Andalusian descent — Movement III's lament family at speed.
    push(17, &[AM, G_LOW, F, E, AM, G_LOW, F, E]);
    push(25, &[AM, G_LOW, F, E, AM, F, E, E]);
    // Double-harmonic episode over the A drone.
    push(33, &[A5, BB, A5, BB, A5, BB, A5, E]);
    // Recall: Movement I's own progression under the adagio's aria.
    push(41, &[AM, F, C, G_LOW, AM, F, E, E]);
    push(49, &[AM, F, C, G_LOW, G_LOW, G_LOW, A, A]);
    // Apotheosis: B major; the motto sits on I and IV, and the bVI–bVII–I
    // cadences arrive at the phrase ends (69–71, 81–83, 93–95).
    push(57, &[BMAJ, EMAJ, BMAJ, BMAJ, EMAJ, BMAJ, BMAJ, BMAJ]);
    push(65, &[EMAJ, BMAJ, EMAJ, BMAJ, G, A, BMAJ, BMAJ]);
    push(73, &[BMAJ, G, A, BMAJ, BMAJ, EMAJ, A, BMAJ]);
    push(81, &[G, A, BMAJ, EMAJ, G, A, BMAJ, BMAJ]);
    // Coda over the tonic pedal.
    push(89, &[BMAJ, A, EMAJ, BMAJ, G, A, BMAJ, BMAJ]);
    push(97, &[BMAJ, BMAJ, BMAJ, BMAJ]);
    bars
}

// --- Strings -----------------------------------------------------------------

fn string_voice(meter: Meter, voice: usize) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord) in harmony() {
        let tone = chord.voices[voice];
        match bar {
            // Menace: long pole chords, swelling on each oscillation.
            1..=16 => part.hit(bar, 1.0, 3.8, tone, 0.46 + 0.01 * (bar - 1) as f32),
            // Toccata: driving offbeat stabs.
            17..=32 => {
                part.hit(bar, 1.5, 0.4, tone, 0.56);
                part.hit(bar, 3.5, 0.4, tone, 0.52);
            }
            33..=40 => part.hit(bar, 1.0, 3.8, tone, 0.54),
            // Recall: hushed pads.
            41..=56 => part.hit(bar, 1.0, 3.7, tone, 0.44),
            // Apotheosis and coda: full sustained wall.
            89..=96 => part.hit(bar, 1.0, 3.8, tone, 0.7),
            97..=100 => {
                if bar == 97 {
                    part.hit(bar, 1.0, 14.0, tone, 0.74);
                }
            }
            _ => part.hit(bar, 1.0, 3.8, tone, 0.66),
        }
    }
    part.into_notes()
}

fn string_bass(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord) in harmony() {
        match bar {
            // Menace: dotted heartbeat on the root.
            1..=16 => {
                part.hit(bar, 1.0, 1.4, chord.bass, 0.6);
                part.hit(bar, 3.5, 0.9, chord.bass, 0.5);
            }
            // Toccata: driving eighths.
            17..=40 => {
                for slot in 0..8 {
                    let accent = if slot == 0 || slot == 4 { 0.08 } else { 0.0 };
                    part.hit(bar, 1.0 + slot as f32 * 0.5, 0.4, chord.bass, 0.6 + accent);
                }
            }
            41..=52 => {
                part.hit(bar, 1.0, 1.9, chord.bass, 0.5);
                part.hit(bar, 3.0, 1.9, chord.bass, 0.46);
            }
            // The climb into the apotheosis: bVI–bVII walked up.
            53..=56 => {
                for beat in 0..4 {
                    part.hit(
                        bar,
                        1.0 + beat as f32,
                        0.85,
                        chord.bass,
                        0.58 + 0.04 * (bar - 53) as f32 + 0.02 * beat as f32,
                    );
                }
            }
            97..=100 => {
                if bar == 97 {
                    part.hit(bar, 1.0, 14.0, chord.bass, 0.8);
                }
            }
            // Apotheosis: quarter pulse with the fifth on the backbeat.
            _ => {
                part.hit(bar, 1.0, 0.9, chord.bass, 0.74);
                part.hit(bar, 2.0, 0.9, chord.bass, 0.64);
                part.hit(bar, 3.0, 0.9, chord.bass + 7, 0.68);
                part.hit(bar, 4.0, 0.9, chord.bass, 0.66);
            }
        }
    }
    part.into_notes()
}

// --- Keys and bells --------------------------------------------------------------

fn modal_arp(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord) in harmony() {
        match bar {
            // Menace: the sixteenth surface fades in halfway.
            1..=8 => {}
            9..=16 => sixteenth_surface(&mut part, bar, chord, 0.42 + 0.015 * (bar - 9) as f32),
            // Toccata: the organ role — full sixteenth figuration.
            17..=40 => sixteenth_surface(&mut part, bar, chord, 0.58),
            // Recall: the waltz ghost — a 3-beat cycle drifting across 4/4.
            41..=52 => {}
            53..=56 => sixteenth_surface(&mut part, bar, chord, 0.6),
            97..=100 => {
                if bar == 97 {
                    final_roll(&mut part, chord);
                }
            }
            _ => sixteenth_surface(&mut part, bar, chord, 0.64),
        }
    }
    // The waltz ghost under the recall: dotted-half strikes every 3 beats.
    waltz_ghost(&mut part);
    part.into_notes()
}

/// The relentless sixteenth ostinato: low-high rocking on chord tones.
fn sixteenth_surface(part: &mut Part, bar: usize, chord: Chord, velocity: f32) {
    let [low, mid, high] = chord.voices;
    let tones = [
        low + 12,
        high + 12,
        mid + 12,
        high + 12,
        low + 24,
        high + 12,
        mid + 12,
        high + 12,
    ];
    for slot in 0..16 {
        let accent = if slot % 4 == 0 { 0.06 } else { 0.0 };
        part.hit(
            bar,
            1.0 + slot as f32 * 0.25,
            0.55,
            tones[slot % 8],
            velocity + accent,
        );
    }
}

/// The recall's waltz memory: soft chord strikes every 3 beats from bar 41,
/// drifting against the 4/4 — the waltz rhythm in augmentation.
fn waltz_ghost(part: &mut Part) {
    let tones = [57u8, 60, 64, 65];
    let mut beat_index = 0usize;
    loop {
        let absolute = beat_index * 3;
        let bar = 41 + absolute / 4;
        if bar > 52 {
            break;
        }
        let beat = 1.0 + (absolute % 4) as f32;
        part.hit(bar, beat, 2.4, tones[beat_index % 4] + 12, 0.4);
        beat_index += 1;
    }
}

fn final_roll(part: &mut Part, chord: Chord) {
    let [low, mid, high] = chord.voices;
    for (slot, tone) in [low + 12, mid + 12, high + 12, low + 24, mid + 24, high + 24]
        .iter()
        .enumerate()
    {
        part.hit(97, 1.0 + slot as f32 * 0.25, 6.0, *tone, 0.74);
    }
}

/// Bells: pole tolls in the menace, octave doubling of the apotheosis theme's
/// arrivals, the final high tonic left ringing.
fn modal_bells(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in [3usize, 7, 10, 12] {
        part.hit(bar, 1.0, 3.0, 80, 0.42);
    }
    for (bar, beat, duration, midi, velocity) in LEAD_APOTHEOSIS {
        if duration > 3.0 {
            part.hit(bar, beat, duration, midi + 12, velocity - 0.18);
        }
    }
    for (bar, beat, duration, midi, velocity) in LEAD_APOTHEOSIS_TWO {
        if duration > 3.0 {
            part.hit(bar, beat, duration, midi + 12, velocity - 0.18);
        }
    }
    part.hit(97, 1.0, 8.0, 83, 0.66);
    part.hit(97, 1.5, 8.0, 90, 0.6);
    part.hit(97, 2.0, 8.0, 95, 0.54);
    part.into_notes()
}

// --- Cymbals ----------------------------------------------------------------------

fn cymbal_ride(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in 17..=40 {
        for slot in 0..8 {
            let accent = if slot == 0 || slot == 4 { 0.06 } else { 0.0 };
            part.hit(bar, 1.0 + slot as f32 * 0.5, 0.45, 60, 0.36 + accent);
        }
    }
    for bar in 57..=96 {
        for beat in 0..4 {
            let velocity = if beat == 0 { 0.5 } else { 0.4 };
            part.hit(bar, 1.0 + beat as f32, 0.9, 60, velocity);
        }
    }
    part.into_notes()
}

/// The heartbeat of the menace, the heavy quarters of the apotheosis.
fn cymbal_tom(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in 1..=16 {
        part.hit(bar, 1.0, 0.6, 60, 0.6);
        part.hit(bar, 3.5, 0.6, 60, 0.46);
    }
    for bar in 17..=40 {
        part.hit(bar, 1.0, 0.5, 60, 0.66);
        part.hit(bar, 2.5, 0.5, 60, 0.56);
        part.hit(bar, 3.0, 0.5, 60, 0.6);
        part.hit(bar, 4.0, 0.5, 60, 0.62);
    }
    for bar in 53..=56 {
        for beat in 0..4 {
            part.hit(
                bar,
                1.0 + beat as f32,
                0.6,
                60,
                0.6 + 0.04 * (bar - 53) as f32,
            );
        }
    }
    for bar in 57..=96 {
        part.hit(bar, 1.0, 0.6, 60, 0.74);
        part.hit(bar, 2.0, 0.6, 60, 0.62);
        part.hit(bar, 3.0, 0.6, 60, 0.68);
        part.hit(bar, 4.0, 0.6, 60, 0.64);
    }
    part.hit(97, 1.0, 1.0, 60, 0.8);
    part.into_notes()
}

fn cymbal_crash(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, velocity) in [
        (17, 0.6),
        (33, 0.62),
        (41, 0.4),
        (73, 0.74),
        (89, 0.7),
        (93, 0.72),
    ] {
        part.hit(bar, 1.0, 6.0, 60, velocity);
    }
    // The swell into the Lift, and the arrival itself.
    for slot in 0..8 {
        part.hit(
            56,
            1.0 + slot as f32 * 0.5,
            0.45,
            60,
            0.12 + 0.05 * slot as f32,
        );
    }
    part.hit(57, 1.0, 6.0, 60, 0.78);
    part.hit(97, 1.0, 14.0, 60, 0.88);
    part.into_notes()
}
