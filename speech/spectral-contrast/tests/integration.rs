//! Real-speech enhancement test: proves the effect's claim on a public-domain spoken-word fixture
//! while introducing no artifacts. Run via `make test-models` (kept out of `make ci`). This
//! effect derives its control signal inline, so no worker warm-up is needed.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{assert_no_artifacts, spectral_contrast};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_spectral_contrast::{PARAM_AMOUNT_PCT, SpectralContrast};

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
fn raises_spectral_contrast_without_artifacts() {
    let (dry, sr) = fixture("speech_clean_continuous_48k.wav");
    let mut fx = SpectralContrast::new();
    fx.prepare(sr, 512);
    fx.set_parameter(PARAM_AMOUNT_PCT, 50.0);
    let wet = run(&mut fx, &dry);
    let (dc, wc) = (spectral_contrast(&dry, sr), spectral_contrast(&wet, sr));
    eprintln!("spectral-contrast: dry {dc:.2} dB wet {wc:.2} dB");
    assert!(
        wc > dc + 0.5,
        "claim: raises spectral peak-to-valley contrast"
    );
    assert_no_artifacts(&dry, &wet);
}
