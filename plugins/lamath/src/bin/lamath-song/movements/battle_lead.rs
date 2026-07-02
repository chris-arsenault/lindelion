//! Movement II — "Battle": the tube lead and counter-voice material.
//!
//! The riff tables and phrase data for the battle movement live here so the
//! form/rhythm side of the score ([`super::battle`]) stays within the file
//! size lint; the two sides are one movement.

use super::battle::stop_time_hits;
use crate::composition::{PhraseNote, push_phrase};
use crate::score::{Meter, Note, Part};

/// One motto-in-diminution riff bar: 1–2–3 then 3–4–3–1 compressed into
/// eighths. `notes` are the seven pitches of the figure low-to-contour.
fn riff_bar(part: &mut Part, bar: usize, notes: [u8; 7], velocity: f32) {
    const BEATS: [f32; 7] = [1.0, 1.5, 2.0, 3.0, 3.5, 4.0, 4.5];
    const DURS: [f32; 7] = [0.45, 0.45, 0.92, 0.45, 0.45, 0.45, 0.45];
    const ACCENTS: [f32; 7] = [0.06, -0.04, 0.08, 0.0, 0.02, 0.0, -0.04];
    for index in 0..7 {
        part.hit(
            bar,
            BEATS[index],
            DURS[index],
            notes[index],
            (velocity + ACCENTS[index]).clamp(0.0, 1.0),
        );
    }
}

/// Motto riff on the B-minor tonic (B4 base).
const RIFF_BM: [u8; 7] = [71, 73, 74, 74, 76, 74, 71];
/// Motto riff sequenced onto the third (D5 base).
const RIFF_D: [u8; 7] = [74, 76, 78, 78, 79, 78, 74];
/// Motto riff restated +1 in C minor — the shock.
const RIFF_CM: [u8; 7] = [72, 74, 75, 75, 77, 75, 72];
/// Motto riff bent through the Hijaz tetrachord (F#–G–A#–B): the augmented
/// 2nd between scale degrees 2 and 3 is the maqam color.
const RIFF_HIJAZ: [u8; 7] = [66, 67, 70, 70, 71, 70, 66];

/// A-theme answer phrases (section-local bars; the riff bars are generated).
const LEAD_A_FILL: [PhraseNote; 12] = [
    (2, 1.0, 1.42, 79, 0.70),
    (2, 2.5, 0.42, 78, 0.62),
    (2, 3.0, 1.92, 74, 0.66),
    (3, 1.0, 0.92, 76, 0.70),
    (3, 2.0, 0.45, 74, 0.60),
    (3, 2.5, 0.45, 73, 0.60),
    (3, 3.0, 1.92, 76, 0.68),
    (4, 1.0, 1.45, 71, 0.70),
    (4, 4.0, 0.45, 69, 0.60),
    (4, 4.5, 0.45, 71, 0.64),
    (8, 1.0, 1.92, 70, 0.72),
    (8, 3.0, 1.42, 73, 0.68),
];

/// Second-half A-theme fill (bars 5–7 section-local): climb to the high B.
const LEAD_A_CLIMB: [PhraseNote; 6] = [
    (6, 1.0, 1.42, 79, 0.70),
    (6, 2.5, 0.42, 81, 0.66),
    (6, 3.0, 1.92, 83, 0.72),
    (7, 1.0, 0.92, 79, 0.70),
    (7, 2.0, 0.92, 76, 0.66),
    (7, 3.0, 1.92, 79, 0.68),
];

/// A-theme back half (bars 10–16 section-local): high answer and the seam
/// bar truncated at beat 3.5 with the anticipation pickup on 4.5.
const LEAD_A_BACK: [PhraseNote; 13] = [
    (10, 1.0, 1.42, 83, 0.78),
    (10, 2.5, 0.42, 81, 0.70),
    (10, 3.0, 1.92, 78, 0.70),
    (11, 1.0, 0.92, 81, 0.74),
    (11, 2.0, 0.45, 79, 0.66),
    (11, 2.5, 0.45, 78, 0.64),
    (11, 3.0, 1.92, 81, 0.72),
    (12, 1.0, 2.92, 83, 0.78),
    (13, 1.0, 1.92, 78, 0.72),
    (13, 3.0, 1.92, 74, 0.66),
    (14, 1.0, 1.42, 79, 0.70),
    (14, 2.5, 0.42, 81, 0.64),
    (14, 3.0, 1.42, 79, 0.66),
];

/// Seam bar (16 section-local): the dropped eighth. Material stops at 3.42;
/// the next section's pickup lands on 4.5.
const LEAD_A_SEAM: [PhraseNote; 5] = [
    (15, 1.0, 0.92, 76, 0.68),
    (15, 2.0, 0.92, 74, 0.62),
    (15, 3.0, 1.42, 76, 0.66),
    (16, 1.0, 0.92, 73, 0.70),
    (16, 2.0, 0.92, 70, 0.68),
];

