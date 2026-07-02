//! Movement IV — "Waltz": a dark waltz in G minor, 3/4 at 150 BPM (one-in-a-bar).
//!
//! Models: Sibelius's Valse triste (rhythm first, melody later) and
//! Shostakovich's Waltz No. 2 (wry minor tune over a deliberately square
//! oom-pah-pah). The waltz tune is Movement I's Ascent motto re-metered into
//! 3/4 and decorated with grace-note turns — Berlioz's "Un bal" move.
//!
//! | Bars    | Section | Material |
//! | ------- | ------- | -------- |
//! | 1–8     | Rhythm  | Oom-pah-pah alone, melody pickup in bar 8 |
//! | 9–24    | Waltz A | The motto as waltz tune, G minor |
//! | 25–40   | Waltz A' | Tune an octave up on the bells, tube counter below |
//! | 41–72   | Trio    | B♭ major; the Dream arpeggio flows in the keys |
//! | 73–88   | Reprise | Waltz A with written-in rubato (late entries) |
//! | 89–92   | Pivot   | Common-tone dim7 → F#7: the chromatic-mediant lift |
//! | 93–108  | Final   | The tune replayed +4 in B minor (foreshadowing V) |
//! | 109–116 | Coda    | Hemiola cadence (2-beat groups across 3/4), quiet close |
//! | 117–120 | Tail    | Ring-out |

use crate::composition::{FLAT, Level, MixSpec, PhraseNote, TrackSpec, push_phrase, track};
use crate::render::Voice;
use crate::score::{Chord, Meter, Note, Part};

pub(crate) const METER: Meter = Meter::new(150.0, 3.0);
pub(crate) const TOTAL_BARS: usize = 120;

/// Gentle dynamic arc: the trio sits back, the B-minor final opens up,
/// the coda tapers away.
const BED_RIDE: &[(f32, f32)] = &[
    (40.0, -1.0),
    (41.0, -2.5),
    (73.0, -1.5),
    (93.0, 0.0),
    (109.0, -1.0),
    (116.0, -4.0),
];

pub(crate) fn tracks(meter: Meter) -> Vec<TrackSpec> {
    vec![
        track("tube-lead", Voice::TubeLead, tube_lead(meter), lead_mix()),
        track(
            "tube-harmony",
            Voice::TubeDark,
            tube_harmony(meter),
            bed_mix(-22.0, -0.25),
        ),
        track(
            "string-chord-low",
            Voice::StringBow,
            string_voice(meter, 0),
            bed_mix(-28.0, -0.7),
        ),
        track(
            "string-chord-mid",
            Voice::StringBow,
            string_voice(meter, 1),
            bed_mix(-28.0, 0.15),
        ),
        track(
            "string-chord-high",
            Voice::StringBow,
            string_voice(meter, 2),
            bed_mix(-28.0, 0.7),
        ),
        track(
            "string-bass",
            Voice::StringPick,
            string_bass(meter),
            bass_mix(),
        ),
        track(
            "modal-arp",
            Voice::ModalKeys,
            modal_arp(meter),
            bed_mix(-24.0, 0.5),
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
            bed_mix(-28.0, 0.4),
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
        level: Level::ActiveRms(-19.5),
        pan: 0.0,
        highpass_hz: 35.0,
        ride: FLAT,
    }
}

fn bells_mix() -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(-21.0),
        pan: -0.45,
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

// --- Harmony -----------------------------------------------------------------

const GMI: Chord = Chord {
    bass: 43,
    voices: [58, 62, 67],
};
const CMI: Chord = Chord {
    bass: 48,
    voices: [60, 63, 67],
};
const D7: Chord = Chord {
    bass: 50,
    voices: [60, 66, 69],
};
const BB: Chord = Chord {
    bass: 46,
    voices: [58, 62, 65],
};
const EB: Chord = Chord {
    bass: 51,
    voices: [58, 63, 67],
};
const F: Chord = Chord {
    bass: 53,
    voices: [57, 60, 65],
};
/// Common-tone diminished 7th (B–D–F–A♭): shares D with both G minor and
/// B minor — the pivot that lifts the final reprise a chromatic mediant.
const B07: Chord = Chord {
    bass: 47,
    voices: [62, 65, 68],
};
const FS7: Chord = Chord {
    bass: 42,
    voices: [58, 61, 64],
};

/// One 16-bar waltz strain in the home key.
const STRAIN: [Chord; 16] = [
    GMI, GMI, D7, D7, GMI, GMI, CMI, D7, GMI, BB, EB, CMI, D7, D7, GMI, GMI,
];
/// The trio's 8-bar period in B♭ major.
const TRIO: [Chord; 8] = [BB, EB, F, BB, EB, BB, F, BB];

/// The chord per bar as (bar, chord, transpose). The final reprise replays
/// home-key shapes at +4 — G minor lifted to B minor.
fn harmony() -> Vec<(usize, Chord, i32)> {
    let mut bars: Vec<(usize, Chord, i32)> = Vec::new();
    let mut push = |start: usize, transpose: i32, chords: &[Chord]| {
        for (index, chord) in chords.iter().enumerate() {
            bars.push((start + index, *chord, transpose));
        }
    };
    push(1, 0, &[GMI, GMI, D7, D7, GMI, CMI, D7, GMI]);
    push(9, 0, &STRAIN);
    push(25, 0, &STRAIN);
    for start in [41, 49, 57] {
        push(start, 0, &TRIO);
    }
    push(65, 0, &[BB, EB, F, BB, EB, F, D7, D7]);
    push(73, 0, &STRAIN);
    push(89, 0, &[GMI, B07, B07, FS7]);
    push(93, 4, &STRAIN);
    push(109, 4, &[GMI, D7, GMI, D7, GMI, D7, GMI, GMI]);
    bars
}

// --- Oom-pah-pah (bass + keys) ---------------------------------------------------

fn string_bass(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord, transpose) in harmony() {
        part.set_transpose(transpose);
        match bar {
            109..=114 => {
                // Hemiola: the bass marks 2-beat groups across the 3/4 bars.
                if bar % 2 == 1 {
                    part.hit(bar, 1.0, 0.9, chord.bass, 0.6);
                    part.hit(bar, 3.0, 0.9, chord.bass, 0.55);
                } else {
                    part.hit(bar, 2.0, 0.9, chord.bass, 0.58);
                }
            }
            115 | 116 => part.hit(bar, 1.0, 2.0, chord.bass, 0.5),
            _ => {
                part.hit(bar, 1.0, 0.9, chord.bass, 0.6);
                // The occasional fifth keeps the oom from plodding.
                if bar % 4 == 0 {
                    part.hit(bar, 3.0, 0.6, chord.bass + 7, 0.42);
                }
            }
        }
    }
    part.into_notes()
}

