//! "Olórelindë (The Dream Ascending)" — a 32-bar piece for the four Lamath
//! instrument families.
//!
//! Form at 96 BPM in 4/4, in A minor with a whole-step modulation to B minor:
//!
//! | Bars  | Section | Material |
//! | ----- | ------- | -------- |
//! | 1–4   | Intro   | Modal arpeggio alone, tube pedal, crash swell |
//! | 5–12  | A       | Tube lead melody, three bowed-string chord voices, picked bass, modal arpeggios, ride |
//! | 13–20 | B       | Bell counter-melody takes over, tube duet in bars 17–20 |
//! | 21–24 | Build   | Crescendo on every track, rising sixteenth ladder into the modulation |
//! | 25–32 | Final   | A-section material transposed up a whole step (B minor), full ensemble |
//! | 33–36 | Tail    | Ring-out (final crash, bell, and bowed chord decay) |
//!
//! Chords are voiced by rendering three separate bowed-string instances (one
//! voice each); arpeggios run on the modal keys two octaves above the chords.

use crate::render::{SAMPLE_RATE, Voice};
use crate::score::{BEATS_PER_BAR, Chord, Note, Part, beats_to_seconds};

pub(crate) const TOTAL_BARS: usize = 36;

/// (bar, beat, duration beats, written MIDI note, velocity)
type PhraseNote = (usize, f32, f32, u8, f32);

pub(crate) fn total_frames() -> usize {
    (beats_to_seconds(TOTAL_BARS as f32 * BEATS_PER_BAR) * SAMPLE_RATE as f32).ceil() as usize
}

/// How the mixer levels a track before summing.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Level {
    /// Normalize the track's active-region RMS to this dBFS value. For
    /// sustained material, where loudness is what matters.
    ActiveRms(f32),
    /// Normalize the track's absolute peak to this dBFS value. For sparse
    /// transient material (the crash), whose ring-tail RMS says nothing about
    /// how hard its strikes hit the mix bus.
    Peak(f32),
}

/// One mixer channel's placement: level target, pan, cleanup filter, fader ride.
pub(crate) struct MixSpec {
    pub(crate) level: Level,
    /// Constant-power pan, -1 (left) to +1 (right).
    pub(crate) pan: f32,
    /// Cleanup highpass cutoff applied to the stem before leveling.
    pub(crate) highpass_hz: f32,
    /// Fader ride as (bar, dB) breakpoints, linearly interpolated; constant
    /// before the first and after the last. Empty = flat at 0 dB.
    pub(crate) ride: &'static [(f32, f32)],
}

/// One mixer channel: an instrument voice, its notes, and its mix placement.
pub(crate) struct TrackSpec {
    pub(crate) name: &'static str,
    pub(crate) voice: Voice,
    pub(crate) notes: Vec<Note>,
    pub(crate) mix: MixSpec,
}

/// The accompaniment bed ducks through the early sections and opens to full
/// for the final section, giving the song a dynamic arc the flat targets
/// could not: -3 dB through the intro and A, -2 dB in B, ramp through the
/// build, full from the key change on.
const BED_RIDE: &[(f32, f32)] = &[(12.0, -3.0), (13.0, -2.0), (21.0, -2.0), (25.0, 0.0)];
const FLAT: &[(f32, f32)] = &[];

pub(crate) fn tracks() -> Vec<TrackSpec> {
    vec![
        track("tube-lead", Voice::TubeLead, tube_lead(), lead_mix()),
        track(
            "tube-harmony",
            Voice::TubeDark,
            tube_harmony(),
            bed_mix(-20.0, -0.25),
        ),
        track(
            "string-chord-low",
            Voice::StringBow,
            string_chord_voice(0),
            bed_mix(-27.0, -0.7),
        ),
        track(
            "string-chord-mid",
            Voice::StringBow,
            string_chord_voice(1),
            bed_mix(-27.0, 0.15),
        ),
        track(
            "string-chord-high",
            Voice::StringBow,
            string_chord_voice(2),
            bed_mix(-27.0, 0.7),
        ),
        track("string-bass", Voice::StringPick, string_bass(), bass_mix()),
        track(
            "modal-arp",
            Voice::ModalKeys,
            modal_arp(),
            bed_mix(-25.0, 0.5),
        ),
        track("modal-bells", Voice::ModalBells, modal_bells(), bells_mix()),
        track(
            "cymbal-ride",
            Voice::CymbalRide,
            cymbal_ride(),
            bed_mix(-24.0, 0.4),
        ),
        track(
            "cymbal-crash",
            Voice::CymbalCrash,
            cymbal_crash(),
            crash_mix(),
        ),
    ]
}

