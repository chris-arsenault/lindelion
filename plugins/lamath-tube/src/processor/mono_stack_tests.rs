//! Monophonic note-stack and register-break transition behavior: trill returns to the held
//! key, stack pop ordering, the cross-break dip transition, and audio-thread allocation
//! freedom across the full mono lifecycle.

use super::tests::{
    default_sources, note_off, note_on, render_held_note, render_phrase, sustained_cents_error,
    sustained_dbfs,
};
use super::*;
use lindelion_dsp_utils::analysis::{assert_all_finite, max_adjacent_delta};

const SAMPLE_RATE: f32 = 48_000.0;

/// Monophonic note stack: releasing a trilled note while the first key is still down must
/// return to the held note (finger-lift slur), not cut to silence.
#[test]
fn trill_release_returns_to_the_held_note() {
    let phrase = render_phrase(&[(60, 0.0, 2.0, 0.9), (62, 0.5, 0.8, 0.9)]);
    assert_all_finite(&phrase);
    let trill_window = &phrase[(0.55 * SAMPLE_RATE) as usize..(0.78 * SAMPLE_RATE) as usize];
    let trill_cents = sustained_cents_error(trill_window, 62);
    assert!(
        trill_cents.abs() < 25.0,
        "trilled note should sound while down: {trill_cents:+.1} cents"
    );
    let return_window = &phrase[(1.4 * SAMPLE_RATE) as usize..(1.9 * SAMPLE_RATE) as usize];
    assert!(
        sustained_dbfs(return_window) > -30.0,
        "held note should keep sounding after the trill: {:.1} dBFS",
        sustained_dbfs(return_window)
    );
    let return_cents = sustained_cents_error(return_window, 60);
    assert!(
        return_cents.abs() < 25.0,
        "release should return to the held note's pitch: {return_cents:+.1} cents"
    );
}

/// The note stack pops in most-recent-first order and releases to silence only when empty.
#[test]
fn note_stack_pops_in_order_and_releases_when_empty() {
    let phrase = render_phrase(&[
        (60, 0.0, 2.6, 0.9),
        (64, 0.2, 2.2, 0.9),
        (67, 0.4, 1.0, 0.9),
    ]);
    assert_all_finite(&phrase);
    let window = |start: f32, end: f32| {
        &phrase[(start * SAMPLE_RATE) as usize..(end * SAMPLE_RATE) as usize]
    };
    // 67 released at 1.0 -> back to 64; 64 released at 2.2 -> back to 60; 60 released at 2.6.
    let back_to_64 = sustained_cents_error(window(1.5, 2.1), 64);
    assert!(
        back_to_64.abs() < 25.0,
        "first pop should return to the next held key: {back_to_64:+.1} cents"
    );
    let back_to_60 = sustained_cents_error(window(2.35, 2.58), 60);
    assert!(
        back_to_60.abs() < 25.0,
        "second pop should return to the bottom key: {back_to_60:+.1} cents"
    );
    let tail = window(3.0, 3.1);
    assert!(
        sustained_dbfs(tail) < sustained_dbfs(window(2.35, 2.58)),
        "empty stack should release"
    );
}

/// A trill over the register break: the return to the held note crosses the break through the
/// dip transition and must land back on the held pitch, audibly.
#[test]
fn trill_across_the_break_returns_through_the_dip() {
    let phrase = render_phrase(&[(67, 0.0, 2.4, 0.9), (71, 0.5, 0.9, 0.9)]);
    assert_all_finite(&phrase);
    let return_window = &phrase[(1.7 * SAMPLE_RATE) as usize..(2.3 * SAMPLE_RATE) as usize];
    assert!(
        sustained_dbfs(return_window) > -30.0,
        "held note should speak again after the cross-break trill: {:.1} dBFS",
        sustained_dbfs(return_window)
    );
    let return_cents = sustained_cents_error(return_window, 67);
    assert!(
        return_cents.abs() < 25.0,
        "cross-break return should land on the held note: {return_cents:+.1} cents"
    );
}

/// Legato runs crossing the register break must not snap: inside the crossing windows the
/// per-sample step must stay bounded by the steeper of the two held notes' own waveform steps
/// (the dip transition swells through near-silence instead of stepping the topology).
#[test]
fn legato_break_crossing_does_not_snap() {
    let velocity = 100.0 / 127.0;
    let held_low = max_adjacent_delta(&render_held_note(
        TubePatch::default(),
        67,
        velocity,
        48_000,
    ));
    let held_high = max_adjacent_delta(&render_held_note(
        TubePatch::default(),
        71,
        velocity,
        48_000,
    ));
    let held = held_low.max(held_high);
    // Overlapping notes walking up across the break (legato run G4 -> A4 -> B4).
    let run = render_phrase(&[
        (67, 0.0, 0.42, velocity),
        (69, 0.4, 0.82, velocity),
        (71, 0.8, 1.6, velocity),
    ]);
    assert_all_finite(&run);
    let crossing_window = |start: f32, end: f32| {
        max_adjacent_delta(&run[(start * SAMPLE_RATE) as usize..(end * SAMPLE_RATE) as usize])
    };
    let first = crossing_window(0.38, 0.6);
    let second = crossing_window(0.78, 1.0);
    assert!(
        first <= held * 1.2 && second <= held * 1.2,
        "cross-break legato should not snap: crossings {first}/{second} vs held {held}"
    );
}

/// The full mono lifecycle (trill, stack pops, cross-break dips) stays allocation-free on the
/// audio thread.
#[test]
fn trill_and_break_transition_do_not_allocate() {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    let mut left = [0.0; 256];
    let mut right = [0.0; 256];
    let hold = [note_on(67, 0.8)];
    let trill_on = [note_on(71, 0.8)];
    let trill_off = [note_off(71)];
    let release = [note_off(67)];

    crate::assert_no_allocations("lamath_tube_trill_transition", || {
        processor.process(&hold, &mut left, &mut right);
        processor.process(&trill_on, &mut left, &mut right);
        processor.process(&trill_off, &mut left, &mut right);
        processor.process(&release, &mut left, &mut right);
    });
}
