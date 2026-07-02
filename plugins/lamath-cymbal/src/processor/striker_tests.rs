//! Striker (excitation) tests: the built-in contact-force pulse tables, slot selection,
//! and loaded-sample behavior.

use super::tests::{
    BLOCK, SAMPLE_RATE, builtin_sources, custom_sources, note_on, render_strikes,
    render_with_sources,
};
use super::*;
use lindelion_dsp_utils::analysis::{assert_all_finite, rms_difference, spectral_centroid_hz};

#[test]
fn loaded_excitation_changes_attack() {
    let builtin = render_strikes(CymbalPatch::default(), &[(60, 0, 0.9)], 8_192);
    let custom = render_with_sources(
        CymbalPatch::default(),
        custom_sources(&[0.0, 1.0, -0.75, 0.45, -0.2, 0.0]),
        &[(60, 0, 0.9)],
        8_192,
    );

    assert_all_finite(&custom);
    assert!(rms_difference(&builtin[..2_048], &custom[..2_048]) > 0.000_01);
}

#[test]
fn striker_keyswitch_selects_slot_without_triggering_audio() {
    let mut processor =
        CymbalProcessor::new(SAMPLE_RATE, CymbalPatch::default(), builtin_sources());
    let mut left = [0.0; BLOCK];
    let mut right = [0.0; BLOCK];

    processor.process(&[note_on(2, 1.0)], &mut left, &mut right);

    assert_eq!(processor.selected_slot(), 2);
    assert_eq!(processor.active_injectors(), 0);
    assert!(left.iter().all(|sample| sample.abs() == 0.0));
}

#[test]
fn builtin_striker_slots_have_distinct_attacks() {
    let hard_stick = render_strikes(CymbalPatch::default(), &[(60, 0, 0.9)], 8_192);
    let jazz_brush = render_strikes(CymbalPatch::default(), &[(2, 0, 1.0), (60, 0, 0.9)], 8_192);

    assert_all_finite(&jazz_brush);
    assert!(rms_difference(&hard_stick[..2_048], &jazz_brush[..2_048]) > 0.000_01);
}

#[test]
fn builtin_strikers_are_unipolar_pulses_ordered_by_hardness() {
    // A contact force pulse is unipolar — the striker can only push (Hertzian contact;
    // Rossing/Chaigne contact times). Brightness comes from contact duration, never sign
    // alternation: the membrane-era hard/bell tables oscillated at fs/2, above the
    // plate's modal band entirely.
    for (name, table) in [
        ("hard stick", &HARD_STICK_EXCITATION[..]),
        ("soft mallet", &SOFT_MALLET_EXCITATION[..]),
        ("jazz brush", &JAZZ_BRUSH_EXCITATION[..]),
        ("bell stick", &BELL_STICK_EXCITATION[..]),
    ] {
        assert!(
            table.iter().all(|&sample| sample >= 0.0),
            "{name} must be a unipolar push"
        );
        assert!(table.iter().any(|&sample| sample > 0.0), "{name} is empty");
    }
    let centroid = |table: &[f32]| {
        let mut padded = table.to_vec();
        padded.resize(2_048, 0.0);
        spectral_centroid_hz(&padded, SAMPLE_RATE).unwrap_or(0.0)
    };
    let bell = centroid(&BELL_STICK_EXCITATION);
    let hard = centroid(&HARD_STICK_EXCITATION);
    let mallet = centroid(&SOFT_MALLET_EXCITATION);
    assert!(
        bell > hard && hard > mallet,
        "duration is the hardness axis: bell={bell} hard={hard} mallet={mallet}"
    );
}