fn track(name: &'static str, voice: Voice, notes: Vec<Note>, mix: MixSpec) -> TrackSpec {
    TrackSpec {
        name,
        voice,
        notes,
        mix,
    }
}

fn lead_mix() -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(-16.0),
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
        level: Level::ActiveRms(-19.0),
        pan: 0.0,
        highpass_hz: 35.0,
        ride: BED_RIDE,
    }
}

fn bells_mix() -> MixSpec {
    MixSpec {
        level: Level::ActiveRms(-20.0),
        pan: -0.5,
        highpass_hz: 90.0,
        ride: FLAT,
    }
}

fn crash_mix() -> MixSpec {
    MixSpec {
        level: Level::Peak(-8.0),
        pan: 0.0,
        highpass_hz: 130.0,
        ride: FLAT,
    }
}

// --- Harmony -----------------------------------------------------------------

const AM: Chord = Chord {
    bass: 45,
    voices: [57, 60, 64],
};
const F: Chord = Chord {
    bass: 41,
    voices: [57, 60, 65],
};
const C: Chord = Chord {
    bass: 48,
    voices: [55, 60, 64],
};
const G: Chord = Chord {
    bass: 43,
    voices: [55, 59, 62],
};
const DM: Chord = Chord {
    bass: 50,
    voices: [57, 62, 65],
};
const E: Chord = Chord {
    bass: 40,
    voices: [56, 59, 64],
};
const EM: Chord = Chord {
    bass: 40,
    voices: [55, 59, 64],
};

/// The chord per bar as (bar, chord, transpose). Written in A minor; the final
/// section re-uses home-key shapes transposed +2 (Bm, G, D, A, Bm, G, F#, Bm).
fn harmony() -> Vec<(usize, Chord, i32)> {
    let mut bars = Vec::new();
    push_chords(&mut bars, 5, 0, &[AM, F, C, G, AM, F, DM, E]);
    push_chords(&mut bars, 13, 0, &[F, G, AM, EM, F, G, E, E]);
    push_chords(&mut bars, 21, 0, &[AM, F, G, E]);
    push_chords(&mut bars, 25, 2, &[AM, F, C, G, AM, F, E, AM]);
    bars
}

fn push_chords(
    bars: &mut Vec<(usize, Chord, i32)>,
    start: usize,
    transpose: i32,
    chords: &[Chord],
) {
    for (index, chord) in chords.iter().enumerate() {
        bars.push((start + index, *chord, transpose));
    }
}

fn chord_velocity(bar: usize) -> f32 {
    match bar {
        5..=12 => 0.55,
        13..=20 => 0.5,
        21..=24 => 0.5 + 0.12 * (bar - 21) as f32,
        32 => 0.8,
        _ => 0.7,
    }
}

// --- Strings -----------------------------------------------------------------

/// One bowed chord voice: three of these tracks (voice 0/1/2) sum into triads.
fn string_chord_voice(voice_index: usize) -> Vec<Note> {
    let mut part = Part::new();
    for (bar, chord, transpose) in harmony() {
        part.set_transpose(transpose);
        let duration = if bar == 32 { 6.0 } else { 3.7 };
        part.hit(
            bar,
            1.0,
            duration,
            chord.voices[voice_index],
            chord_velocity(bar),
        );
    }
    part.into_notes()
}

fn string_bass() -> Vec<Note> {
    let mut part = Part::new();
    for (bar, chord, transpose) in harmony() {
        part.set_transpose(transpose);
        match bar {
            32 => part.hit(bar, 1.0, 6.0, chord.bass, 0.85),
            21..=24 => bass_build_bar(&mut part, bar, chord),
            _ => bass_pattern_bar(&mut part, bar, chord),
        }
    }
    part.into_notes()
}

