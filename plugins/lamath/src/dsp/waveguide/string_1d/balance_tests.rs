//! Source↔body balance (M9) tests for the String waveguide. Split out of
//! `string_1d/tests.rs` to keep each test file within the 600-line size cap.

use super::*;
use lindelion_dsp_utils::analysis::{assert_all_finite, audio_window_metrics, rms, rms_difference};

fn render_string_balance(depth: f32, energy: f32, samples: usize) -> Vec<f32> {
    // Drive the balance energy only (tension off) so the measured difference is the
    // source↔body mix in isolation, not M4 tension modulation. Default style = String.
    let params = WaveguideParams {
        frequency_hz: 220.0,
        loop_filter_cutoff: 9_000.0,
        loop_gain: 0.99,
        position_of_strike: 0.3,
        pickup_position: 0.6,
        source_body_balance: depth,
        ..WaveguideParams::default()
    };
    let mut string = String1d::new(48_000.0);
    let mut out = Vec::with_capacity(samples);
    for index in 0..samples {
        string.set_balance_drive(energy);
        out.push(string.process((index == 0) as u8 as f32, params));
    }
    out
}

#[test]
fn source_body_balance_is_inert_at_zero_depth() {
    // Regression guard: at depth 0 the output is the pre-M9 fixed pickup/body blend,
    // independent of measured energy — so soft and loud render bit-identically and the
    // balance can never change the default patch.
    let soft = render_string_balance(0.0, 0.02, 8_192);
    let loud = render_string_balance(0.0, 1.0, 8_192);
    assert_all_finite(&soft);
    assert!(
        rms_difference(&soft, &loud) == 0.0,
        "depth-0 balance leaked energy"
    );
}

#[test]
fn source_body_balance_shifts_timbre_soft_vs_loud() {
    // With the balance engaged, soft playing leans to the warm body and loud playing
    // to the direct pickup, so soft and loud are a distinct, measurable timbre. The
    // spectral centroid is gain-invariant, so the shift cannot be explained by level;
    // and the equal-power crossfade holds output level (peaks stay comparable).
    let soft = render_string_balance(0.85, 0.02, 8_192);
    let loud = render_string_balance(0.85, 1.0, 8_192);
    assert_all_finite(&soft);
    assert_all_finite(&loud);
    assert!(
        rms(&soft[..4_096]) > 0.0 && rms(&loud[..4_096]) > 0.0,
        "produced silence"
    );

    let soft_centroid = audio_window_metrics(&soft[..4_096], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let loud_centroid = audio_window_metrics(&loud[..4_096], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let relative_shift = (loud_centroid - soft_centroid).abs() / soft_centroid.max(1.0e-3);
    assert!(
        relative_shift > 0.1,
        "balance should shift timbre soft vs loud: soft={soft_centroid} loud={loud_centroid} shift={relative_shift}"
    );

    // Equal-power, level-preserving: the loud/soft peak ratio stays bounded — the
    // change is timbral, not a gain swing (the energy-dependent mix is not a fader).
    let soft_peak = audio_window_metrics(&soft[..4_096], 48_000.0).peak_abs;
    let loud_peak = audio_window_metrics(&loud[..4_096], 48_000.0).peak_abs;
    assert!(
        (loud_peak / soft_peak.max(1.0e-6)) < 2.0,
        "balance must not act as a fader: soft_peak={soft_peak} loud_peak={loud_peak}"
    );
}
