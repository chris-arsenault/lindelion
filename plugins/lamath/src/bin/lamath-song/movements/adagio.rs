//! Movement III — "Adagio": a lament in D minor at 58 BPM.
//!
//! A passacaglia over the descending chromatic tetrachord (D–C#–C–B–B♭–A,
//! the *passus duriusculus*), harmonized i – VI6 – iv6 – V (the Andalusian
//! family). The tube sings an operatic line — stepwise, appoggiatura on the
//! strong beats, written breaths, one peak note placed two-thirds through —
//! carrying Movement I's Ascent motto in 4x augmentation.
//!
//! | Bars  | Cycle | Material |
//! | ----- | ----- | -------- |
//! | 1–4   | 1 | Ground bass alone over a low tube drone |
//! | 5–8   | 2 | Strings enter; the augmented motto rises in the lead |
//! | 9–16  | 3–4 | Aria phrases 1–2, appoggiatura-saturated |
//! | 17–20 | 5 | Keys interlude — broken chords take the foreground |
//! | 21–24 | 6 | Hijaz cycle: dominant pedal, the aria bent through A–B♭–C#–D |
//! | 25–28 | 7 | Aria phrase 3: the sixth leap, recovered by step |
//! | 29–32 | 8 | Climax: omnibus wedge under the held peak A5 |
//! | 33–36 | 9 | Denouement, the ground resumes |
//! | 37–40 | 10 | Picardy tease (D major) — withheld: deceptive close on B♭ |
//! | 41–44 | — | Tail ring-out |

use crate::composition::{FLAT, Level, MixSpec, PhraseNote, TrackSpec, push_phrase, track};
use crate::render::Voice;
use crate::score::{Meter, Note, Part};

pub(crate) const METER: Meter = Meter::new(58.0, 4.0);
pub(crate) const TOTAL_BARS: usize = 44;

/// The bed breathes with the form: back through the early cycles, open at the
/// climax, easing for the deceptive close.
const BED_RIDE: &[(f32, f32)] = &[
    (8.0, -2.5),
    (25.0, -1.5),
    (29.0, 0.0),
    (35.0, 0.0),
    (37.0, -1.0),
];

pub(crate) fn tracks(meter: Meter) -> Vec<TrackSpec> {
    vec![
        track("tube-lead", Voice::TubeLead, tube_lead(meter), lead_mix()),
        track(
            "tube-harmony",
            Voice::TubeDark,
            tube_harmony(meter),
            bed_mix(-23.0, -0.25),
        ),
        track(
            "string-chord-low",
            Voice::StringBow,
            string_voice_low(meter),
            bed_mix(-28.0, -0.7),
        ),
        track(
            "string-chord-mid",
            Voice::StringBow,
            string_voice_mid(meter),
            bed_mix(-28.0, 0.15),
        ),
        track(
            "string-chord-high",
            Voice::StringBow,
            string_voice_high(meter),
            bed_mix(-28.0, 0.7),
        ),
        track(
            "string-bass",
            Voice::StringPick,
            ground_bass(meter),
            bass_mix(),
        ),
        track(
            "modal-arp",
            Voice::ModalKeys,
            modal_arp(meter),
            bed_mix(-26.0, 0.5),
        ),
        track(
            "modal-bells",
            Voice::ModalBells,
            modal_bells(meter),
            bells_mix(),
        ),
        track(
            "cymbal-ride",
            Voice::CymbalRide,
            cymbal_ride(meter),
            bed_mix(-27.0, 0.4),
        ),
        track(
            "cymbal-crash",
            Voice::CymbalCrash,
            cymbal_crash(meter),
            crash_mix(),
        ),
    ]
}

fn lead_mix() -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(-16.5),
        pan: 0.0,
        highpass_hz: 90.0,
        ride: FLAT,
    }
}

fn bed_mix(target_rms_dbfs: f32, pan: f32) -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(target_rms_dbfs),
        pan,
        highpass_hz: 90.0,
        ride: BED_RIDE,
    }
}

fn bass_mix() -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(-20.0),
        pan: 0.0,
        highpass_hz: 30.0,
        ride: FLAT,
    }
}

fn bells_mix() -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(-22.0),
        pan: -0.4,
        highpass_hz: 90.0,
        ride: FLAT,
    }
}

fn crash_mix() -> MixSpec {
    MixSpec {
        level: Level::Peak(-12.0),
        pan: 0.0,
        highpass_hz: 130.0,
        ride: FLAT,
    }
}

