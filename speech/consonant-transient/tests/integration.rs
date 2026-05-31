//! Real-speech enhancement test: proves the effect's claim on a public-domain spoken-word fixture
//! while introducing no artifacts. Heavy (loads the model/worker over several seconds), so it is
//! `#[ignore]`d and runs via `make test-models`. Worker-driven voicing is made deterministic by
//! the `test-sync-analysis` feature (synchronous analysis); see the signals crate.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{assert_no_artifacts, band_energy};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_consonant_transient::{ConsonantTransient, PARAM_AMOUNT_PCT};

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
fn run(effect: &mut impl Effect, dry: &[f32]) -> Vec<f32> {
    let mut wet = dry.to_vec();
    let mut s = 0;
    while s < wet.len() {
        let e = (s + 512).min(wet.len());
        effect.process(&mut wet[s..e]);
        s = e;
    }
    wet
}

#[test]
#[ignore = "real-speech enhancement; run via `make test-models`"]
fn emphasizes_consonant_transients_without_artifacts() {
    // Consonant/plosive attacks are high-frequency transients; emphasizing them raises HF energy.
    let (dry, sr) = fixture("speech_clean_continuous_48k.wav");
    let mut fx = ConsonantTransient::new();
    fx.prepare(sr, 512);
    fx.set_parameter(PARAM_AMOUNT_PCT, 40.0);
    let wet = run(&mut fx, &dry);
    let (dh, wh) = (
        band_energy(&dry, sr, 3000.0, 12000.0),
        band_energy(&wet, sr, 3000.0, 12000.0),
    );
    eprintln!(
        "consonant-transient: HF band dry {dh:.5} wet {wh:.5} (+{:.1}%)",
        (wh / dh - 1.0) * 100.0
    );
    assert!(
        wh > dh * 1.03,
        "claim: emphasizes consonant transients (HF up)"
    );
    assert_no_artifacts(&dry, &wet);
}