/// B-section lyrical melody: the motto in 2x augmentation over the Royal
/// Road, one arch with its peak two-thirds through.
const LEAD_B: [PhraseNote; 30] = [
    (1, 1.0, 1.92, 74, 0.60),
    (1, 3.0, 1.92, 76, 0.62),
    (2, 1.0, 2.92, 78, 0.66),
    (2, 4.0, 0.92, 79, 0.60),
    (3, 1.0, 1.92, 81, 0.68),
    (3, 3.0, 1.92, 78, 0.62),
    (4, 1.0, 3.42, 74, 0.60),
    (5, 1.0, 1.92, 74, 0.60),
    (5, 3.0, 1.92, 76, 0.62),
    (6, 1.0, 2.92, 79, 0.66),
    (6, 4.0, 0.92, 81, 0.62),
    (7, 1.0, 1.92, 83, 0.70),
    (7, 3.0, 1.92, 81, 0.64),
    (8, 1.0, 3.42, 78, 0.62),
    (9, 1.0, 0.92, 79, 0.64),
    (9, 2.0, 0.92, 81, 0.66),
    (9, 3.0, 1.92, 83, 0.70),
    (10, 1.0, 1.92, 81, 0.66),
    (10, 3.0, 1.92, 79, 0.62),
    (11, 1.0, 2.92, 78, 0.64),
    (12, 1.0, 3.42, 74, 0.60),
    (13, 1.0, 1.92, 83, 0.72),
    (13, 3.0, 1.92, 81, 0.66),
    (14, 1.0, 1.92, 79, 0.64),
    (14, 3.0, 1.92, 78, 0.62),
    (15, 1.0, 1.92, 76, 0.62),
    (15, 3.0, 0.92, 74, 0.60),
    (16, 1.0, 1.92, 70, 0.66),
    (16, 3.0, 1.42, 73, 0.66),
    (16, 4.5, 0.42, 66, 0.62),
];

/// Hijaz episode fills between riff statements (section-local bars).
const LEAD_HIJAZ_FILL: [PhraseNote; 10] = [
    (2, 1.0, 1.42, 71, 0.66),
    (2, 2.5, 0.42, 70, 0.60),
    (2, 3.0, 1.92, 67, 0.60),
    (4, 1.0, 1.42, 71, 0.64),
    (4, 3.0, 1.92, 70, 0.62),
    (6, 1.0, 0.92, 73, 0.68),
    (6, 2.0, 0.92, 74, 0.66),
    (6, 3.0, 1.92, 76, 0.70),
    (8, 1.0, 1.92, 78, 0.70),
    (8, 3.0, 1.92, 73, 0.64),
];

/// Hijaz climb out (bars 13–16 section-local) toward the C-minor shock.
const LEAD_HIJAZ_OUT: [PhraseNote; 8] = [
    (13, 1.0, 0.92, 70, 0.66),
    (13, 2.0, 0.92, 71, 0.66),
    (13, 3.0, 1.92, 73, 0.68),
    (14, 1.0, 1.92, 74, 0.70),
    (14, 3.0, 1.92, 76, 0.70),
    (15, 1.0, 3.92, 78, 0.74),
    (16, 1.0, 1.92, 78, 0.74),
    (16, 3.0, 1.92, 73, 0.68),
];

/// Shock fills over bVI/bVII of C minor, then stop-time on F#.
const LEAD_SHOCK_FILL: [PhraseNote; 13] = [
    (2, 1.0, 1.42, 80, 0.72),
    (2, 2.5, 0.42, 79, 0.64),
    (2, 3.0, 1.92, 75, 0.66),
    (3, 1.0, 0.92, 77, 0.70),
    (3, 2.0, 0.92, 75, 0.64),
    (3, 3.0, 1.92, 74, 0.66),
    (4, 1.0, 2.92, 72, 0.70),
    (6, 1.0, 1.42, 80, 0.74),
    (6, 2.5, 0.42, 79, 0.66),
    (6, 3.0, 1.92, 75, 0.68),
    (7, 1.0, 0.45, 78, 0.80),
    (7, 2.5, 0.45, 78, 0.76),
    (7, 4.0, 0.92, 78, 0.78),
];

/// Rideout anthem: long bVI–bVII–i tones, then hemiola stabs.
const LEAD_RIDEOUT: [PhraseNote; 9] = [
    (1, 1.0, 3.92, 79, 0.74),
    (2, 1.0, 3.92, 81, 0.76),
    (3, 1.0, 7.42, 83, 0.80),
    (5, 1.0, 3.92, 79, 0.76),
    (6, 1.0, 3.92, 81, 0.78),
    (7, 1.0, 7.42, 83, 0.82),
    (9, 1.0, 1.42, 79, 0.76),
    (10, 1.0, 1.42, 81, 0.78),
    (12, 1.0, 3.42, 78, 0.78),
];

