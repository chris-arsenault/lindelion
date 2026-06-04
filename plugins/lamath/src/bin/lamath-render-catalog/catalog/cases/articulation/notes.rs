use super::super::super::ScheduledNote;

const V40: f32 = 40.0 / 127.0;
const V70: f32 = 70.0 / 127.0;
const V100: f32 = 100.0 / 127.0;
const V127: f32 = 1.0;

/// Tongued / separato: each scale note starts after the previous one ends (a gap), so every note
/// is a fresh tongued attack.
pub(super) const SCALE_TONGUED: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.22,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.30,
        end_seconds: 0.52,
        note: 62,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.60,
        end_seconds: 0.82,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.90,
        end_seconds: 1.12,
        note: 65,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.20,
        end_seconds: 1.42,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.50,
        end_seconds: 1.72,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.80,
        end_seconds: 2.02,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.10,
        end_seconds: 2.32,
        note: 72,
        velocity: V100,
    },
];

/// Legato: each note's onset arrives before the previous note ends (overlap), so the phrase
/// connects rather than separating.
pub(super) const SCALE_LEGATO: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.33,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.25,
        end_seconds: 0.58,
        note: 62,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.50,
        end_seconds: 0.83,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.75,
        end_seconds: 1.08,
        note: 65,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.00,
        end_seconds: 1.33,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.25,
        end_seconds: 1.58,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.50,
        end_seconds: 1.83,
        note: 71,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.75,
        end_seconds: 2.08,
        note: 72,
        velocity: V100,
    },
];

/// Rapid same-note restrike: C4 struck repeatedly at ~0.15 s spacing, the last hit left to ring.
/// With a single ring-preserving body (`polyphony = 1`, `retrigger = false`) this is a plate hit
/// over and over while it still rings, not twelve fresh voices.
pub(super) const RESTRIKE_C4: [ScheduledNote; 12] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.10,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.15,
        end_seconds: 0.25,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.30,
        end_seconds: 0.40,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.45,
        end_seconds: 0.55,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.60,
        end_seconds: 0.70,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.75,
        end_seconds: 0.85,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.90,
        end_seconds: 1.00,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.05,
        end_seconds: 1.15,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.20,
        end_seconds: 1.30,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.35,
        end_seconds: 1.45,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.50,
        end_seconds: 1.60,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.65,
        end_seconds: 2.40,
        note: 60,
        velocity: V100,
    },
];

/// A held C-major triad (C4/E4/G4), struck together and sustained. Rendered both polyphonically
/// (each note its own struck voice, all ringing) and monophonically (`polyphony = 1`, so the
/// three note-ons steal the one voice - only the latest sounds).
pub(super) const CHORD_CMAJ: [ScheduledNote; 3] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 2.60,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 2.60,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 2.60,
        note: 67,
        velocity: V100,
    },
];

/// Slow ascent C2->C6 hitting only sparse degrees (fourths/fifths/octaves, not every note), each
/// note left to ring a beat. Each step strikes the plate at a different position (note -> strike),
/// so the timbre shifts as it climbs - the clearest audition of the note's effect.
pub(super) const SLOW_CLIMB_C2_C6: [ScheduledNote; 8] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.60,
        note: 36,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.75,
        end_seconds: 1.35,
        note: 43,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.50,
        end_seconds: 2.10,
        note: 48,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.25,
        end_seconds: 2.85,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.00,
        end_seconds: 3.60,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.75,
        end_seconds: 4.35,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 4.50,
        end_seconds: 5.10,
        note: 72,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 5.25,
        end_seconds: 6.10,
        note: 84,
        velocity: V100,
    },
];

/// 16 notes in a fixed non-monotonic (scrambled) order across C2-C6 at a steady ~0.22 s pulse -
/// the strike position jumps around the plate note to note, with no rising/falling trend.
pub(super) const RANDOM_16: [ScheduledNote; 16] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.18,
        note: 60,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.22,
        end_seconds: 0.40,
        note: 43,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.44,
        end_seconds: 0.62,
        note: 79,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.66,
        end_seconds: 0.84,
        note: 48,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.88,
        end_seconds: 1.06,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.10,
        end_seconds: 1.28,
        note: 36,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.32,
        end_seconds: 1.50,
        note: 72,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.54,
        end_seconds: 1.72,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.76,
        end_seconds: 1.94,
        note: 84,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.98,
        end_seconds: 2.16,
        note: 41,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.20,
        end_seconds: 2.38,
        note: 64,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.42,
        end_seconds: 2.60,
        note: 50,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.64,
        end_seconds: 2.82,
        note: 76,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.86,
        end_seconds: 3.04,
        note: 38,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.08,
        end_seconds: 3.26,
        note: 69,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.30,
        end_seconds: 3.48,
        note: 57,
        velocity: V100,
    },
];

/// 16 notes varying all three axes at once: irregular timing (gaps and overlaps), velocity
/// (soft/accent), and pitch: a loose performance phrase exercising dynamics, strike position,
/// and overlap together.
pub(super) const EXPRESSIVE_16: [ScheduledNote; 16] = [
    ScheduledNote {
        start_seconds: 0.00,
        end_seconds: 0.30,
        note: 48,
        velocity: V70,
    },
    ScheduledNote {
        start_seconds: 0.25,
        end_seconds: 0.55,
        note: 55,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 0.60,
        end_seconds: 0.90,
        note: 60,
        velocity: V40,
    },
    ScheduledNote {
        start_seconds: 0.70,
        end_seconds: 1.10,
        note: 72,
        velocity: V127,
    },
    ScheduledNote {
        start_seconds: 1.20,
        end_seconds: 1.45,
        note: 64,
        velocity: V70,
    },
    ScheduledNote {
        start_seconds: 1.35,
        end_seconds: 1.70,
        note: 67,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 1.90,
        end_seconds: 2.30,
        note: 43,
        velocity: V127,
    },
    ScheduledNote {
        start_seconds: 2.10,
        end_seconds: 2.40,
        note: 50,
        velocity: V40,
    },
    ScheduledNote {
        start_seconds: 2.35,
        end_seconds: 2.70,
        note: 79,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 2.55,
        end_seconds: 2.85,
        note: 60,
        velocity: V70,
    },
    ScheduledNote {
        start_seconds: 3.10,
        end_seconds: 3.60,
        note: 36,
        velocity: V127,
    },
    ScheduledNote {
        start_seconds: 3.30,
        end_seconds: 3.65,
        note: 84,
        velocity: V40,
    },
    ScheduledNote {
        start_seconds: 3.80,
        end_seconds: 4.10,
        note: 57,
        velocity: V100,
    },
    ScheduledNote {
        start_seconds: 3.95,
        end_seconds: 4.25,
        note: 69,
        velocity: V70,
    },
    ScheduledNote {
        start_seconds: 4.40,
        end_seconds: 4.80,
        note: 48,
        velocity: V127,
    },
    ScheduledNote {
        start_seconds: 4.55,
        end_seconds: 5.10,
        note: 72,
        velocity: V100,
    },
];
