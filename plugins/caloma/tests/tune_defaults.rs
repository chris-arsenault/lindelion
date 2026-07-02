//! The offline **default-tuning** generator (M5 Step 6). For each signal order it runs seeded
//! coordinate descent over the order's search space, scoring candidates by the full-chain battery
//! objective, and writes the winning patch into `src/defaults/<order>.toml` — the committed built-in
//! default that `default_patch_for` loads. Deterministic (fixed seed, synchronous analyzer), so the
//! result is reproducible and reviewable in diffs.
//!
//! Heavy: it loads the DFN3/Silero/SwiftF0 models and runs the chain many times. `#[ignore]`d; run
//! via `make tune-defaults`. The search uses a representative fixture subset and trimmed clips to
//! keep the offline run tractable; the **full** battery is validated by the committed-defaults
//! regression (`make test-models`).

use std::fs;
use std::path::PathBuf;

use caloma::order::SignalOrder;
use caloma::patch::default_patch_for;
use caloma::patch_io;
use caloma::tuning::config::{default_config, search_space};
use caloma::tuning::harness::{BatteryFixture, OrderEvaluator, synthesize_reverb};
use caloma::tuning::search::{SearchOpts, coordinate_descent};
use lindelion_sample_library::decode_wav_mono;

/// Trim every clip to this many samples for the search (≈ 1 s at 48 kHz) to bound the offline run.
/// DFN3 inference per block dominates the cost, so the search uses short clips + a single coordinate
/// pass; the committed defaults are then validated on the full battery by the regression.
const SEARCH_CLIP_LEN: usize = 48_000;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .canonicalize()
        .expect("testdata/audio present")
}

/// Load a fixture, scale it to a realistic ~−12 dBFS operating level, and trim it for the search.
fn load(name: &str) -> (Vec<f32>, f32) {
    let decoded = decode_wav_mono(&testdata_dir().join(name)).expect("decode fixture");
    let peak = decoded
        .samples
        .iter()
        .fold(0.0_f32, |m, x| m.max(x.abs()))
        .max(1e-6);
    let gain = 0.25 / peak;
    let samples: Vec<f32> = decoded
        .samples
        .iter()
        .take(SEARCH_CLIP_LEN)
        .map(|s| s * gain)
        .collect();
    (samples, decoded.sample_rate as f32)
}

/// The representative search battery: clean (clarity + coloration), the matched noisy↔clean pair
/// (noise), and a synthesized-reverb variant of the clean clip (dereverb).
fn search_battery() -> Vec<BatteryFixture> {
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

fn defaults_path(order: SignalOrder) -> PathBuf {
    let name = match order {
        SignalOrder::Clarity => "clarity",
        SignalOrder::Broadcast => "broadcast",
        SignalOrder::Light => "light",
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("src/defaults/{name}.toml"))
}

#[test]
#[ignore = "loads NN models and runs the full chain many times; run via `make tune-defaults`"]
fn tune_committed_defaults() {
    let cfg = default_config();
    let opts = SearchOpts {
        seed: 0xCA10_0DEF,
        max_passes: 1,
        restarts: 0,
    };

    for order in SignalOrder::ALL {
        let dims = search_space(order);
        let mut evaluator = OrderEvaluator::new(order, search_battery(), cfg);
        // Start each tune from a neutral gain staging so a previous run's committed levels never
        // carry over (the output level is not searched, so it must start — and stay — at unity).
        let mut start = default_patch_for(order);
        start.input_level_db = 0.0;
        start.output_level_db = 0.0;
        let baseline = evaluator.objective(&start);

        let mut evals = 0usize;
        let mut scores_seen: Vec<f32> = Vec::new();
        let (best, best_score) = coordinate_descent(
            start,
            &dims,
            |p| {
                evals += 1;
                if evals.is_multiple_of(20) {
                    eprintln!("  {order:?}: {evals} evals…");
                }
                let s = evaluator.objective(p);
                if let Some(s) = s {
                    scores_seen.push(s);
                }
                s
            },
            opts,
        );

        eprintln!(
            "{order:?}: baseline {baseline:?} -> tuned {best_score:.4} over {} dims ({evals} evals)",
            dims.len()
        );
        assert!(
            best_score.is_finite() && best_score > 0.0,
            "{order:?}: tuned patch must pass the hard constraints and score > 0"
        );
        // Guard against a flat objective: with continuous metrics, dozens of candidates spread
        // across every dimension's grid must produce more than one distinct score. A flat
        // objective means the evaluator is not applying the candidate patches (the search would
        // silently "confirm" whatever it started from).
        let min = scores_seen.iter().copied().fold(f32::INFINITY, f32::min);
        let max = scores_seen
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 1e-5,
            "{order:?}: objective is flat across {} candidates (all scored {max}) — \
             candidate patches are not reaching the chain",
            scores_seen.len()
        );

        // The committed default = the search's tonal/dynamics tuning at **unity gain staging**.
        // Loudness is a user/editor concern, not a baked-in default: a default should be a robust
        // starting point, not a mastering target. (Earlier experiments chasing −16 LUFS by driving
        // the input were brittle — heavy drive wrecked noise/dereverb — so gain staging stays unity
        // and the limiter provides peak safety.)
        let mut tuned = best;
        tuned.input_level_db = 0.0;
        tuned.output_level_db = 0.0;
        tuned.order = order;

        let toml = patch_io::to_toml_string(&tuned).expect("serialize tuned patch");
        fs::write(defaults_path(order), toml).expect("write committed default");
    }
}
