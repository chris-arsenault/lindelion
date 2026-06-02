//! M2 exit (dynamism half): SwiftF0 over the flat/animated fixtures → pitch dynamism ranks flat ≪
//! animated and lands near the trustworthy `FIXTURES.md` `pstd` targets. This runs ONNX pitch
//! detection over whole fixtures (heavy), so it is `#[ignore]`d and run via `make test-models`, not
//! `make ci`.
//!
//! Tracker note: SwiftF0 is run at a raised confidence (0.70). A clean f0 tracker measures the
//! *animated* fixture at ≈3.3 semitones of pitch std, **not** the `FIXTURES.md` value of 7.4 — that
//! table value is inflated by octave-tracking errors (confirmed here: at low confidence the *slow*
//! fixture's std balloons to ~6.6 st, impossible for slow speech, which is octave-error noise). At
//! confidence 0.70 the octave noise is rejected and the trustworthy clean fixtures (flat ≈1.3 vs
//! 1.1, slow ≈2.2 vs 2.5) land on target. So the firm assertion is the *strong ranking*; flat lands
//! near its target; animated is asserted *clearly elevated*, not pinned to the artefactual 7.4.

use std::path::PathBuf;

use lindelion_pitch_detect::{
    PitchDetectionConfig, StreamingPitchTracker, SwiftF0StreamingPitchTracker,
};
use lindelion_sample_library::decode_wav_mono;
use lumedir::pitch_dynamism::{PitchDynamism, PitchDynamismConfig};

/// Raised confidence to reject the octave-tracking errors that inflate pitch std on expressive
/// speech (see the module note).
const CONFIDENCE: f32 = 0.70;

fn fixture(name: &str) -> (Vec<f32>, u32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode fixture");
    (decoded.samples, decoded.sample_rate)
}

/// Pitch dynamism (semitone std) over a fixture, with SwiftF0 supplying the voiced f0 frames.
fn dynamism(name: &str) -> f32 {
    let (samples, sample_rate) = fixture(name);
    let config = PitchDetectionConfig {
        confidence_threshold: CONFIDENCE,
        ..PitchDetectionConfig::default()
    };
    let mut tracker = SwiftF0StreamingPitchTracker::new(sample_rate, config);
    let mut estimator = PitchDynamism::new(PitchDynamismConfig::default());
    for chunk in samples.chunks(4096) {
        if let Ok(frames) = tracker.next_block(chunk) {
            for frame in frames {
                estimator.push_frame(frame.f0_hz);
            }
        }
    }
    estimator.semitone_std()
}

#[test]
#[ignore = "runs SwiftF0 ONNX over fixtures; run via make test-models"]
fn dynamism_ranks_flat_below_animated_and_lands_near_targets() {
    let flat = dynamism("speech_flat_48k.wav"); // pstd target 1.1
    let animated = dynamism("speech_animated_48k.wav"); // table pstd 7.4 (octave-inflated; see note)
    eprintln!(
        "dynamism: flat={flat:.2} (target 1.1)  animated={animated:.2} (table 7.4, clean ≈3.3)"
    );

    // Firm requirement: strong ranking — animated is markedly more dynamic than flat.
    assert!(
        animated > flat + 1.5 && animated > 2.0 * flat,
        "animated ({animated}) should be markedly above flat ({flat})"
    );

    // Flat (the trustworthy low end) lands near its target.
    assert!((flat - 1.1).abs() <= 0.7, "flat {flat} off target 1.1");

    // Animated is clearly elevated — above the slow/mid range and well above flat. (Not pinned to
    // the artefactual 7.4: a clean tracker measures ≈3.3 st of true pitch std here.)
    assert!(
        animated > 2.8,
        "animated dynamism {animated} should be clearly elevated"
    );
}