// --- The ground (passacaglia bass) ---------------------------------------------

/// Cycle start bars for the regular ground statements (the omnibus cycle at
/// 29 and the deceptive cycle at 37 write their own bass).
const GROUND_CYCLES: [usize; 8] = [1, 5, 9, 13, 17, 21, 25, 33];

/// One 4-bar ground statement: D–C# | C–B | B♭–A in halves, then the V pedal.
/// The Hijaz cycle (bar 21) holds the dominant pedal throughout instead.
fn ground_cycle(part: &mut Part, start: usize, velocity: f32) {
    if start == 21 {
        for bar in start..start + 4 {
            part.hit(bar, 1.0, 1.9, 33, velocity);
            part.hit(bar, 3.0, 1.9, 33, velocity - 0.05);
        }
        return;
    }
    const WALK: [(usize, f32, u8); 6] = [
        (0, 1.0, 38),
        (0, 3.0, 37),
        (1, 1.0, 36),
        (1, 3.0, 35),
        (2, 1.0, 34),
        (2, 3.0, 33),
    ];
    for (bar_offset, beat, note) in WALK {
        part.hit(start + bar_offset, beat, 1.9, note, velocity);
    }
    part.hit(start + 3, 1.0, 3.9, 33, velocity);
}

fn ground_bass(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for start in GROUND_CYCLES {
        let velocity = match start {
            1 => 0.5,
            25 => 0.6,
            _ => 0.55,
        };
        ground_cycle(&mut part, start, velocity);
    }
    // Omnibus cycle: chromatic descent from the dominant, halves.
    for (slot, note) in [33u8, 32, 31, 30, 29, 28].iter().enumerate() {
        let bar = 29 + slot / 2;
        let beat = 1.0 + 2.0 * (slot % 2) as f32;
        part.hit(bar, beat, 1.9, *note, 0.58);
    }
    part.hit(32, 1.0, 3.9, 33, 0.6);
    // Deceptive cycle: D pedal, then the bVI floor.
    part.hit(37, 1.0, 3.9, 38, 0.52);
    part.hit(38, 1.0, 3.9, 38, 0.5);
    part.hit(39, 1.0, 7.9, 34, 0.55);
    part.into_notes()
}

// --- Strings (lament harmonization + the omnibus wedge) --------------------------

/// (bar, beat, duration, note, velocity) rows per string voice. The lament
/// cycles harmonize i – VI6 – iv6 – V; the omnibus cycle moves the outer
/// voices in contrary chromatic motion around the held dominant common tone.
fn string_rows(voice: usize) -> Vec<PhraseNote> {
    let mut rows: Vec<PhraseNote> = Vec::new();
    // Lament cycles with strings (from cycle 2 on).
    for start in [5usize, 9, 13, 17, 21, 25, 33] {
        let velocity = if start == 25 { 0.56 } else { 0.5 };
        let chords: [[u8; 3]; 4] = if start == 21 {
            // Hijaz cycle: V, bII over the pedal (Neapolitan color), V7, V.
            [[57, 61, 64], [58, 62, 65], [55, 61, 64], [57, 61, 64]]
        } else {
            [[57, 62, 65], [57, 60, 65], [58, 62, 67], [57, 61, 64]]
        };
        for (bar_offset, chord) in chords.iter().enumerate() {
            rows.push((
                start + bar_offset,
                1.0,
                3.8,
                chord[voice],
                velocity + 0.02 * bar_offset as f32,
            ));
        }
    }
    // Omnibus wedge (bars 29–31): high voice rises chromatically, low voice
    // falls, mid holds the dominant; resolution to V7 in bar 32.
    let omnibus: [[u8; 3]; 6] = [
        [65, 57, 74],
        [64, 57, 75],
        [63, 57, 76],
        [62, 57, 77],
        [61, 57, 78],
        [60, 57, 79],
    ];
    for (slot, voices) in omnibus.iter().enumerate() {
        let bar = 29 + slot / 2;
        let beat = 1.0 + 2.0 * (slot % 2) as f32;
        rows.push((bar, beat, 1.85, voices[voice], 0.6 + 0.02 * slot as f32));
    }
    rows.push((32, 1.0, 3.8, [61, 57, 67][voice], 0.62));
    // Picardy tease then the deceptive bVI close, held into the tail.
    rows.push((37, 1.0, 3.8, [57, 62, 65][voice], 0.48));
    rows.push((38, 1.0, 3.8, [57, 62, 66][voice], 0.5));
    rows.push((39, 1.0, 8.0, [58, 62, 65][voice], 0.54));
    rows
}

