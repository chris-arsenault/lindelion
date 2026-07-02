//! The symphony's movements. Each movement is a self-contained score — its
//! own meter, bar count, and tracks — rendered to its own mix and stems, then
//! concatenated (with gaps) into the combined symphony master.

pub(crate) mod adagio;
pub(crate) mod battle;
mod battle_lead;
pub(crate) mod finale;
mod finale_lead;
pub(crate) mod olorelinde;
pub(crate) mod waltz;

use crate::composition::TrackSpec;
use crate::render::SAMPLE_RATE;
use crate::score::Meter;

/// One movement: identity, meter, length, and a score function.
pub(crate) struct Movement {
    /// Directory / file-name slug.
    pub(crate) slug: &'static str,
    /// Display title (also the MIDI song-track name).
    pub(crate) title: &'static str,
    pub(crate) meter: Meter,
    /// Total bars including any ring-out tail bars.
    pub(crate) total_bars: usize,
    tracks: fn(Meter) -> Vec<TrackSpec>,
}

impl Movement {
    pub(crate) fn total_frames(&self) -> usize {
        self.meter.total_frames(self.total_bars, SAMPLE_RATE)
    }

    pub(crate) fn tracks(&self) -> Vec<TrackSpec> {
        (self.tracks)(self.meter)
    }
}

/// Every movement, in performance order.
pub(crate) fn all() -> Vec<Movement> {
    vec![
        Movement {
            slug: "olorelinde",
            title: "I. Olorelinde (The Dream Ascending)",
            meter: olorelinde::METER,
            total_bars: olorelinde::TOTAL_BARS,
            tracks: olorelinde::tracks,
        },
        Movement {
            slug: "battle",
            title: "II. Battle",
            meter: battle::METER,
            total_bars: battle::TOTAL_BARS,
            tracks: battle::tracks,
        },
        Movement {
            slug: "adagio",
            title: "III. Adagio",
            meter: adagio::METER,
            total_bars: adagio::TOTAL_BARS,
            tracks: adagio::tracks,
        },
        Movement {
            slug: "waltz",
            title: "IV. Waltz",
            meter: waltz::METER,
            total_bars: waltz::TOTAL_BARS,
            tracks: waltz::tracks,
        },
        Movement {
            slug: "finale",
            title: "V. Finale",
            meter: finale::METER,
            total_bars: finale::TOTAL_BARS,
            tracks: finale::tracks,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_slugs_are_unique() {
        let movements = all();
        for (index, movement) in movements.iter().enumerate() {
            assert!(
                movements[index + 1..]
                    .iter()
                    .all(|other| other.slug != movement.slug),
                "duplicate movement slug {}",
                movement.slug
            );
        }
    }

    #[test]
    fn every_track_has_notes_inside_the_movement() {
        for movement in all() {
            let end_seconds = movement.total_frames() as f32 / SAMPLE_RATE as f32;
            for track in movement.tracks() {
                assert!(
                    !track.notes.is_empty(),
                    "{}/{} is empty",
                    movement.slug,
                    track.name
                );
                for note in &track.notes {
                    assert!(
                        note.start_seconds >= 0.0 && note.start_seconds < end_seconds,
                        "{}/{} note starts at {}s outside movement ({}s)",
                        movement.slug,
                        track.name,
                        note.start_seconds,
                        end_seconds
                    );
                }
            }
        }
    }
}