pub(super) fn tube_lead(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    // A theme (bars 9–24) and its anticipation pickup into A'.
    push_a_theme(&mut part, 8, 0.0);
    part.hit(24, 4.5, 0.45, 71, 0.74);
    // A' (25–40), one notch hotter.
    push_a_theme(&mut part, 24, 0.04);
    // B section (41–56).
    push_phrase(&mut part, &LEAD_B, 40, usize::MAX);
    // Hijaz episode (57–72): riff statements + fills + climb out.
    for bar in [57, 59, 61, 63, 65, 67] {
        riff_bar(&mut part, bar, RIFF_HIJAZ, 0.64);
    }
    push_phrase(&mut part, &LEAD_HIJAZ_FILL, 56, usize::MAX);
    push_phrase(&mut part, &LEAD_HIJAZ_OUT, 56, usize::MAX);
    // Shock (73–80): the riff +1 in C minor, then stop-time on F#.
    riff_bar(&mut part, 73, RIFF_CM, 0.74);
    riff_bar(&mut part, 77, RIFF_CM, 0.76);
    push_phrase(&mut part, &LEAD_SHOCK_FILL, 72, usize::MAX);
    part.hit(80, 1.0, 0.45, 78, 0.80);
    part.hit(80, 2.5, 1.92, 70, 0.78);
    // A return (81–96) at full tilt and its pickup into the rideout.
    push_a_theme(&mut part, 80, 0.06);
    part.hit(96, 4.5, 0.45, 79, 0.78);
    // Rideout (97–108).
    push_phrase(&mut part, &LEAD_RIDEOUT, 96, usize::MAX);
    // Coda (109–116): held high B over the ladder, then unison stabs.
    part.hit(109, 1.0, 7.92, 83, 0.80);
    riff_bar(&mut part, 111, RIFF_BM, 0.78);
    part.hit(112, 1.0, 3.92, 83, 0.82);
    for bar in [113, 114, 115] {
        stop_time_hits(&mut part, bar, 83, 0.84);
    }
    part.hit(116, 1.0, 5.0, 83, 0.85);
    part.into_notes()
}

/// One full 16-bar A statement starting after `bar_offset` (riff bars 1, 5,
/// and 9 section-local are generated; fills come from the phrase tables).
fn push_a_theme(part: &mut Part, bar_offset: usize, heat: f32) {
    riff_bar(part, bar_offset + 1, RIFF_BM, 0.66 + heat);
    push_phrase(part, &LEAD_A_FILL, bar_offset, usize::MAX);
    riff_bar(part, bar_offset + 5, RIFF_BM, 0.68 + heat);
    push_phrase(part, &LEAD_A_CLIMB, bar_offset, usize::MAX);
    riff_bar(part, bar_offset + 9, RIFF_D, 0.70 + heat);
    push_phrase(part, &LEAD_A_BACK, bar_offset, usize::MAX);
    push_phrase(part, &LEAD_A_SEAM, bar_offset, usize::MAX);
}

/// Counter-lines: thirds under the B-section melody, sustained pads in the
/// Hijaz episode (the open-fifth drone color), tacet elsewhere.
pub(super) fn tube_harmony(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    const HARMONY_B: [PhraseNote; 14] = [
        (1, 1.0, 1.92, 71, 0.50),
        (1, 3.0, 1.92, 73, 0.52),
        (2, 1.0, 3.42, 74, 0.54),
        (3, 1.0, 1.92, 78, 0.56),
        (3, 3.0, 1.92, 74, 0.52),
        (4, 1.0, 3.42, 71, 0.50),
        (5, 1.0, 1.92, 71, 0.50),
        (5, 3.0, 1.92, 73, 0.52),
        (6, 1.0, 3.42, 76, 0.54),
        (7, 1.0, 1.92, 79, 0.58),
        (7, 3.0, 1.92, 78, 0.54),
        (8, 1.0, 3.42, 74, 0.52),
        (13, 1.0, 3.92, 78, 0.56),
        (14, 1.0, 3.92, 74, 0.52),
    ];
    push_phrase(&mut part, &HARMONY_B, 40, usize::MAX);
    // Hijaz drone: low F# and C# held in two-bar breaths.
    for bar in [57, 59, 61, 63, 65, 67, 69, 71] {
        part.hit(bar, 1.0, 7.8, 54, 0.46);
    }
    // Rideout pads a sixth under the anthem.
    const HARMONY_RIDEOUT: [PhraseNote; 6] = [
        (1, 1.0, 3.92, 74, 0.52),
        (2, 1.0, 3.92, 76, 0.54),
        (3, 1.0, 7.42, 78, 0.56),
        (5, 1.0, 3.92, 74, 0.54),
        (6, 1.0, 3.92, 76, 0.56),
        (7, 1.0, 7.42, 78, 0.58),
    ];
    push_phrase(&mut part, &HARMONY_RIDEOUT, 96, usize::MAX);
    part.hit(116, 1.0, 5.0, 74, 0.6);
    part.into_notes()
}