fn string_voice(meter: Meter, voice: usize) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, beat, duration, note, velocity) in string_rows(voice) {
        part.hit(bar, beat, duration, note, velocity);
    }
    part.into_notes()
}

fn string_voice_low(meter: Meter) -> Vec<Note> {
    string_voice(meter, 0)
}

fn string_voice_mid(meter: Meter) -> Vec<Note> {
    string_voice(meter, 1)
}

fn string_voice_high(meter: Meter) -> Vec<Note> {
    string_voice(meter, 2)
}

// --- The aria (tube lead) ---------------------------------------------------------

/// Cycle 2: the Ascent motto in 4x augmentation, each tone approached from
/// its upper neighbor (the appoggiatura sigh).
const ARIA_MOTTO: [PhraseNote; 8] = [
    (5, 1.0, 1.0, 64, 0.50),
    (5, 2.0, 2.92, 62, 0.48),
    (6, 1.0, 1.0, 65, 0.52),
    (6, 2.0, 2.92, 64, 0.50),
    (7, 1.0, 1.0, 67, 0.54),
    (7, 2.0, 2.92, 65, 0.52),
    (8, 1.0, 1.92, 64, 0.50),
    (8, 3.0, 1.42, 61, 0.48),
];

/// Phrases 1–2 (cycles 3–4): stepwise, sighing, a breath at each cycle end.
const ARIA_PHRASES: [PhraseNote; 16] = [
    (9, 1.0, 1.0, 69, 0.54),
    (9, 2.0, 2.92, 67, 0.52),
    (10, 1.0, 1.0, 67, 0.52),
    (10, 2.0, 2.92, 65, 0.50),
    (11, 1.0, 1.92, 67, 0.54),
    (11, 3.0, 1.42, 65, 0.50),
    (12, 1.0, 2.92, 64, 0.50),
    (13, 1.0, 1.0, 71, 0.56),
    (13, 2.0, 2.92, 69, 0.54),
    (14, 1.0, 1.0, 72, 0.58),
    (14, 2.0, 2.92, 70, 0.54),
    (15, 1.0, 1.92, 69, 0.54),
    (15, 3.0, 1.42, 67, 0.52),
    (16, 1.0, 1.92, 65, 0.52),
    (16, 3.0, 1.42, 64, 0.50),
    (20, 3.0, 1.92, 62, 0.46),
];

/// Hijaz cycle (bars 21–24): the line bent through A–B♭–C#–D — the
/// augmented 2nd between B♭ and C# is the maqam color over the pedal.
const ARIA_HIJAZ: [PhraseNote; 9] = [
    (21, 1.0, 1.92, 69, 0.52),
    (21, 3.0, 1.92, 70, 0.54),
    (22, 1.0, 1.92, 73, 0.58),
    (22, 3.0, 1.92, 74, 0.56),
    (23, 1.0, 1.0, 74, 0.56),
    (23, 2.0, 1.92, 73, 0.54),
    (23, 4.0, 0.92, 70, 0.52),
    (24, 1.0, 1.92, 69, 0.52),
    (24, 3.0, 1.92, 68, 0.50),
];

/// Phrase 3 (cycle 7): the sixth leap up, recovered by step — and the climb
/// to the climax.
const ARIA_LEAP: [PhraseNote; 10] = [
    (25, 1.0, 1.92, 62, 0.52),
    (25, 3.0, 1.92, 70, 0.60),
    (26, 1.0, 1.0, 69, 0.58),
    (26, 2.0, 2.92, 67, 0.56),
    (27, 1.0, 1.92, 70, 0.60),
    (27, 3.0, 1.92, 72, 0.62),
    (28, 1.0, 1.92, 74, 0.64),
    (28, 3.0, 0.92, 76, 0.66),
    (28, 4.0, 0.92, 77, 0.68),
    (28, 4.5, 0.45, 79, 0.68),
];

/// Climax (cycle 8): the single peak A5 held over the omnibus, then the fall.
const ARIA_CLIMAX: [PhraseNote; 6] = [
    (29, 1.0, 7.42, 81, 0.70),
    (31, 1.0, 1.0, 79, 0.64),
    (31, 2.0, 1.92, 77, 0.62),
    (31, 4.0, 0.92, 74, 0.58),
    (32, 1.0, 1.92, 73, 0.56),
    (32, 3.0, 1.92, 69, 0.54),
];

