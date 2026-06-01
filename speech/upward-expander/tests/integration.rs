//! Real-speech enhancement test: proves the effect's claim on a public-domain spoken-word fixture
//! while introducing no artifacts. Heavy (the analyzer loads the SwiftF0 model over several
//! seconds), so it is `#[ignore]`d and runs via `make test-models`. Voicing state is made
//! deterministic by owning a `SignalAnalyzer` and injecting the per-block snapshot — the same
//! computation the chain performs once per block.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{assert_no_artifacts, rms};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_signals::SignalAnalyzer;
use lindelion_speech_upward_expander::{PARAM_AMOUNT_PCT, PARAM_THRESHOLD_DB, UpwardExpander};

fn fixture(name: &str) -> (Vec<f32>, f32) {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .expect("fixture present");
    let d = decode_wav_mono(&p).expect("decode fixture");
    // Scale to a realistic ~-12 dBFS operating level (the fixtures are peak-normalized to -3 dBFS),
    // giving enhancement boosts headroom so a clean effect does not clip.
    let pk = d
        .samples
        .iter()
        .fold(0.0_f32, |m, x| m.max(x.abs()))
        .max(1e-6);
    let g = 0.25 / pk;
    (
        d.samples.iter().map(|s| s * g).collect(),
        d.sample_rate as f32,
    )
}

/// Process `dry` block by block, injecting the per-block analysis snapshot (computed from the dry
/// input, as the chain does) before each `process` call.
fn run(effect: &mut UpwardExpander, analyzer: &mut SignalAnalyzer, dry: &[f32]) -> Vec<f32> {
    let mut wet = dry.to_vec();
    let mut s = 0;
    while s < wet.len() {
        let e = (s + 512).min(wet.len());
        let snapshot = analyzer.process(&wet[s..e]);
        effect.set_snapshot(&snapshot);
        effect.process(&mut wet[s..e]);
        s = e;
    }
    wet
}

/// RMS over the frames the dry signal leaves quiet — the 10th–40th percentile of frame energy
/// (above silence, below the speech peaks), measured on the same frames in both signals. Upward
/// expansion should raise this without touching the peaks.
fn quiet_detail(dry: &[f32], wet: &[f32]) -> (f32, f32) {
    let win = 1024;
    let mut frames: Vec<(f32, usize)> = Vec::new();
    let mut s = 0;
    while s + win <= dry.len() {
        frames.push((rms(&dry[s..s + win]), s));
        s += win;
    }
    frames.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (lo, hi) = (frames.len() / 10, frames.len() * 4 / 10);
    let quiet = &frames[lo..hi];
    let energy = |sig: &[f32]| {
        (quiet
            .iter()
            .map(|&(_, st)| rms(&sig[st..st + win]).powi(2))
            .sum::<f32>()
            / quiet.len() as f32)
            .sqrt()
    };
    (energy(dry), energy(wet))
}

#[test]
#[ignore = "real-speech enhancement; run via `make test-models`"]
fn lifts_quiet_detail_without_artifacts() {
    let (dry, sr) = fixture("speech_pauses_48k.wav");
    let mut fx = UpwardExpander::new();
    fx.prepare(sr, 512);
    fx.set_parameter(PARAM_AMOUNT_PCT, 100.0);
    fx.set_parameter(PARAM_THRESHOLD_DB, -20.0);
    let mut analyzer = SignalAnalyzer::new(sr as u32);
    let wet = run(&mut fx, &mut analyzer, &dry);
    let (dq, wq) = quiet_detail(&dry, &wet);
    eprintln!(
        "upward-expander: quiet-detail rms dry {dq:.5} wet {wq:.5} (+{:.1}%)",
        (wq / dq - 1.0) * 100.0
    );
    assert!(
        wq > dq * 1.05,
        "claim: lifts quiet detail (dry {dq:.5}, wet {wq:.5})"
    );
    assert_no_artifacts(&dry, &wet);
}
