//! Movement V — "Finale": the tube lead and counter-voice material.
//!
//! The menace fragments, toccata, double-harmonic statement, recall aria,
//! and apotheosis theme live here so the form/rhythm side of the score
//! ([`super::finale`]) stays within the file size lint.

use crate::composition::{PhraseNote, push_phrase};
use crate::score::{Meter, Note, Part};

/// Menace: motto fragments, low and halting (the boss circling). The
/// fragments answer the pole oscillation: A-minor bars get the motto, the
/// D♭ bars only its echo on the pole's own tones (C#/F).
const LEAD_MENACE: [PhraseNote; 10] = [
    (5, 1.0, 1.92, 57, 0.46),
    (5, 3.0, 1.42, 59, 0.44),
    (6, 1.0, 2.92, 60, 0.48),
    (7, 1.0, 1.92, 61, 0.5),
    (7, 3.0, 1.42, 65, 0.46),
    (8, 1.0, 2.92, 61, 0.46),
    (11, 1.0, 0.92, 64, 0.52),
    (12, 1.0, 0.92, 65, 0.54),
    (15, 1.0, 3.92, 64, 0.56),
    (16, 1.0, 3.42, 64, 0.54),
];

/// Toccata: the motto in diminution over the Andalusian descent — the
/// battle's riff grammar at the boss's heavier tread.
const LEAD_TOCCATA: [PhraseNote; 22] = [
    (17, 1.0, 0.45, 69, 0.62),
    (17, 1.5, 0.45, 71, 0.58),
    (17, 2.0, 0.92, 72, 0.64),
    (17, 3.0, 0.92, 72, 0.58),
    (17, 4.0, 0.92, 69, 0.56),
    (18, 1.0, 1.42, 71, 0.6),
    (18, 2.5, 0.42, 69, 0.54),
    (18, 3.0, 1.92, 67, 0.56),
    (19, 1.0, 0.92, 65, 0.58),
    (19, 2.0, 0.92, 67, 0.56),
    (19, 3.0, 1.92, 69, 0.6),
    (20, 1.0, 1.92, 68, 0.6),
    (20, 3.0, 1.42, 64, 0.54),
    (21, 1.0, 0.45, 69, 0.64),
    (21, 1.5, 0.45, 71, 0.6),
    (21, 2.0, 0.92, 72, 0.66),
    (21, 3.0, 0.92, 74, 0.62),
    (21, 4.0, 0.92, 72, 0.6),
    (22, 1.0, 2.92, 71, 0.62),
    (23, 1.0, 1.92, 72, 0.62),
    (23, 3.0, 1.92, 71, 0.6),
    (24, 1.0, 3.42, 68, 0.58),
];

/// The double-harmonic statement: A–B♭–C# — the motto's step bent to an
/// augmented 2nd, answered with the ♭2→1 close.
const LEAD_DOUBLE_HARMONIC: [PhraseNote; 12] = [
    (33, 1.0, 1.42, 69, 0.6),
    (33, 2.5, 0.42, 70, 0.58),
    (33, 3.0, 1.92, 73, 0.64),
    (34, 1.0, 0.92, 74, 0.62),
    (34, 2.0, 0.92, 73, 0.6),
    (34, 3.0, 1.92, 70, 0.58),
    (35, 1.0, 1.42, 69, 0.6),
    (35, 2.5, 0.42, 70, 0.6),
    (35, 3.0, 0.92, 73, 0.64),
    (35, 4.0, 0.92, 74, 0.62),
    (36, 1.0, 1.92, 76, 0.66),
    (36, 3.0, 1.92, 73, 0.62),
];

/// Recall: the adagio's aria at home in A minor, consoling — each strong
/// beat an appoggiatura resolving onto the chord of its bar (Am F C G).
const LEAD_RECALL: [PhraseNote; 14] = [
    (41, 1.0, 1.0, 71, 0.48),
    (41, 2.0, 2.92, 69, 0.46),
    (42, 1.0, 1.0, 74, 0.5),
    (42, 2.0, 2.92, 72, 0.48),
    (43, 1.0, 1.0, 77, 0.52),
    (43, 2.0, 2.92, 76, 0.5),
    (44, 1.0, 1.92, 74, 0.5),
    (44, 3.0, 1.42, 71, 0.48),
    (45, 1.0, 1.0, 72, 0.5),
    (45, 2.0, 2.92, 69, 0.48),
    (46, 1.0, 1.0, 74, 0.5),
    (46, 2.0, 2.92, 72, 0.48),
    (47, 1.0, 2.92, 68, 0.48),
    (48, 1.0, 3.42, 64, 0.46),
];

/// The apotheosis theme: the Ascent motto with the major third, B major,
/// in 2x augmentation — the contour everyone has heard since bar 5 of the
/// prelude, finally in the light.
pub(super) const LEAD_APOTHEOSIS: [PhraseNote; 17] = [
    (57, 1.0, 3.92, 71, 0.76),
    (58, 1.0, 3.92, 73, 0.78),
    (59, 1.0, 3.92, 75, 0.8),
    (60, 1.0, 3.42, 75, 0.76),
    (61, 1.0, 1.92, 76, 0.78),
    (61, 3.0, 1.92, 75, 0.76),
    (62, 1.0, 1.92, 73, 0.74),
    (62, 3.0, 1.92, 75, 0.76),
    (63, 1.0, 3.92, 71, 0.76),
    (64, 1.0, 3.42, 71, 0.74),
    (65, 1.0, 3.92, 76, 0.78),
    (66, 1.0, 3.92, 75, 0.78),
    (67, 1.0, 3.92, 78, 0.8),
    (68, 1.0, 3.42, 75, 0.76),
    (69, 1.0, 3.92, 74, 0.76),
    (70, 1.0, 3.92, 73, 0.76),
    (71, 1.0, 7.42, 71, 0.78),
];

