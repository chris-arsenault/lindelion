//! Timing and note-list helpers for the song renderer: bar/beat addressing,
//! tempo conversion, and a per-track note builder with section transposition.
//!
//! Tempo and meter are per-movement: each movement carries a [`Meter`] and all
//! bar/beat math goes through it, so movements can differ in BPM and beats per
//! bar (the waltz is 3/4) on one timeline.

/// One movement's tempo and time signature.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Meter {
    pub(crate) bpm: f32,
    pub(crate) beats_per_bar: f32,
}

impl Meter {
    pub(crate) const fn new(bpm: f32, beats_per_bar: f32) -> Self {
        Self { bpm, beats_per_bar }
    }

    pub(crate) fn beats_to_seconds(self, beats: f32) -> f32 {
        beats * 60.0 / self.bpm
    }

    /// 1-based bar and beat to absolute beats from the top of the movement.
    pub(crate) fn at(self, bar: usize, beat: f32) -> f32 {
        (bar as f32 - 1.0) * self.beats_per_bar + (beat - 1.0)
    }

    pub(crate) fn seconds_per_bar(self) -> f32 {
        self.beats_to_seconds(self.beats_per_bar)
    }

    /// Frame count for `total_bars` full bars at `sample_rate`.
    pub(crate) fn total_frames(self, total_bars: usize, sample_rate: u32) -> usize {
        (self.beats_to_seconds(total_bars as f32 * self.beats_per_bar) * sample_rate as f32).ceil()
            as usize
    }
}

/// One scheduled note: start/end seconds from the movement top, MIDI note,
/// normalized velocity.
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

/// Accumulates one track's notes; `transpose` shifts everything pushed after it,
/// which is how a key change re-uses material written in the home key.
#[derive(Debug)]
pub(crate) struct Part {
    meter: Meter,
    notes: Vec<Note>,
    transpose: i32,
}

impl Part {
    pub(crate) fn new(meter: Meter) -> Self {
        Self {
            meter,
            notes: Vec::new(),
            transpose: 0,
        }
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
        let start_beats = self.meter.at(bar, beat);
        let note = (i32::from(midi) + self.transpose).clamp(0, 127) as u8;
        self.notes.push(Note {
            start_seconds: self.meter.beats_to_seconds(start_beats),
            end_seconds: self
                .meter
                .beats_to_seconds(start_beats + duration_beats.max(0.05)),
            note,
            velocity: velocity.clamp(0.0, 1.0),
        });
    }

    pub(crate) fn into_notes(self) -> Vec<Note> {
        self.notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_addresses_three_four_bars() {
        let waltz = Meter::new(150.0, 3.0);
        assert_eq!(waltz.at(1, 1.0), 0.0);
        assert_eq!(waltz.at(2, 1.0), 3.0);
        assert_eq!(waltz.at(3, 2.5), 7.5);
        let bar_seconds = waltz.seconds_per_bar();
        assert!((bar_seconds - 1.2).abs() < 1.0e-6);
    }

    #[test]
    fn meter_total_frames_matches_olorelinde_reference() {
        // 36 bars of 4/4 at 96 BPM is exactly 90 s = 4_320_000 frames at 48 kHz.
        let meter = Meter::new(96.0, 4.0);
        assert_eq!(meter.total_frames(36, 48_000), 4_320_000);
    }

    #[test]
    fn part_applies_meter_and_transpose() {
        let mut part = Part::new(Meter::new(120.0, 4.0));
        part.set_transpose(2);
        part.hit(2, 1.0, 1.0, 60, 0.5);
        let notes = part.into_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note, 62);
        assert!((notes[0].start_seconds - 2.0).abs() < 1.0e-6);
        assert!((notes[0].end_seconds - 2.5).abs() < 1.0e-6);
    }
}
