use lindelion_dsp_utils::analysis::{assert_all_finite, dft_magnitude_at, rms_difference};

use crate::dsp::constants::WAVEGUIDE_PICKUP_POSITION;

use super::{WaveguideParams, WaveguideResonator, WaveguideStyle};

#[test]
fn pickup_position_materially_changes_collapsed_loop_render() {
    let sample_rate = 48_000.0;
    let base = WaveguideParams {
        frequency_hz: 240.0,
        loop_filter_cutoff: 14_000.0,
        loop_filter_resonance: 0.05,
        loop_gain: 0.97,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.34,
        ..WaveguideParams::default()
    };
    let bridge_pickup = render_impulse(
        sample_rate,
        WaveguideParams {
            pickup_position: 0.18,
            ..base
        },
        8_192,
    );
    let neck_pickup = render_impulse(
        sample_rate,
        WaveguideParams {
            pickup_position: WAVEGUIDE_PICKUP_POSITION.default,
            ..base
        },
        8_192,
    );

    assert_all_finite(&bridge_pickup);
    assert_all_finite(&neck_pickup);
    assert!(rms_difference(&bridge_pickup[256..], &neck_pickup[256..]) > 0.000_01);
}

#[test]
fn strike_position_notches_expected_harmonics() {
    let sample_rate = 48_000.0;
    let f0 = 220.0;
    // Pickup at 0.1 only notches harmonic 10 and its multiples, so it does not
    // confound the strike-position notches in harmonics 1..=8.
    let render_strike = |strike: f32| {
        render_impulse(
            sample_rate,
            WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: f0,
                loop_filter_cutoff: 18_000.0,
                loop_filter_resonance: 0.0,
                loop_gain: 0.99,
                loop_nonlinearity: 0.0,
                dispersion: 0.0,
                position_of_strike: strike,
                pickup_position: 0.1,
                ..WaveguideParams::default()
            },
            48_000,
        )
    };
    let harmonic = |output: &[f32], n: u32| {
        dft_magnitude_at(&output[8_192..8_192 + 16_384], sample_rate, f0 * n as f32)
    };

    // Striking at 1/4 suppresses the 4th harmonic (sin(n·pi/4) = 0 at n = 4)
    // while its neighbours stay present.
    let quarter = render_strike(0.25);
    assert_all_finite(&quarter);
    let (h3, h4, h5) = (
        harmonic(&quarter, 3),
        harmonic(&quarter, 4),
        harmonic(&quarter, 5),
    );
    assert!(
        h4 < 0.2 * h3.min(h5),
        "4th harmonic should be notched at strike 1/4: h3={h3} h4={h4} h5={h5}"
    );

    // The notch tracks strike position: at 2/5 the 4th returns and the 5th is
    // notched instead, confirming the notch is caused by strike position.
    let two_fifths = render_strike(0.4);
    assert_all_finite(&two_fifths);
    let (h4_moved, h5_moved) = (harmonic(&two_fifths, 4), harmonic(&two_fifths, 5));
    assert!(
        h5_moved < 0.2 * h4_moved,
        "5th harmonic should be notched at strike 2/5: h4={h4_moved} h5={h5_moved}"
    );
}

fn render_impulse(sample_rate: f32, params: WaveguideParams, sample_count: usize) -> Vec<f32> {
    let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
    let mut output = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
    }
    output
}
