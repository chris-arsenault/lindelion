//! M1 exit: the speaking-rate estimator ranks the spoken-word fixtures and lands near their
//! `FIXTURES.md` `syl/s` targets. Heavy (loads wav fixtures), so it is gated behind the
//! `integration-tests` feature and run via `make test-integration`, not `make ci`.

use std::path::PathBuf;

use lindelion_sample_library::decode_wav_mono;
use lumedir::speaking_rate::{SpeakingRateConfig, SpeakingRateEstimator};

/// Load a mono fixture from `testdata/audio`, returning its samples and sample rate.
fn fixture(name: &str) -> (Vec<f32>, f32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode fixture");
    (decoded.samples, decoded.sample_rate as f32)
}

/// Speaking rate (syllables/second) measured over the whole fixture.
fn rate(name: &str) -> f32 {
    let (samples, sample_rate) = fixture(name);
    let mut estimator = SpeakingRateEstimator::new(sample_rate, SpeakingRateConfig::default());
    estimator.push(&samples);
    estimator.syllables_per_second()
}

#[test]
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "loads wav fixtures; run via make test-integration"
)]
fn estimator_ranks_and_lands_near_fixture_targets() {
    // FIXTURES.md syl/s targets.
    let slow = rate("speech_slow_48k.wav"); // target 2.8
    let fast = rate("speech_fast_48k.wav"); // target 3.8
    let clean = rate("speech_clean_continuous_48k.wav"); // target 3.7

    // Firm requirement: ranking.
    assert!(slow < fast, "slow ({slow}) should read below fast ({fast})");
    assert!(
        slow < clean,
        "slow ({slow}) should read below clean ({clean})"
    );

    // Softer requirement: proximity to the FIXTURES.md targets.
    const TOL: f32 = 1.0;
    assert!((slow - 2.8).abs() <= TOL, "slow {slow} off target 2.8");
    assert!((fast - 3.8).abs() <= TOL, "fast {fast} off target 3.8");
    assert!((clean - 3.7).abs() <= TOL, "clean {clean} off target 3.7");
}
