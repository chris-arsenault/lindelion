//! Timing and note-list helpers for the song renderer: bar/beat addressing,
//! tempo conversion, and a per-track note builder with section transposition.

pub(crate) const BPM: f32 = 96.0;
pub(crate) const BEATS_PER_BAR: f32 = 4.0;

/// One scheduled note: absolute start/end seconds, MIDI note, normalized velocity.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Note {
    pub(crate) start_seconds: f32,
    pub(crate) end_seconds: f32,
    pub(crate) note: u8,
    pub(crate) velocity: f32,
}

/// A chord shape: the bass root plus three close-voiced upper voices (low to high).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Chord {
    pub(crate) bass: u8,
    pub(crate) voices: [u8; 3],
}

pub(crate) fn beats_to_seconds(beats: f32) -> f32 {
    beats * 60.0 / BPM
}

/// 1-based bar and beat to absolute beats from the top of the song.
pub(crate) fn at(bar: usize, beat: f32) -> f32 {
    (bar as f32 - 1.0) * BEATS_PER_BAR + (beat - 1.0)
}

/// Accumulates one track's notes; `transpose` shifts everything pushed after it,
/// which is how the bar-25 key change re-uses material written in the home key.
#[derive(Debug, Default)]
pub(crate) struct Part {
    notes: Vec<Note>,
    transpose: i32,
}

impl Part {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_transpose(&mut self, semitones: i32) {
        self.transpose = semitones;
    }

    /// Push a note at 1-based `bar`/`beat` lasting `duration_beats`.
    pub(crate) fn hit(
        &mut self,
        bar: usize,
        beat: f32,
        duration_beats: f32,
        midi: u8,
        velocity: f32,
    ) {
        let start_beats = at(bar, beat);
        let note = (i32::from(midi) + self.transpose).clamp(0, 127) as u8;
        self.notes.push(Note {
            start_seconds: beats_to_seconds(start_beats),
            end_seconds: beats_to_seconds(start_beats + duration_beats.max(0.05)),
            note,
            velocity: velocity.clamp(0.0, 1.0),
        });
    }

    pub(crate) fn into_notes(self) -> Vec<Note> {
        self.notes
    }
}
