use midly::{MetaMessage, Smf, Timing, TrackEventKind};

use super::{
    DEFAULT_PPQ, DetectedNote, MidiClip, QuantizeSettings, TimingGrid, clip_from_detected_notes,
    hz_from_midi_note, quantize_notes,
};

#[test]
fn smf_contains_clip_tempo_and_time_signature() {
    let clip = MidiClip::empty_with_time_signature(135, 7, 8);

    let bytes = clip.to_smf_bytes().unwrap();
    let smf = Smf::parse(&bytes).unwrap();

    assert_eq!(ppq(&smf), Some(DEFAULT_PPQ));
    assert_eq!(tempo_meta(&smf), Some(60_000_000 / 135));
    assert_eq!(time_signature_meta(&smf), Some((7, 3, 24, 8)));
}

#[test]
fn smf_tempo_rounds_for_non_divisor_bpm() {
    // 60_000_000 / 127 = 472440.94..., which must round to 472441, not truncate.
    let bytes = MidiClip::empty_with_time_signature(127, 4, 4)
        .to_smf_bytes()
        .unwrap();
    let smf = Smf::parse(&bytes).unwrap();

    assert_eq!(tempo_meta(&smf), Some(472_441));
}

#[test]
fn empty_capture_exports_metadata_only() {
    let bytes = MidiClip::empty_with_time_signature(90, 3, 4)
        .to_smf_bytes()
        .unwrap();
    let smf = Smf::parse(&bytes).unwrap();

    // 60_000_000 / 90 = 666666.67..., rounded (not truncated) to 666667.
    assert_eq!(tempo_meta(&smf), Some(666_667));
    assert_eq!(time_signature_meta(&smf), Some((3, 2, 24, 8)));
    assert_eq!(midi_event_count(&smf), 0);
}

#[test]
fn short_notes_are_extended_to_minimum_duration() {
    let notes = quantize_notes(
        &[DetectedNote {
            start_sample: 0,
            end_sample: 1,
            pitch_hz: hz_from_midi_note(60.0),
            peak_rms: 0.5,
            mean_rms: 0.5,
        }],
        &QuantizeSettings {
            ppq: DEFAULT_PPQ,
            ..QuantizeSettings::default()
        },
    );

    assert_eq!(notes[0].duration_ticks, u32::from(DEFAULT_PPQ / 16));
}

#[test]
fn quantized_notes_do_not_overlap() {
    let settings = QuantizeSettings {
        grid: TimingGrid::Quarter,
        sample_rate: 48_000,
        bpm: 120.0,
        ppq: DEFAULT_PPQ,
        ..QuantizeSettings::default()
    };
    let notes = quantize_notes(
        &[
            detected_note(0, 8_000, 60.0),
            detected_note(1_000, 9_000, 62.0),
            detected_note(2_000, 10_000, 64.0),
        ],
        &settings,
    );

    for pair in notes.windows(2) {
        assert!(pair[0].start_tick + pair[0].duration_ticks <= pair[1].start_tick);
    }
}

#[test]
fn clip_from_detected_notes_uses_capture_musical_context() {
    let settings = QuantizeSettings {
        bpm: 128.0,
        time_signature_numerator: 5,
        time_signature_denominator: 8,
        ..QuantizeSettings::default()
    };

    let clip = clip_from_detected_notes(&[detected_note(0, 24_000, 60.0)], &settings);

    assert_eq!(clip.bpm, 128);
    assert_eq!(clip.time_signature_numerator, 5);
    assert_eq!(clip.time_signature_denominator, 8);
}

fn detected_note(start_sample: usize, end_sample: usize, midi_note: f32) -> DetectedNote {
    DetectedNote {
        start_sample,
        end_sample,
        pitch_hz: hz_from_midi_note(midi_note),
        peak_rms: 0.5,
        mean_rms: 0.5,
    }
}

fn tempo_meta(smf: &Smf<'_>) -> Option<u32> {
    smf.tracks
        .iter()
        .flatten()
        .find_map(|event| match event.kind {
            TrackEventKind::Meta(MetaMessage::Tempo(tempo)) => Some(tempo.as_int()),
            _ => None,
        })
}

fn time_signature_meta(smf: &Smf<'_>) -> Option<(u8, u8, u8, u8)> {
    smf.tracks
        .iter()
        .flatten()
        .find_map(|event| match event.kind {
            TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, c, b)) => Some((n, d, c, b)),
            _ => None,
        })
}

fn midi_event_count(smf: &Smf<'_>) -> usize {
    smf.tracks
        .iter()
        .flatten()
        .filter(|event| matches!(event.kind, TrackEventKind::Midi { .. }))
        .count()
}

fn ppq(smf: &Smf<'_>) -> Option<u16> {
    match smf.header.timing {
        Timing::Metrical(ppq) => Some(ppq.as_int()),
        Timing::Timecode(_, _) => None,
    }
}
