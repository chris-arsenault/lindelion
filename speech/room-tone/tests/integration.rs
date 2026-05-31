//! Real-speech enhancement test: proves the effect's claim on a public-domain spoken-word fixture
//! while introducing no artifacts. Run via `make test-models` (kept out of `make ci`). This
//! effect derives its control signal inline, so no worker warm-up is needed.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{assert_no_artifacts, rms};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_room_tone::{PARAM_LEVEL_DB, RoomTone};

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
fn fills_silence_without_artifacts() {
    // Use the pause-rich fixture: the bed should fill the silent gaps (presence ducks it on speech).
    let (dry, sr) = fixture("speech_pauses_48k.wav");
    let mut fx = RoomTone::new();
    fx.prepare(sr, 512);
    fx.set_parameter(PARAM_LEVEL_DB, -24.0);
    let wet = run(&mut fx, &dry);
    // Compare energy in the dry signal's true-silence frames — well below speech level, where the
    // bed is not presence-ducked. (Quieter gaps than the 0.02 quiet-speech floor.)
    let win = 1024;
    let (mut dq, mut wq, mut n) = (0.0_f64, 0.0_f64, 0u32);
    let mut s = 0;
    while s + win <= dry.len() {
        let dr = rms(&dry[s..s + win]);
        if dr < 0.006 {
            dq += (dr * dr) as f64;
            wq += (rms(&wet[s..s + win]).powi(2)) as f64;
            n += 1;
        }
        s += win;
    }
    assert!(n > 0, "fixture must contain silent gaps");
    let (dq, wq) = ((dq / n as f64).sqrt() as f32, (wq / n as f64).sqrt() as f32);
    eprintln!(
        "room-tone: silent-gap rms dry {dq:.5} wet {wq:.5} (+{:.0}%)",
        (wq / dq - 1.0) * 100.0
    );
    assert!(
        wq > dq * 1.5,
        "claim: fills silent gaps with a low-level bed (dry {dq:.5}, wet {wq:.5})"
    );
    assert_no_artifacts(&dry, &wet);
}
