//! Standard MIDI File export for the Lamath song arrangement.
//!
//! Ableton imports type-1 SMF files as separate MIDI tracks and reads the track
//! name meta events as lane labels. Each movement gets its own file carrying
//! that movement's tempo and time signature.

use crate::composition::TrackSpec;
use crate::score::{Meter, Note};
use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u4, u7, u15, u24, u28},
};
use std::path::Path;

const PPQ: u16 = 960;

pub(crate) fn write_song_midi(
    path: &Path,
    title: &str,
    meter: Meter,
    specs: &[TrackSpec],
) -> std::io::Result<()> {
    std::fs::write(path, song_midi_bytes(title, meter, specs)?)
}

fn song_midi_bytes(title: &str, meter: Meter, specs: &[TrackSpec]) -> std::io::Result<Vec<u8>> {
    let mut tracks = Vec::with_capacity(specs.len() + 1);
    tracks.push(meta_track(title, meter));
    for spec in specs {
        tracks.push(note_track(
            spec.name,
            instrument_name(spec.name),
            &spec.notes,
            meter,
        ));
    }

    let smf = Smf {
        header: Header {
            format: Format::Parallel,
            timing: Timing::Metrical(u15::new(PPQ)),
        },
        tracks,
    };

    let mut out = Vec::new();
    smf.write_std(&mut out)?;
    Ok(out)
}

fn meta_track(title: &str, meter: Meter) -> Vec<TrackEvent<'_>> {
    vec![
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(title.as_bytes())),
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_micros_per_quarter(
                meter.bpm,
            )))),
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(
                meter.beats_per_bar as u8,
                2,
                24,
                8,
            )),
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        },
    ]
}

fn note_track<'a>(
    name: &'a str,
    instrument: &'static [u8],
    notes: &'a [Note],
    meter: Meter,
) -> Vec<TrackEvent<'a>> {
    let mut events = Vec::with_capacity(notes.len() * 2 + 2);
    events.push(AbsoluteEvent {
        tick: 0,
        order: 0,
        kind: TrackEventKind::Meta(MetaMessage::TrackName(name.as_bytes())),
    });
    events.push(AbsoluteEvent {
        tick: 0,
        order: 1,
        kind: TrackEventKind::Meta(MetaMessage::InstrumentName(instrument)),
    });

    for note in notes {
        let start_tick = note_tick(note.start_seconds, meter.bpm);
        let end_tick = note_tick(note.end_seconds, meter.bpm).max(start_tick.saturating_add(1));
        let note_key = u7::new(note.note.min(127));
        events.push(AbsoluteEvent {
            tick: start_tick,
            order: 3,
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOn {
                    key: note_key,
                    vel: u7::new(midi_velocity(note.velocity)),
                },
            },
        });
        events.push(AbsoluteEvent {
            tick: end_tick,
            order: 2,
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOff {
                    key: note_key,
                    vel: u7::new(0),
                },
            },
        });
    }

    events.sort_by_key(|event| (event.tick, event.order));
    let mut track = Vec::with_capacity(events.len() + 1);
    let mut last_tick = 0;
    for event in events {
        track.push(TrackEvent {
            delta: u28::new(event.tick.saturating_sub(last_tick)),
            kind: event.kind,
        });
        last_tick = event.tick;
    }
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    track
}

#[derive(Debug, Clone)]
struct AbsoluteEvent<'a> {
    tick: u32,
    order: u8,
    kind: TrackEventKind<'a>,
}

fn tempo_micros_per_quarter(bpm: f32) -> u32 {
    (60_000_000.0 / f64::from(bpm)).round() as u32
}

fn note_tick(seconds: f32, bpm: f32) -> u32 {
    let beats = f64::from(seconds) * f64::from(bpm) / 60.0;
    (beats * f64::from(PPQ)).round().clamp(0.0, u32::MAX as f64) as u32
}

fn midi_velocity(velocity: f32) -> u8 {
    (velocity.clamp(0.0, 1.0) * 127.0).round().clamp(1.0, 127.0) as u8
}

fn instrument_name(track_name: &str) -> &'static [u8] {
    match track_name {
        "tube-lead" => b"Lamath Tube Lead",
        "tube-harmony" => b"Lamath Tube Dark",
        "string-chord-low" | "string-chord-mid" | "string-chord-high" => b"Lamath Bowed String",
        "string-bass" => b"Lamath Picked String",
        "modal-arp" => b"Lamath Modal Keys",
        "modal-bells" => b"Lamath Modal Bells",
        "cymbal-ride" => b"Lamath Cymbal Ride",
        "cymbal-crash" => b"Lamath Cymbal Crash",
        "cymbal-tom" => b"Lamath Cymbal Tom",
        _ => b"Lamath",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movements;

    #[test]
    fn movement_midi_is_type_one_with_named_tracks() {
        for movement in movements::all() {
            let specs = movement.tracks();
            let bytes = song_midi_bytes(movement.title, movement.meter, &specs).unwrap();
            let smf = Smf::parse(&bytes).unwrap();

            assert_eq!(smf.header.format, Format::Parallel);
            assert_eq!(smf.tracks.len(), specs.len() + 1);
            assert_eq!(
                track_names(&smf),
                expected_track_names(movement.title, &specs)
            );
        }
    }

    #[test]
    fn movement_midi_preserves_all_note_events() {
        for movement in movements::all() {
            let specs = movement.tracks();
            let expected_note_events = specs.iter().map(|spec| spec.notes.len() * 2).sum::<usize>();
            let bytes = song_midi_bytes(movement.title, movement.meter, &specs).unwrap();
            let smf = Smf::parse(&bytes).unwrap();

            assert_eq!(midi_event_count(&smf), expected_note_events);
        }
    }

    fn track_names(smf: &Smf<'_>) -> Vec<String> {
        smf.tracks
            .iter()
            .filter_map(|track| {
                track.iter().find_map(|event| match event.kind {
                    TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                        Some(String::from_utf8_lossy(name).into_owned())
                    }
                    _ => None,
                })
            })
            .collect()
    }

    fn expected_track_names(title: &str, specs: &[TrackSpec]) -> Vec<String> {
        std::iter::once(title.to_string())
            .chain(specs.iter().map(|spec| spec.name.to_string()))
            .collect()
    }

    fn midi_event_count(smf: &Smf<'_>) -> usize {
        smf.tracks
            .iter()
            .flatten()
            .filter(|event| matches!(event.kind, TrackEventKind::Midi { .. }))
            .count()
    }
}
