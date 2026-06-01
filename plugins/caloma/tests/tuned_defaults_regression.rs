//! Committed-defaults regression (M5 Step 7). For each signal order, run the **full** spoken-word
//! battery on the committed built-in default (`default_patch_for`) and assert it still passes the
//! hard constraints and scores at or above the configured floor. A future effect change that
//! degrades a default trips this (run via `make test-models`) and prompts a re-tune, rather than
//! silently shipping worse defaults.
//!
//! Heavy: loads the NN models and runs the full chain on every fixture. `#[ignore]`d.

use std::path::PathBuf;

use caloma::order::SignalOrder;
use caloma::patch::default_patch_for;
use caloma::tuning::config::default_config;
use caloma::tuning::harness::{BatteryFixture, OrderEvaluator, synthesize_reverb};
use caloma::tuning::score::score;
use lindelion_sample_library::decode_wav_mono;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .canonicalize()
        .expect("testdata/audio present")
}

/// Trim each clip to ~1 s — enough for stable loudness/SNR/dereverb/coloration, and small enough that
/// the 16-slot NN chain runs fast even in the debug `make test-models` path (~3 min for the battery).
const REGRESSION_CLIP_LEN: usize = 48_000;

/// Load a fixture at a realistic ~−12 dBFS operating level.
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
            .take(REGRESSION_CLIP_LEN)
            .map(|s| s * gain)
            .collect(),
        decoded.sample_rate as f32,
    )
}

/// The regression battery: the three metric-bearing fixtures — clean (loudness + clarity +
/// coloration), the matched noisy↔clean pair (noise), and a synthesized-reverb variant (dereverb).
/// These three exercise every scored term + both hard constraints, so they suffice as a degradation
/// guard while keeping the run cheap (the exhaustive delivery sweep is the search's job, not the
/// guard's).
fn regression_battery() -> Vec<BatteryFixture> {
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
#[ignore = "loads NN models and runs the full battery; run via `make test-models`"]
fn committed_defaults_score_above_the_floor() {
    let cfg = default_config();
    for order in SignalOrder::ALL {
        let mut evaluator = OrderEvaluator::new(order, regression_battery(), cfg);
        let candidate = evaluator.evaluate(&default_patch_for(order));
        let scored = score(&candidate, &cfg);
        eprintln!("{order:?}: committed-default battery score {scored:?}");
        let value = scored.unwrap_or_else(|| {
            panic!("{order:?}: committed default violates a hard constraint on the full battery")
        });
        assert!(
            value >= cfg.regression_floor,
            "{order:?}: committed default scored {value:.4} < floor {:.4} — re-tune (make tune-defaults)",
            cfg.regression_floor
        );
    }
}