/// The pah-pah: struck-key chords on beats 2 and 3, lighter on 3. In the
/// trio the keys switch to the flowing Dream-arpeggio eighths; the reprise
/// anticipates beat 2 slightly (the Viennese lilt, written in).
fn modal_arp(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord, transpose) in harmony() {
        part.set_transpose(transpose);
        match bar {
            41..=64 => trio_arpeggio_bar(&mut part, bar, chord),
            65..=72 => trio_arpeggio_bar(&mut part, bar, chord),
            109..=116 => {}
            _ => {
                let beat_two = if bar >= 73 { 1.9 } else { 2.0 };
                for (beat, velocity) in [(beat_two, 0.5), (3.0, 0.4)] {
                    for tone in chord.voices {
                        part.hit(bar, beat, 0.55, tone, velocity);
                    }
                }
            }
        }
    }
    part.into_notes()
}

/// The trio's flowing line: the Dream arpeggio (1–3–5–8) up and back in
/// eighths, one chord per bar.
fn trio_arpeggio_bar(part: &mut Part, bar: usize, chord: Chord) {
    let [low, mid, high] = chord.voices;
    let tones = [low + 12, mid + 12, high + 12, low + 24, high + 12, mid + 12];
    for (slot, tone) in tones.iter().enumerate() {
        part.hit(bar, 1.0 + slot as f32 * 0.5, 0.8, *tone, 0.46);
    }
}

// --- The waltz tune -----------------------------------------------------------

/// The motto re-metered into 3/4 (dotted halves), with grace-turn
/// decorations: 1–2–3 rising, then the 3–4–3–1 arch with ♭6 leans.
/// Written in G minor; the final reprise replays it at +4.
const WALTZ_TUNE: [PhraseNote; 22] = [
    (1, 1.0, 2.7, 67, 0.52),
    (2, 1.0, 2.7, 69, 0.54),
    (3, 1.0, 2.7, 70, 0.56),
    (4, 1.0, 1.7, 70, 0.54),
    (4, 3.0, 0.9, 72, 0.52),
    (5, 1.0, 2.7, 72, 0.56),
    (6, 1.0, 2.7, 70, 0.54),
    (7, 1.0, 1.7, 69, 0.52),
    (7, 3.0, 0.9, 67, 0.50),
    (8, 1.0, 2.7, 67, 0.52),
    (9, 1.0, 2.7, 74, 0.58),
    (10, 1.0, 1.7, 75, 0.58),
    (10, 3.0, 0.9, 74, 0.54),
    (11, 1.0, 2.7, 72, 0.56),
    (12, 1.0, 2.7, 70, 0.54),
    (13, 1.0, 1.7, 69, 0.54),
    (13, 3.0, 0.9, 72, 0.52),
    (14, 1.0, 2.7, 66, 0.54),
    (15, 1.0, 2.7, 67, 0.52),
    (16, 1.0, 1.7, 67, 0.5),
    (16, 2.0, 0.6, 62, 0.44),
    (16, 3.0, 0.6, 58, 0.4),
]; // bars are strain-local (1–16)

