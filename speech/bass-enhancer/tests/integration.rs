//! Real-speech enhancement test: proves the effect's claim on a public-domain spoken-word fixture
//! while introducing no artifacts. Heavy (the analyzer loads the SwiftF0 model over several
//! seconds), so it is `#[ignore]`d and runs via `make test-models`. Voicing is made deterministic
//! by owning a `SignalAnalyzer` and injecting the per-block snapshot — the same computation the
//! chain performs once per block.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{assert_no_artifacts, band_energy};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_bass_enhancer::{BassEnhancer, PARAM_AMOUNT_PCT};
use lindelion_speech_signals::SignalAnalyzer;

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
fn run(effect: &mut BassEnhancer, analyzer: &mut SignalAnalyzer, dry: &[f32]) -> Vec<f32> {
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

#[test]
#[ignore = "real-speech enhancement; run via `make test-models`"]
fn adds_bass_energy_without_artifacts() {
    let (dry, sr) = fixture("speech_clean_continuous_48k.wav");
    let mut fx = BassEnhancer::new();
    fx.prepare(sr, 512);
    fx.set_parameter(PARAM_AMOUNT_PCT, 100.0);
    let mut analyzer = SignalAnalyzer::new(sr as u32);
    let wet = run(&mut fx, &mut analyzer, &dry);
    let (db, wb) = (
        band_energy(&dry, sr, 80.0, 400.0),
        band_energy(&wet, sr, 80.0, 400.0),
    );
    eprintln!(
        "bass-enhancer: bass band dry {db:.4} wet {wb:.4} (+{:.1}%)",
        (wb / db - 1.0) * 100.0
    );
    assert!(
        wb > db * 1.05,
        "claim: adds bass-band energy (dry {db:.4}, wet {wb:.4})"
    );
    assert_no_artifacts(&dry, &wet);
}