/// Root on 1, pickup root on 2.5, fifth on 3: a simple root–fifth groove.
fn bass_pattern_bar(part: &mut Part, bar: usize, chord: Chord) {
    part.hit(bar, 1.0, 1.45, chord.bass, 0.72);
    part.hit(bar, 2.5, 0.4, chord.bass, 0.55);
    part.hit(bar, 3.0, 1.8, chord.bass + 7, 0.65);
}

/// Quarter-note pulse with a velocity ramp through the build.
fn bass_build_bar(part: &mut Part, bar: usize, chord: Chord) {
    let base = 0.6 + 0.08 * (bar - 21) as f32;
    for beat in 0..4 {
        part.hit(
            bar,
            1.0 + beat as f32,
            0.85,
            chord.bass,
            base + 0.02 * beat as f32,
        );
    }
}

// --- Modal keys (arpeggios) ----------------------------------------------------

fn modal_arp() -> Vec<Note> {
    let mut part = Part::new();
    intro_arp(&mut part);
    for (bar, chord, transpose) in harmony() {
        part.set_transpose(transpose);
        arp_bar(&mut part, bar, chord);
    }
    part.into_notes()
}

fn arp_bar(part: &mut Part, bar: usize, chord: Chord) {
    match bar {
        // The bells own the start of the B section; the arp re-enters at 17.
        13..=16 => {}
        21..=23 => arp_eighths(part, bar, chord, 0.55 + 0.1 * (bar - 21) as f32),
        24 => arp_rising_sixteenths(part, bar, chord),
        32 => arp_final_roll(part, chord),
        _ => arp_eighths(part, bar, chord, 0.6),
    }
}

/// Chord tones two octaves above the string voicing, plus the doubled root —
/// clear of the chord/lead midrange so the arp reads as sparkle, not doubling.
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
            1.2,
            tones[*tone_index],
            velocity + accent,
        );
    }
}

/// Bar 24: a two-octave sixteenth ladder, crescendo into the key change.
fn arp_rising_sixteenths(part: &mut Part, bar: usize, chord: Chord) {
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
        let velocity = 0.55 + 0.022 * slot as f32;
        part.hit(
            bar,
            1.0 + slot as f32 * 0.25,
            0.8,
            ladder[slot % 8],
            velocity,
        );
    }
}

/// Bar 32: a quick rolled chord left to ring with the final crash.
fn arp_final_roll(part: &mut Part, chord: Chord) {
    for (slot, tone) in arp_tones(chord).iter().enumerate() {
        part.hit(32, 1.0 + slot as f32 * 0.25, 3.0, *tone, 0.78);
    }
}

fn intro_arp(part: &mut Part) {
    for (slot, tone) in [81u8, 84, 88, 93].iter().enumerate() {
        part.hit(1, 1.0 + slot as f32, 2.0, *tone, 0.42);
    }
    for (slot, tone) in [84u8, 88, 93, 96].iter().enumerate() {
        part.hit(2, 1.0 + slot as f32, 2.0, *tone, 0.45);
    }
    arp_eighths(part, 3, AM, 0.48);
    arp_eighths(part, 4, AM, 0.52);
}

// --- Modal bells ----------------------------------------------------------------

/// B-section counter-melody over F G Am Em / F G E E.
const BELL_MELODY: [PhraseNote; 15] = [
    (13, 1.0, 2.0, 81, 0.60),
    (13, 3.0, 2.0, 84, 0.62),
    (14, 1.0, 2.0, 83, 0.60),
    (14, 3.0, 2.0, 79, 0.55),
    (15, 1.0, 3.0, 81, 0.62),
    (15, 4.0, 1.0, 76, 0.50),
    (16, 1.0, 4.0, 79, 0.55),
    (17, 1.0, 1.0, 77, 0.55),
    (17, 2.0, 1.0, 81, 0.58),
    (17, 3.0, 2.0, 84, 0.62),
    (18, 1.0, 2.0, 86, 0.65),
    (18, 3.0, 2.0, 83, 0.60),
    (19, 1.0, 2.0, 80, 0.60),
    (19, 3.0, 2.0, 76, 0.55),
    (20, 1.0, 4.0, 83, 0.62),
];