/// Statement two climbs to the high tonic, with the single Lydian #4 (E#)
/// inflection on the way — the radiance note.
pub(super) const LEAD_APOTHEOSIS_TWO: [PhraseNote; 14] = [
    (73, 1.0, 3.92, 78, 0.8),
    (74, 1.0, 3.92, 79, 0.78),
    (75, 1.0, 3.92, 81, 0.82),
    (76, 1.0, 3.42, 78, 0.78),
    (77, 1.0, 1.92, 76, 0.76),
    (77, 3.0, 0.92, 77, 0.78),
    (77, 4.0, 0.92, 78, 0.8),
    (78, 1.0, 3.92, 80, 0.8),
    (79, 1.0, 3.92, 81, 0.82),
    (80, 1.0, 3.42, 83, 0.84),
    (81, 1.0, 3.92, 79, 0.8),
    (82, 1.0, 3.92, 81, 0.82),
    (83, 1.0, 7.42, 83, 0.85),
    (85, 1.0, 3.92, 79, 0.8),
];

/// Coda: written augmentation — the motto in whole notes over the pedal,
/// the Mixolydian A-natural giving the plagal glow.
const LEAD_CODA: [PhraseNote; 8] = [
    (86, 1.0, 3.92, 81, 0.8),
    (87, 1.0, 7.42, 83, 0.84),
    (89, 1.0, 3.92, 71, 0.74),
    (90, 1.0, 3.92, 73, 0.76),
    (91, 1.0, 3.92, 75, 0.78),
    (92, 1.0, 3.92, 76, 0.76),
    (93, 1.0, 3.92, 79, 0.78),
    (94, 1.0, 3.92, 81, 0.8),
];

pub(super) fn tube_lead(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    push_phrase(&mut part, &LEAD_MENACE, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_TOCCATA, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_TOCCATA, 8, 24);
    push_phrase(&mut part, &LEAD_DOUBLE_HARMONIC, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_DOUBLE_HARMONIC, 4, 36);
    push_phrase(&mut part, &LEAD_RECALL, 0, usize::MAX);
    // The climb: bVI–bVII under a rising line into the Lift.
    part.hit(53, 1.0, 3.92, 67, 0.6);
    part.hit(54, 1.0, 3.92, 69, 0.66);
    part.hit(55, 1.0, 1.92, 71, 0.7);
    part.hit(55, 3.0, 1.92, 73, 0.72);
    part.hit(56, 1.0, 3.42, 75, 0.74);
    push_phrase(&mut part, &LEAD_APOTHEOSIS, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_APOTHEOSIS_TWO, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_CODA, 0, usize::MAX);
    part.hit(95, 1.0, 3.92, 80, 0.78);
    part.hit(96, 1.0, 1.92, 78, 0.76);
    part.hit(96, 3.0, 1.92, 80, 0.78);
    part.hit(97, 1.0, 12.0, 83, 0.85);
    part.into_notes()
}

/// The dark tube: the menace's low pole tones, a counter-sixth under the
/// recall, sustained thirds under the apotheosis.
pub(super) fn tube_harmony(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    // Drone tones matched to the pole bars: the 5th over A minor, the 3rd
    // (F = enharmonic floor) over D♭, the roots under the F/E arrival.
    const MENACE_DRONES: [(usize, u8); 8] = [
        (1, 52),
        (3, 53),
        (5, 52),
        (7, 53),
        (9, 52),
        (11, 52),
        (13, 53),
        (15, 52),
    ];
    for (bar, tone) in MENACE_DRONES {
        part.hit(bar, 1.0, 7.8, tone, 0.44);
    }
    for bar in [33usize, 35, 37, 39] {
        part.hit(bar, 1.0, 7.8, 45, 0.46);
    }
    const COUNTER_RECALL: [PhraseNote; 6] = [
        (41, 1.0, 3.8, 64, 0.42),
        (43, 1.0, 3.8, 67, 0.44),
        (45, 1.0, 3.8, 64, 0.44),
        (47, 1.0, 3.8, 64, 0.42),
        (49, 1.0, 3.8, 64, 0.42),
        (51, 1.0, 3.8, 62, 0.4),
    ];
    push_phrase(&mut part, &COUNTER_RECALL, 0, usize::MAX);
    const COUNTER_APOTHEOSIS: [PhraseNote; 8] = [
        (57, 1.0, 3.92, 66, 0.56),
        (59, 1.0, 3.92, 68, 0.58),
        (61, 1.0, 3.92, 68, 0.58),
        (63, 1.0, 3.92, 66, 0.56),
        (73, 1.0, 3.92, 75, 0.6),
        (75, 1.0, 3.92, 76, 0.62),
        (83, 1.0, 7.42, 78, 0.62),
        (87, 1.0, 7.42, 78, 0.62),
    ];
    push_phrase(&mut part, &COUNTER_APOTHEOSIS, 0, usize::MAX);
    part.hit(97, 1.0, 12.0, 75, 0.62);
    part.into_notes()
}
