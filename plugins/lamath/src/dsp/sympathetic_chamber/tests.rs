use super::*;
use crate::assert_no_allocations;
use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};
use std::f32::consts::TAU;

const SR: f32 = 48_000.0;

/// Drive the chamber with a sine burst (then silence) and return the left output. In
/// the silent tail (after `burst`) the input is 0, so the output there is the pure
/// sympathetic ring.
fn render_ring(
    chamber: &mut SympatheticChamber,
    freq: f32,
    amplitude: f32,
    total: usize,
    burst: usize,
) -> Vec<f32> {
    let mut left = vec![0.0; total];
    let mut right = vec![0.0; total];
    for index in 0..burst.min(total) {
        let sample = amplitude * (TAU * freq * index as f32 / SR).sin();
        left[index] = sample;
        right[index] = sample;
    }
    chamber.process_block(&mut left, &mut right);
    left
}

#[test]
fn chamber_rings_bounded_and_is_defeatable() {
    let mut chamber = SympatheticChamber::new(SR);
    chamber.set_depth(1.0);
    chamber.note_on(60);
    let out = render_ring(&mut chamber, midi_note_to_hz(60.0), 0.5, 24_000, 8_000);
    assert_all_finite(&out);
    // It rings: energy remains in the tail after the exciting burst ended.
    assert!(
        rms(&out[12_000..]) > 1.0e-3,
        "chamber should ring after excitation"
    );
    // Passive/bounded.
    assert!(
        peak_abs(&out) < 4.0,
        "sympathetic ring must stay bounded: {}",
        peak_abs(&out)
    );

    // Defeatable: at depth 0 the tail (post-burst) is silent.
    let mut defeated = SympatheticChamber::new(SR);
    defeated.set_depth(0.0);
    defeated.note_on(60);
    let dry = render_ring(&mut defeated, midi_note_to_hz(60.0), 0.5, 24_000, 8_000);
    assert_eq!(
        rms(&dry[8_000..]),
        0.0,
        "depth-0 chamber should add no ring"
    );
}

#[test]
fn chamber_resonates_at_the_played_pitch_not_a_fixed_drone() {
    // A string tuned to a note resonates at that pitch but barely responds to a pitch
    // a semitone away — proof the strings track the played notes, not a fixed drone.
    let mut on = SympatheticChamber::new(SR);
    on.set_depth(1.0);
    on.note_on(60);
    let on_out = render_ring(&mut on, midi_note_to_hz(60.0), 0.5, 24_000, 8_000);

    let mut off = SympatheticChamber::new(SR);
    off.set_depth(1.0);
    off.note_on(60);
    let off_out = render_ring(&mut off, midi_note_to_hz(61.0), 0.5, 24_000, 8_000);

    let on_tail = rms(&on_out[12_000..]);
    let off_tail = rms(&off_out[12_000..]);
    assert!(
        on_tail > off_tail * 3.0,
        "string should resonate at its tuned pitch, not a detuned one: on={on_tail} off={off_tail}"
    );
}

#[test]
fn chamber_couples_across_pitches_via_harmonics() {
    // Cross-voice sympathy: a held low note's string blooms when a harmonically
    // related higher note sounds. Tune a C3 string, then excite at C4 (its exact
    // octave / 2nd harmonic) vs an unrelated pitch — the octave rings the C3 string.
    let mut octave = SympatheticChamber::new(SR);
    octave.set_depth(1.0);
    octave.note_on(48); // C3
    let octave_out = render_ring(&mut octave, midi_note_to_hz(60.0), 0.5, 24_000, 8_000); // C4 = 2*C3

    let mut unrelated = SympatheticChamber::new(SR);
    unrelated.set_depth(1.0);
    unrelated.note_on(48);
    let unrelated_out = render_ring(&mut unrelated, midi_note_to_hz(57.0), 0.5, 24_000, 8_000); // A3, no harmonic match

    let octave_tail = rms(&octave_out[12_000..]);
    let unrelated_tail = rms(&unrelated_out[12_000..]);
    assert!(
        octave_tail > unrelated_tail * 2.0,
        "a harmonically related note should ring the held string: octave={octave_tail} unrelated={unrelated_tail}"
    );
}

#[test]
fn chamber_send_scales_super_linearly_with_energy() {
    // The squared energy-scaled send means hard playing blooms far more than soft —
    // a louder excitation rings the string more than proportionally. The burst
    // amplitudes (mix RMS = amplitude/√2) straddle the recalibrated
    // `SYMPATHETIC_SEND_ENERGY_REF` (~0.004, the real output-mix level): soft sits
    // partway up the curve, loud near full send.
    let mut soft = SympatheticChamber::new(SR);
    soft.set_depth(1.0);
    soft.note_on(60);
    let soft_out = render_ring(&mut soft, midi_note_to_hz(60.0), 0.003, 24_000, 8_000);

    let mut loud = SympatheticChamber::new(SR);
    loud.set_depth(1.0);
    loud.note_on(60);
    let loud_out = render_ring(&mut loud, midi_note_to_hz(60.0), 0.015, 24_000, 8_000);

    let soft_tail = rms(&soft_out[12_000..]);
    let loud_tail = rms(&loud_out[12_000..]);
    // 5x the amplitude rings far more than 5x (super-linear), via the squared send.
    assert!(
        loud_tail > soft_tail * 8.0,
        "sympathetic send should scale super-linearly with energy: soft={soft_tail} loud={loud_tail}"
    );
}

#[test]
fn silence_clears_the_sympathetic_tail() {
    let mut chamber = SympatheticChamber::new(SR);
    chamber.set_depth(1.0);
    chamber.note_on(60);
    let _ = render_ring(&mut chamber, midi_note_to_hz(60.0), 0.5, 8_000, 8_000);
    chamber.silence();

    // After silencing, a fresh silent block produces no output.
    let mut left = vec![0.0; 2_048];
    let mut right = vec![0.0; 2_048];
    chamber.process_block(&mut left, &mut right);
    assert_eq!(
        rms(&left),
        0.0,
        "silence() should clear the ringing strings"
    );
}

#[test]
fn chamber_process_does_not_allocate() {
    let mut chamber = SympatheticChamber::new(SR);
    chamber.set_depth(0.8);
    chamber.note_on(48);
    chamber.note_on(55);
    chamber.note_on(60);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    for index in 0..512 {
        let sample = 0.4 * (TAU * 220.0 * index as f32 / SR).sin();
        left[index] = sample;
        right[index] = sample;
    }
    assert_no_allocations("sympathetic_chamber_process", || {
        chamber.process_block(&mut left, &mut right);
    });
}