fn modal_bells() -> Vec<Note> {
    let mut part = Part::new();
    push_phrase(&mut part, &BELL_MELODY, 0, usize::MAX);
    part.set_transpose(2);
    for bar in [25, 27, 29] {
        part.hit(bar, 1.0, 3.0, 81, 0.5);
    }
    part.hit(31, 1.0, 3.0, 80, 0.5);
    part.hit(32, 1.0, 4.0, 81, 0.72);
    part.hit(32, 2.0, 4.0, 88, 0.6);
    part.into_notes()
}

// --- Tube ------------------------------------------------------------------------

/// A-section lead melody over Am F C G / Am F Dm E (written in A minor; the
/// final section replays bars 5–10 transposed +2, then cadences in B minor).
const LEAD_A: [PhraseNote; 24] = [
    (5, 1.0, 1.42, 69, 0.62),
    (5, 2.5, 0.42, 71, 0.58),
    (5, 3.0, 1.92, 72, 0.65),
    (6, 1.0, 0.92, 72, 0.60),
    (6, 2.0, 0.92, 74, 0.62),
    (6, 3.0, 1.42, 72, 0.60),
    (6, 4.5, 0.42, 69, 0.55),
    (7, 1.0, 1.92, 67, 0.60),
    (7, 3.0, 1.92, 64, 0.55),
    (8, 1.0, 0.92, 62, 0.55),
    (8, 2.0, 0.92, 64, 0.58),
    (8, 3.0, 1.92, 67, 0.62),
    (9, 1.0, 1.42, 69, 0.65),
    (9, 2.5, 0.42, 72, 0.60),
    (9, 3.0, 0.92, 71, 0.60),
    (9, 4.0, 0.92, 69, 0.58),
    (10, 1.0, 0.92, 65, 0.58),
    (10, 2.0, 0.92, 69, 0.60),
    (10, 3.0, 1.92, 72, 0.65),
    (11, 1.0, 1.42, 74, 0.68),
    (11, 2.5, 0.42, 72, 0.62),
    (11, 3.0, 1.92, 69, 0.60),
    (12, 1.0, 1.92, 68, 0.60),
    (12, 3.0, 1.42, 71, 0.62),
];

/// Lead re-entry closing the B section (bars 17–20 over F G E E).
const LEAD_B: [PhraseNote; 7] = [
    (17, 1.0, 1.92, 72, 0.60),
    (17, 3.0, 1.92, 69, 0.58),
    (18, 1.0, 1.92, 71, 0.60),
    (18, 3.0, 1.92, 74, 0.62),
    (19, 1.0, 2.42, 76, 0.66),
    (19, 3.5, 1.42, 71, 0.60),
    (20, 1.0, 3.92, 68, 0.58),
];

/// Long crescendo notes through the build.
const LEAD_BUILD: [PhraseNote; 4] = [
    (21, 1.0, 3.92, 69, 0.55),
    (22, 1.0, 3.92, 72, 0.64),
    (23, 1.0, 3.92, 74, 0.72),
    (24, 1.0, 3.92, 76, 0.82),
];

/// Tube duet line under LEAD_B (mostly thirds and sixths below).
const HARMONY_B: [PhraseNote; 7] = [
    (17, 1.0, 1.92, 69, 0.50),
    (17, 3.0, 1.92, 65, 0.48),
    (18, 1.0, 1.92, 67, 0.50),
    (18, 3.0, 1.92, 71, 0.52),
    (19, 1.0, 2.42, 71, 0.54),
    (19, 3.5, 1.42, 68, 0.50),
    (20, 1.0, 3.92, 64, 0.50),
];