/// Grace-note turns before the tune's downbeats (strain-local bar numbers).
const TUNE_GRACES: [(usize, u8); 5] = [(2, 67), (3, 69), (5, 74), (9, 72), (15, 66)];

fn push_tune(part: &mut Part, bar_offset: usize, late: bool, velocity_lift: f32) {
    for (bar, beat, duration, midi, velocity) in WALTZ_TUNE {
        // Written-in rubato: the reprise melody enters an eighth late.
        let beat = if late && beat == 1.0 { 1.5 } else { beat };
        let duration = if late && beat == 1.5 {
            duration - 0.5
        } else {
            duration
        };
        part.hit(
            bar + bar_offset,
            beat,
            duration,
            midi,
            velocity + velocity_lift,
        );
    }
    for (bar, grace) in TUNE_GRACES {
        part.hit(bar + bar_offset - 1, 3.7, 0.25, grace, 0.4 + velocity_lift);
    }
}

fn tube_lead(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    // Pickup out of the rhythm-only opening.
    part.hit(8, 2.0, 0.6, 62, 0.44);
    part.hit(8, 3.0, 0.6, 66, 0.48);
    // Waltz A: the tune.
    push_tune(&mut part, 8, false, 0.0);
    // Trio: the tube rests through the first period, then sings the warm
    // B♭ answer (stepwise, one leap, recovered).
    const TRIO_TUNE: [PhraseNote; 14] = [
        (49, 1.0, 2.7, 65, 0.5),
        (50, 1.0, 2.7, 67, 0.52),
        (51, 1.0, 2.7, 69, 0.54),
        (52, 1.0, 2.7, 70, 0.54),
        (53, 1.0, 1.7, 67, 0.52),
        (53, 3.0, 0.9, 65, 0.5),
        (54, 1.0, 2.7, 62, 0.5),
        (55, 1.0, 2.7, 60, 0.48),
        (56, 1.0, 2.7, 58, 0.46),
        (57, 1.0, 2.7, 63, 0.5),
        (58, 1.0, 2.7, 65, 0.52),
        (59, 1.0, 2.7, 67, 0.54),
        (60, 1.0, 5.7, 70, 0.56),
        (63, 1.0, 2.7, 65, 0.5),
    ];
    push_phrase(&mut part, &TRIO_TUNE, 0, usize::MAX);
    // Out of the trio: a falling line into the reprise.
    const TRIO_OUT: [PhraseNote; 4] = [
        (69, 1.0, 2.7, 69, 0.52),
        (70, 1.0, 2.7, 67, 0.5),
        (71, 1.0, 2.7, 66, 0.5),
        (72, 1.0, 2.7, 62, 0.48),
    ];
    push_phrase(&mut part, &TRIO_OUT, 0, usize::MAX);
    // Reprise with the written-in rubato.
    push_tune(&mut part, 72, true, 0.0);
    // The pivot: the tune's head stalls on the dim7 (suspense), then F#7.
    part.hit(89, 1.0, 2.7, 67, 0.5);
    part.hit(90, 1.0, 2.7, 68, 0.52);
    part.hit(91, 1.0, 2.7, 71, 0.54);
    part.hit(92, 1.0, 1.7, 70, 0.54);
    part.hit(92, 3.0, 0.9, 66, 0.52);
    // Final reprise lifted to B minor.
    part.set_transpose(4);
    push_tune(&mut part, 92, false, 0.06);
    // Coda: hemiola figures and the quiet close on the tonic third.
    const CODA: [PhraseNote; 7] = [
        (109, 1.0, 1.9, 70, 0.52),
        (110, 2.0, 1.9, 69, 0.5),
        (111, 3.0, 1.9, 70, 0.5),
        (113, 1.0, 1.9, 67, 0.46),
        (114, 2.0, 1.9, 66, 0.44),
        (115, 1.0, 2.7, 62, 0.42),
        (116, 1.0, 2.7, 58, 0.4),
    ];
    push_phrase(&mut part, &CODA, 0, usize::MAX);
    part.into_notes()
}

