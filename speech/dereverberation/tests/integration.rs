//! Real-speech enhancement test: proves the effect's claim on a public-domain spoken-word fixture
//! while introducing no artifacts. Run via `make test-models` (kept out of `make ci`). This
//! effect derives its control signal inline, so no worker warm-up is needed.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{assert_no_artifacts, peak, rms};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_dereverberation::{Dereverberation, PARAM_AMOUNT_PCT};

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

fn quiet_rms(x: &[f32]) -> f32 {
    let win = 2048;
    let mut e: Vec<f32> = Vec::new();
    let mut s = 0;
    while s + win <= x.len() {
        e.push(rms(&x[s..s + win]));
        s += win;
    }
    e.sort_by(|a, b| a.total_cmp(b));
    let q = (e.len() / 4).max(1);
    (e[..q].iter().map(|v| v * v).sum::<f32>() / q as f32).sqrt()
}

#[test]
#[ignore = "real-speech enhancement; run via `make test-models`"]
fn suppresses_reverb_tails_without_artifacts() {
    // Synthesize reverb on clean speech (feedback comb → decaying tail fills the gaps), then the
    // dereverberator should reduce the late/gap energy while preserving onsets.
    let (clean, sr) = fixture("speech_pauses_48k.wav");
    let mut reverb = clean.clone();
    let d = (0.04 * sr) as usize;
    for i in d..reverb.len() {
        reverb[i] += 0.5 * reverb[i - d];
    }
    let pk = peak(&reverb);
    if pk > 0.99 {
        for s in reverb.iter_mut() {
            *s *= 0.99 / pk;
        }
    }
    let mut fx = Dereverberation::new();
    fx.prepare(sr, 512);
    fx.set_parameter(PARAM_AMOUNT_PCT, 50.0);
    let wet = run(&mut fx, &reverb);
    let (rt, wt) = (quiet_rms(&reverb), quiet_rms(&wet));
    eprintln!(
        "dereverberation: tail rms reverberant {rt:.5} dereverbed {wt:.5} ({:.1}%)",
        (wt / rt - 1.0) * 100.0
    );
    assert!(wt < rt * 0.9, "claim: suppresses reverb tails");
    // Should approach the dry (pre-reverb) level, not gut the signal: check artifacts vs clean.
    assert_no_artifacts(&clean, &wet);
}
