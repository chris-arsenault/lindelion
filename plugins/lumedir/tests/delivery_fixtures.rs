//! M3 exit: clarity behaves sensibly across fixtures, and a complete delivery snapshot is produced.
//!
//! Drives the `DeliveryAggregator` exactly as the off-thread worker does — `SignalAnalyzer` over the
//! fixture in small chunks, the per-chunk snapshot + chunk audio into the aggregator. Clean speech
//! is clearer than its noisy matched pair (noise lowers the voicing ratio), so
//! `clarity(clean) > clarity(noisy)`. Heavy (ONNX over whole fixtures), so it is `#[ignore]`d and run
//! via `make test-models`, not `make ci`.

use std::path::PathBuf;

use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_signals::SignalAnalyzer;
use lumedir::delivery::{DeliveryAggregator, DeliveryConfig, DeliverySnapshot};

const CHUNK: usize = 256; // ≈ one analysis frame, matching the worker's drain size

fn fixture(name: &str) -> (Vec<f32>, u32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode fixture");
    (decoded.samples, decoded.sample_rate)
}

/// Run the analyzer + aggregator over a fixture (as the worker does) and return the snapshot.
fn delivery(name: &str) -> DeliverySnapshot {
    let (samples, sample_rate) = fixture(name);
    let mut analyzer = SignalAnalyzer::new(sample_rate);
    let mut aggregator = DeliveryAggregator::new(sample_rate as f32, DeliveryConfig::default());
    for chunk in samples.chunks(CHUNK) {
        let snapshot = analyzer.process(chunk);
        aggregator.update(chunk, &snapshot);
    }
    aggregator.snapshot()
}

#[test]
#[ignore = "runs SwiftF0 ONNX over fixtures; run via make test-models"]
fn clarity_ranks_clean_above_noisy_and_snapshot_is_complete() {
    let clean = delivery("speech_clean_continuous_48k.wav");
    let noisy = delivery("speech_noisy_48k.wav");
    eprintln!("delivery clean={clean:?}");
    eprintln!("delivery noisy={noisy:?}");

    // Clarity behaves sensibly: clean reads clearer than its noisy matched pair.
    assert!(
        clean.clarity > noisy.clarity,
        "clean clarity ({}) should exceed noisy ({})",
        clean.clarity,
        noisy.clarity
    );

    // A complete delivery snapshot is produced with every field finite and in range.
    for (label, d) in [("clean", &clean), ("noisy", &noisy)] {
        assert!(
            (0.0..=1.0).contains(&d.clarity),
            "{label} clarity out of range: {d:?}"
        );
        assert!(
            (0.0..=1.0).contains(&d.pause_fraction),
            "{label} pause_fraction out of range: {d:?}"
        );
        assert!(
            d.syllables_per_second.is_finite() && d.syllables_per_second >= 0.0,
            "{label} syllables_per_second invalid: {d:?}"
        );
        assert!(
            d.words_per_minute.is_finite() && d.words_per_minute >= 0.0,
            "{label} words_per_minute invalid: {d:?}"
        );
        assert!(
            d.pitch_dynamism_semitones.is_finite() && d.pitch_dynamism_semitones >= 0.0,
            "{label} pitch_dynamism invalid: {d:?}"
        );
    }
}
