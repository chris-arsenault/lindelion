//! Spectral-balance deltas for the M2 radiation stage (ADR-0050): octave-band profiles of
//! the Crash and Ride voicings, reported against the Iowa MIS crash reference recording
//! (`testdata/audio/iowa_cymbal_crash.wav`), plus a loosely pinned regression guard.
//! Profiles are mean-normalized (balance, not level).

use std::path::Path;

use lindelion_dsp_utils::analysis::dft_magnitude_at;
use lindelion_sample_library::decode_wav_mono;

use super::tests::{SAMPLE_RATE, render_strikes};
use crate::patch::CymbalPatch;
use crate::presets::CYMBAL_PRESETS;

const BAND_CENTERS_HZ: [f32; 7] = [250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0];

/// Mean-normalized octave-band energy profile in dB (12 log-spaced probes per band).
fn octave_band_profile(samples: &[f32], sample_rate: f32) -> [f32; 7] {
    let mut profile = [0.0_f32; 7];
    for (band, &center) in BAND_CENTERS_HZ.iter().enumerate() {
        let low = center / std::f32::consts::SQRT_2;
        let probes = 12;
        let energy: f32 = (0..probes)
            .map(|n| low * 2.0_f32.powf(n as f32 / probes as f32))
            .map(|f| {
                let mag = dft_magnitude_at(samples, sample_rate, f);
                mag * mag
            })
            .sum();
        profile[band] = 10.0 * energy.max(1.0e-30).log10();
    }
    let mean = profile.iter().sum::<f32>() / profile.len() as f32;
    for value in &mut profile {
        *value -= mean;
    }
    profile
}

fn preset_patch(name: &str) -> CymbalPatch {
    let preset = CYMBAL_PRESETS
        .iter()
        .find(|preset| preset.name == name)
        .unwrap_or_else(|| panic!("missing preset {name}"));
    let mut patch = CymbalPatch::default();
    preset.apply_to(&mut patch);
    patch
}

fn render_profile(name: &str) -> [f32; 7] {
    let rendered = render_strikes(preset_patch(name), &[(60, 0, 0.9)], 96_000);
    // A clipped render's profile is meaningless: tanh harmonics masquerade as treble
    // (this trap was hit three times across M1–M3).
    let peak = rendered.iter().fold(0.0_f32, |a, &v| a.max(v.abs()));
    assert!(
        peak < 0.985,
        "{name} render is saturated (peak={peak}); profile invalid"
    );
    octave_band_profile(&rendered[..96_000], SAMPLE_RATE)
}

// Balance reporter (M2 step 5): run with
// `cargo test -p lamath-cymbal -- --ignored --nocapture balance_report`.
#[ignore = "balance report; prints octave-band deltas vs the Iowa crash reference"]
#[test]
fn balance_report() {
    let reference_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/audio/iowa_cymbal_crash.wav");
    let reference = decode_wav_mono(&reference_path).expect("decode iowa_cymbal_crash.wav");
    let len = reference
        .samples
        .len()
        .min(2 * reference.sample_rate as usize);
    let reference_profile =
        octave_band_profile(&reference.samples[..len], reference.sample_rate as f32);
    eprintln!("bands (Hz): {BAND_CENTERS_HZ:?}");
    eprintln!("iowa crash reference (rel dB): {reference_profile:?}");
    for name in ["Crash", "Ride"] {
        let profile = render_profile(name);
        let delta: Vec<f32> = profile
            .iter()
            .zip(reference_profile.iter())
            .map(|(render, reference)| render - reference)
            .collect();
        eprintln!("{name} render (rel dB): {profile:?}");
        eprintln!("{name} delta vs reference (dB): {delta:?}");
    }
}

// Regression guard for the radiated balance: pinned from the M2 measurement, ±6 dB per
// band. Catches accidental tilts (a lost radiation corner, a pivot change) without
// constraining M3+ voicing finer than the audition can re-pin.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "multi-second balance render; see make test-integration"
)]
#[test]
fn crash_balance_profile_is_pinned() {
    let profile = render_profile("Crash");
    let pinned: [f32; 7] = [3.3, 0.8, 1.3, 0.4, 6.0, 0.4, -12.1];
    for (band, (measured, expected)) in profile.iter().zip(pinned.iter()).enumerate() {
        assert!(
            (measured - expected).abs() < 6.0,
            "band {} Hz drifted: measured={measured} pinned={expected}",
            BAND_CENTERS_HZ[band]
        );
    }
}
