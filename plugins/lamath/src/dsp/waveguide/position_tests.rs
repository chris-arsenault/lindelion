use lindelion_dsp_utils::analysis::{assert_all_finite, rms_difference};

use crate::dsp::constants::WAVEGUIDE_PICKUP_POSITION;

use super::{WaveguideParams, WaveguideResonator};

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

fn render_impulse(sample_rate: f32, params: WaveguideParams, sample_count: usize) -> Vec<f32> {
    let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
    let mut output = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
    }
    output
}