/// Sustained chord-tone pads under the final section (written for +2 transpose).
const HARMONY_FINAL: [PhraseNote; 8] = [
    (25, 1.0, 3.8, 64, 0.50),
    (26, 1.0, 3.8, 65, 0.50),
    (27, 1.0, 3.8, 64, 0.50),
    (28, 1.0, 3.8, 62, 0.50),
    (29, 1.0, 3.8, 64, 0.52),
    (30, 1.0, 3.8, 65, 0.52),
    (31, 1.0, 3.8, 68, 0.54),
    (32, 1.0, 6.0, 69, 0.55),
];

fn tube_lead() -> Vec<Note> {
    let mut part = Part::new();
    // Intro pedal under the modal arpeggio.
    part.hit(3, 1.0, 7.8, 64, 0.5);
    push_phrase(&mut part, &LEAD_A, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_B, 0, usize::MAX);
    push_phrase(&mut part, &LEAD_BUILD, 0, usize::MAX);
    part.set_transpose(2);
    // Final section: bars 5–10 of the A melody up a whole step (bars 25–30),
    // then the V-chord lick and a held third of B minor to close.
    push_phrase(&mut part, &LEAD_A, 20, 10);
    part.hit(31, 1.0, 1.92, 68, 0.68);
    part.hit(31, 3.0, 1.42, 71, 0.64);
    part.hit(32, 1.0, 4.42, 72, 0.78);
    part.into_notes()
}

fn tube_harmony() -> Vec<Note> {
    let mut part = Part::new();
    push_phrase(&mut part, &HARMONY_B, 0, usize::MAX);
    part.set_transpose(2);
    push_phrase(&mut part, &HARMONY_FINAL, 0, usize::MAX);
    part.into_notes()
}

/// Push `phrase`, shifting bars by `bar_offset` and dropping entries past `last_bar`.
fn push_phrase(part: &mut Part, phrase: &[PhraseNote], bar_offset: usize, last_bar: usize) {
    for (bar, beat, duration, midi, velocity) in phrase {
        if *bar <= last_bar {
            part.hit(bar + bar_offset, *beat, *duration, *midi, *velocity);
        }
    }
}

// --- Cymbals -----------------------------------------------------------------------

fn cymbal_ride() -> Vec<Note> {
    let mut part = Part::new();
    ride_half_notes(&mut part, 5, 12);
    ride_quarters(&mut part, 13, 20, 0.42, 0.3);
    ride_build(&mut part);
    ride_quarters(&mut part, 25, 31, 0.5, 0.38);
    part.into_notes()
}

fn ride_half_notes(part: &mut Part, first_bar: usize, last_bar: usize) {
    for bar in first_bar..=last_bar {
        part.hit(bar, 1.0, 0.9, 60, 0.45);
        part.hit(bar, 3.0, 0.9, 60, 0.34);
    }
}

fn ride_quarters(part: &mut Part, first_bar: usize, last_bar: usize, accent: f32, base: f32) {
    for bar in first_bar..=last_bar {
        for beat in 0..4 {
            let velocity = if beat == 0 { accent } else { base };
            part.hit(bar, 1.0 + beat as f32, 0.9, 60, velocity);
        }
    }
}

/// Eighth notes with a four-bar crescendo through the build.
fn ride_build(part: &mut Part) {
    for bar in 21..=24 {
        for slot in 0..8 {
            let velocity = 0.3 + 0.05 * (bar - 21) as f32 + 0.012 * slot as f32;
            part.hit(bar, 1.0 + slot as f32 * 0.5, 0.45, 60, velocity);
        }
    }
}

fn cymbal_crash() -> Vec<Note> {
    let mut part = Part::new();
    // Bar-4 swell into the A section downbeat.
    for slot in 0..8 {
        part.hit(
            4,
            1.0 + slot as f32 * 0.5,
            0.45,
            60,
            0.10 + 0.045 * slot as f32,
        );
    }
    part.hit(5, 1.0, 6.0, 60, 0.55);
    part.hit(13, 1.0, 6.0, 60, 0.6);
    part.hit(21, 1.0, 6.0, 60, 0.65);
    part.hit(25, 1.0, 6.0, 60, 0.7);
    part.hit(29, 1.0, 6.0, 60, 0.6);
    part.hit(32, 1.0, 14.0, 60, 0.8);
    part.into_notes()
}
