//! M2 exit (pause half): the pause-structure estimator lands the pauses fixture near its
//! `FIXTURES.md` `pause` fraction and reads clearly above a continuous fixture. Heavy (loads wav
//! fixtures), so it is gated behind the `integration-tests` feature and run via
//! `make test-integration`, not `make ci`.

use std::path::PathBuf;

use lindelion_sample_library::decode_wav_mono;
use lumedir::pause_structure::{PauseStructure, PauseStructureConfig};

fn fixture(name: &str) -> (Vec<f32>, f32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode fixture");
    (decoded.samples, decoded.sample_rate as f32)
}

fn pauses(name: &str) -> PauseStructure {
    let (samples, sample_rate) = fixture(name);
    let mut estimator = PauseStructure::new(sample_rate, PauseStructureConfig::default());
    estimator.push(&samples);
    estimator
}

#[test]
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "loads wav fixtures; run via make test-integration"
)]
fn pause_metrics_match_the_pauses_fixture() {
    let with_pauses = pauses("speech_pauses_48k.wav"); // FIXTURES pause 0.26
    let continuous = pauses("speech_clean_continuous_48k.wav"); // FIXTURES pause 0.12
    eprintln!(
        "pause_fraction: pauses={:.2} (0.26)  continuous={:.2} (0.12)  count={}",
        with_pauses.pause_fraction(),
        continuous.pause_fraction(),
        with_pauses.pause_count(),
    );

    // Lands near the FIXTURES.md target.
    assert!(
        (with_pauses.pause_fraction() - 0.26).abs() <= 0.1,
        "pauses fixture fraction {} off target 0.26",
        with_pauses.pause_fraction()
    );
    // Reads clearly above a continuous fixture.
    assert!(
        with_pauses.pause_fraction() > continuous.pause_fraction(),
        "pauses ({}) should read above continuous ({})",
        with_pauses.pause_fraction(),
        continuous.pause_fraction()
    );
    // The pauses fixture has at least one qualifying pause.
    assert!(with_pauses.pause_count() >= 1, "expected ≥1 pause run");
}
