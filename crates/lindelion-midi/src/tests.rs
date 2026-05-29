use super::*;

#[test]
fn hard_snap_moves_to_scale_degree() {
    let settings = QuantizeSettings {
        root: RootNote::C,
        scale: Scale::Major,
        snap_mode: SnapMode::Hard,
        ..QuantizeSettings::default()
    };

    // 66.0 (F#) is an exact tie between F (65) and G (67); round-half-up snaps up.
    assert_eq!(snap_midi_note(66.0, &settings), 67);
    assert_eq!(
        snap_midi_note_to_scale(66.0, RootNote::C, &Scale::Major, SnapMode::Hard, 50.0),
        67
    );
}

#[test]
fn soft_snap_preserves_out_of_key_chromatic_note() {
    let settings = QuantizeSettings {
        root: RootNote::C,
        scale: Scale::Major,
        snap_mode: SnapMode::Soft,
        soft_snap_cents: 25.0,
        ..QuantizeSettings::default()
    };

    assert_eq!(snap_midi_note(66.0, &settings), 66);
}

#[test]
fn exact_tie_rounds_half_up_to_higher_scale_degree() {
    // 64.5 is exactly between E (64) and F (65), both in C major; round-half-up
    // resolves the tie to the higher degree.
    assert_eq!(
        nearest_scale_midi_note(64.5, RootNote::C, &Scale::Major),
        65
    );
}

#[test]
fn nearest_scale_midi_note_is_shared_without_quantize_settings() {
    // 66.0 is an exact tie between F (65) and G (67); round-half-up snaps up.
    assert_eq!(
        nearest_scale_midi_note(66.0, RootNote::C, &Scale::Major),
        67
    );
    assert_eq!(
        nearest_scale_midi_note(66.8, RootNote::C, &Scale::Major),
        67
    );
}

#[test]
fn quantize_strength_moves_timing_partway_to_grid() {
    let settings = QuantizeSettings {
        timing_strength: 0.5,
        grid: TimingGrid::Quarter,
        sample_rate: 48_000,
        bpm: 120.0,
        ppq: 960,
        ..QuantizeSettings::default()
    };
    let note = DetectedNote {
        start_sample: 12_000,
        end_sample: 36_000,
        pitch_hz: 440.0,
        peak_rms: 0.5,
        mean_rms: 0.3,
    };

    let notes = quantize_notes(&[note], &settings);

    assert_eq!(notes[0].start_tick, 720);
}

#[test]
fn smf_contains_header_for_empty_clip() {
    let bytes = MidiClip::empty(120).to_smf_bytes().unwrap();

    assert!(bytes.starts_with(b"MThd"));
}

#[test]
fn velocity_amount_zero_is_constant() {
    let settings = QuantizeSettings::default();
    let notes = quantize_notes(
        &[
            DetectedNote {
                start_sample: 0,
                end_sample: 1_000,
                pitch_hz: 440.0,
                peak_rms: 0.01,
                mean_rms: 0.01,
            },
            DetectedNote {
                start_sample: 1_000,
                end_sample: 2_000,
                pitch_hz: 440.0,
                peak_rms: 1.0,
                mean_rms: 0.5,
            },
        ],
        &settings,
    );

    assert_eq!(notes[0].velocity, 100);
    assert_eq!(notes[1].velocity, 100);
}