/// Counter-voice: long tones a sixth under the tune in A', sighing thirds in
/// the reprise, a low pedal under the coda.
fn tube_harmony(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    const COUNTER_A2: [PhraseNote; 8] = [
        (25, 1.0, 2.7, 58, 0.44),
        (26, 1.0, 2.7, 60, 0.46),
        (27, 1.0, 2.7, 62, 0.48),
        (28, 1.0, 2.7, 62, 0.46),
        (29, 1.0, 2.7, 63, 0.48),
        (30, 1.0, 2.7, 62, 0.46),
        (31, 1.0, 2.7, 60, 0.44),
        (32, 1.0, 2.7, 58, 0.44),
    ];
    push_phrase(&mut part, &COUNTER_A2, 0, usize::MAX);
    part.set_transpose(4);
    const COUNTER_FINAL: [PhraseNote; 4] = [
        (93, 1.0, 2.7, 58, 0.46),
        (94, 1.0, 2.7, 60, 0.48),
        (95, 1.0, 2.7, 62, 0.5),
        (96, 1.0, 2.7, 62, 0.48),
    ];
    push_phrase(&mut part, &COUNTER_FINAL, 0, usize::MAX);
    part.set_transpose(0);
    part.hit(115, 1.0, 5.7, 47, 0.42);
    part.into_notes()
}

// --- Strings and bells -----------------------------------------------------------

/// Sustained dotted-half pads: tacet in the rhythm-only opening and first
/// strain, entering for A', thinned in the trio, full for the final reprise.
fn string_voice(meter: Meter, voice: usize) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, chord, transpose) in harmony() {
        part.set_transpose(transpose);
        let tone = chord.voices[voice];
        match bar {
            1..=24 => {}
            41..=64 if voice != 1 => {}
            109..=114 => {
                // Hemiola: bowed accents with the bass's 2-beat grouping.
                if bar % 2 == 1 {
                    part.hit(bar, 1.0, 1.9, tone, 0.5);
                }
            }
            115 | 116 => part.hit(bar, 1.0, 5.0, tone, 0.44),
            _ => part.hit(bar, 1.0, 2.8, tone, 0.48),
        }
    }
    part.into_notes()
}

/// A': the tune an octave up on the bells (the crooning layer); light
/// sparkles over the trio; the final chord's high third.
fn modal_bells(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for (bar, beat, duration, midi, velocity) in WALTZ_TUNE {
        part.hit(bar + 24, beat, duration, midi + 12, velocity + 0.02);
    }
    for bar in [44usize, 52, 60] {
        part.hit(bar, 2.0, 2.0, 82, 0.4);
        part.hit(bar + 2, 3.0, 2.0, 86, 0.38);
    }
    part.set_transpose(4);
    for (bar, grace) in TUNE_GRACES {
        part.hit(bar + 92 - 1, 3.7, 0.25, grace + 12, 0.42);
    }
    part.set_transpose(0);
    part.hit(115, 1.0, 4.0, 71, 0.5);
    part.hit(116, 1.0, 4.0, 74, 0.44);
    part.into_notes()
}

// --- Cymbals ----------------------------------------------------------------------

/// Brush-soft gong taps marking the bar in the trio and the hemiola accents
/// in the coda; otherwise the waltz keeps its own time.
fn cymbal_ride(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for bar in 41..=72 {
        part.hit(bar, 1.0, 0.9, 60, 0.22);
    }
    for bar in 109..=114 {
        if bar % 2 == 1 {
            part.hit(bar, 1.0, 0.9, 60, 0.3);
            part.hit(bar, 3.0, 0.9, 60, 0.26);
        } else {
            part.hit(bar, 2.0, 0.9, 60, 0.28);
        }
    }
    part.into_notes()
}

/// One soft swell into the B-minor reprise; a final touch on the close.
fn cymbal_crash(meter: Meter) -> Vec<Note> {
    let mut part = Part::new(meter);
    for slot in 0..4 {
        part.hit(
            92,
            1.0 + slot as f32 * 0.5,
            0.4,
            60,
            0.1 + 0.05 * slot as f32,
        );
    }
    part.hit(93, 1.0, 8.0, 60, 0.42);
    part.hit(115, 1.0, 10.0, 60, 0.3);
    part.into_notes()
}