/// Denouement and the withheld Picardy: the voice rises to F# (the major
/// third) — and stops. The orchestra answers with bVI, not the major tonic.
const ARIA_CLOSE: [PhraseNote; 10] = [
    (33, 1.0, 1.0, 76, 0.56),
    (33, 2.0, 2.92, 74, 0.54),
    (34, 1.0, 1.0, 74, 0.54),
    (34, 2.0, 2.92, 72, 0.52),
    (35, 1.0, 1.92, 70, 0.52),
    (35, 3.0, 1.42, 69, 0.50),
    (36, 1.0, 2.92, 67, 0.48),
    (37, 1.0, 3.92, 62, 0.46),
    (38, 1.0, 3.92, 66, 0.52),
    (39, 2.0, 2.92, 65, 0.44),
];

fn tube_lead(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    push_phrase(&mut part, &ARIA_MOTTO, 0, usize::MAX);
    push_phrase(&mut part, &ARIA_PHRASES, 0, usize::MAX);
    push_phrase(&mut part, &ARIA_HIJAZ, 0, usize::MAX);
    push_phrase(&mut part, &ARIA_LEAP, 0, usize::MAX);
    push_phrase(&mut part, &ARIA_CLIMAX, 0, usize::MAX);
    push_phrase(&mut part, &ARIA_CLOSE, 0, usize::MAX);
    part.into_notes()
}

/// The dark tube holds the drone floor: low D through the opening cycles,
/// the dominant through the Hijaz cycle, a low B♭ under the close.
fn tube_harmony(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in [1, 3, 5, 7] {
        part.hit(bar, 1.0, 7.8, 50, 0.42);
    }
    for bar in [21, 23] {
        part.hit(bar, 1.0, 7.8, 45, 0.42);
    }
    part.hit(29, 1.0, 15.0, 45, 0.46);
    part.hit(39, 1.0, 8.0, 46, 0.44);
    part.into_notes()
}

// --- Keys and bells -------------------------------------------------------------

/// The keys interlude (cycle 5) and gentle support after: slow broken chords,
/// the Zanarkand restraint rather than the prelude's sparkle.
fn modal_arp(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    let interlude: [[u8; 4]; 4] = [
        [62, 69, 74, 77],
        [60, 65, 72, 77],
        [58, 67, 74, 79],
        [57, 64, 73, 76],
    ];
    for (cycle_bar, tones) in interlude.iter().enumerate() {
        let bar = 17 + cycle_bar;
        for (slot, tone) in tones.iter().enumerate() {
            part.hit(bar, 1.0 + slot as f32, 2.0, *tone, 0.46);
        }
    }
    // Soft support under phrase 3 and the climax.
    for (bar, tones) in [
        (25usize, [62u8, 69, 74]),
        (26, [60, 65, 72]),
        (27, [58, 67, 74]),
        (28, [57, 64, 73]),
    ] {
        for (slot, tone) in tones.iter().enumerate() {
            part.hit(bar, 1.0 + slot as f32 * 1.5, 2.0, *tone, 0.44);
        }
    }
    part.into_notes()
}

/// Bells: distant tolls at cycle boundaries, the final deceptive chord roll.
fn modal_bells(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in [9, 13, 25] {
        part.hit(bar, 1.0, 3.0, 74, 0.42);
    }
    part.hit(29, 1.0, 4.0, 81, 0.5);
    part.hit(33, 1.0, 3.0, 74, 0.44);
    // The deceptive close: B♭ rolled high, left to ring.
    part.hit(39, 1.0, 6.0, 70, 0.52);
    part.hit(39, 1.5, 6.0, 77, 0.5);
    part.hit(39, 2.0, 6.0, 82, 0.46);
    part.into_notes()
}

// --- Cymbals ----------------------------------------------------------------------

/// Soft gong swells at the section seams only.
fn cymbal_ride(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, velocity) in [(8usize, 0.22), (16, 0.22), (24, 0.26), (28, 0.3), (36, 0.2)] {
        part.hit(bar, 3.0, 1.8, 60, velocity);
    }
    part.into_notes()
}

/// One swell into the climax downbeat; one soft touch on the deceptive chord.
fn cymbal_crash(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for slot in 0..6 {
        part.hit(
            28,
            1.0 + slot as f32 * 0.5,
            0.45,
            60,
            0.08 + 0.04 * slot as f32,
        );
    }
    part.hit(29, 1.0, 10.0, 60, 0.5);
    part.hit(39, 1.0, 12.0, 60, 0.34);
    part.into_notes()
}
