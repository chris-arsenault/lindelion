//! M6 — per-order full-chain **sanity** gates. For each signal order, at its committed default
//! patch, assert the things that genuinely must not break: finite, non-clipping, noise not worsened,
//! dereverb reduces the late tail, and the reported latency matches the measured group delay. Heavy
//! (NN) → `make test-models`.
//!
//! These are deliberately **robust** gates with generous thresholds, not micro-tuned targets: a
//! default is a starting point the user dials in, so the gates catch a real regression (an effect
//! breaking) without being brittle on one or two fixtures. In particular there is **no loudness
//! gate** (loudness is the user's to set; the limiter provides peak safety) and **no clarity gate**
//! (HF presence is order-dependent — the minimal Light order legitimately reduces it). Both are
//! printed for visibility but not asserted.
//!
//! Reuses the M5 harness + a 3-fixture, 1 s battery (clean / matched noisy↔clean / synthesized
//! reverb) so the run is fast (~16 s release).

use std::path::PathBuf;

use caloma::order::SignalOrder;
use caloma::patch::default_patch_for;
use caloma::tuning::config::default_config;
use caloma::tuning::harness::{BatteryFixture, OrderEvaluator, synthesize_reverb};
use lindelion_sample_library::decode_wav_mono;

const CLIP_LEN: usize = 48_000; // ~1 s
const LATENCY_TOL: i64 = 256; // generous: coloration blurs the best-lag alignment

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .canonicalize()
        .expect("testdata/audio present")
}

fn load(name: &str) -> (Vec<f32>, f32) {
    let decoded = decode_wav_mono(&testdata_dir().join(name)).expect("decode fixture");
    let peak = decoded
        .samples
        .iter()
        .fold(0.0_f32, |m, x| m.max(x.abs()))
        .max(1e-6);
    let gain = 0.25 / peak;
    (
        decoded
            .samples
            .iter()
            .take(CLIP_LEN)
            .map(|s| s * gain)
            .collect(),
        decoded.sample_rate as f32,
    )
}

/// The 3 metric-bearing fixtures: clean, matched noisy↔clean (noise), synthesized reverb (dereverb).
fn battery() -> Vec<BatteryFixture> {
    let (clean, sr) = load("speech_clean_continuous_48k.wav");
    let (noisy, _) = load("speech_noisy_48k.wav");
    vec![
        BatteryFixture {
            name: "clean".into(),
            input: clean.clone(),
            sample_rate: sr,
            clean_reference: None,
            measures_dereverb: false,
            measures_clarity: true,
        },
        BatteryFixture {
            name: "noisy".into(),
            input: noisy,
            sample_rate: sr,
            clean_reference: Some(clean.clone()),
            measures_dereverb: false,
            measures_clarity: false,
        },
        BatteryFixture {
            name: "reverb".into(),
            input: synthesize_reverb(&clean, sr),
            sample_rate: sr,
            clean_reference: None,
            measures_dereverb: true,
            measures_clarity: false,
        },
    ]
}

#[test]
#[ignore = "loads NN models and runs the full chain; run via `make test-models`"]
fn committed_defaults_pass_per_order_fidelity_gates() {
    let cfg = default_config();
    let (clean_probe, _) = load("speech_clean_continuous_48k.wav");

    for order in SignalOrder::ALL {
        let patch = default_patch_for(order);
        let mut ev = OrderEvaluator::new(order, battery(), cfg);
        let cm = ev.evaluate(&patch);
        let noisy = &cm.per_fixture[1];
        let reverb = &cm.per_fixture[2];

        // No artifacts: finite + non-clipping on every fixture.
        assert!(
            cm.per_fixture.iter().all(|f| f.finite),
            "{order:?}: non-finite chain output"
        );
        let peak = cm.per_fixture.iter().fold(0.0_f32, |m, f| m.max(f.peak));
        assert!(
            peak <= cfg.constraints.peak_ceiling,
            "{order:?}: clipping — peak {peak:.3} > {:.3}",
            cfg.constraints.peak_ceiling
        );

        // Noise not worsened (matched noisy↔clean pair, measured through the same chain).
        let nz = noisy.noise.expect("noisy fixture measures noise");
        assert!(
            nz.snr_out_db > nz.snr_in_db - 1.0,
            "{order:?}: noise reduction worsened SNR ({:.1} -> {:.1} dB)",
            nz.snr_in_db,
            nz.snr_out_db
        );

        // Dereverb meaningfully reduces the late tail (≥50%).
        let dv = reverb.dereverb.expect("reverb fixture measures dereverb");
        assert!(
            dv.wet_tail < dv.dry_tail * 0.5,
            "{order:?}: dereverb did not reduce the late tail ({:.5} -> {:.5})",
            dv.dry_tail,
            dv.wet_tail
        );

        // Correct latency: the reported value matches the measured group delay.
        let reported = ev.reported_latency() as i64;
        let measured = ev.measure_latency(&patch, &clean_probe) as i64;
        assert!(
            (measured - reported).abs() <= LATENCY_TOL,
            "{order:?}: latency mismatch — reported {reported}, measured {measured}"
        );

        eprintln!(
            "{order:?}: OK — peak {peak:.3}, noise Δ{:+.1} dB, dereverb ×{:.3}, latency Δ{} | (info) loud {:.1} LUFS",
            nz.snr_out_db - nz.snr_in_db,
            dv.wet_tail / dv.dry_tail.max(1e-12),
            measured - reported,
            cm.per_fixture[0].loudness_lufs,
        );
    }
}
