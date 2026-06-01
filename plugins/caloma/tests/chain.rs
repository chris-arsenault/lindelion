//! The compute-once shared-analysis chain: a chain of all four SwiftF0-consuming effects must
//! construct exactly one analyzer (the shared worker), and a snapshot injected from it must drive
//! each effect to finite output. Heavy — `SharedAnalysis::new` spawns the worker thread and loads
//! the SwiftF0 model — so it is `#[ignore]`d and runs via `make test-models`. This integration
//! binary contains only this test, so the global construction counter has no concurrent writers.

use caloma::analysis::SharedAnalysis;
use lindelion_effect::Effect;
use lindelion_speech_bass_enhancer::BassEnhancer;
use lindelion_speech_consonant_transient::ConsonantTransient;
use lindelion_speech_dynamic_eq::DynamicEq;
use lindelion_speech_signals::analysis_worker_constructions;
use lindelion_speech_upward_expander::UpwardExpander;

#[test]
#[ignore = "builds the real analysis worker (thread + SwiftF0 model); run via `make test-models`"]
fn chain_of_four_consumers_uses_exactly_one_analyzer() {
    let sr = 48_000.0;
    let before = analysis_worker_constructions();

    // The chain owns one shared worker; the four consumers own none.
    let shared = SharedAnalysis::new(sr as u32);
    let mut bass = BassEnhancer::new();
    let mut consonant = ConsonantTransient::new();
    let mut dynamic = DynamicEq::new();
    let mut upward = UpwardExpander::new();
    for fx in [
        &mut bass as &mut dyn Effect,
        &mut consonant,
        &mut dynamic,
        &mut upward,
    ] {
        fx.prepare(sr, 512);
    }

    assert_eq!(
        analysis_worker_constructions() - before,
        1,
        "a chain of all four consumers must construct exactly one analyzer (the shared worker)"
    );

    // Chain-wiring smoke: the shared snapshot, injected into every consumer, yields finite output.
    let block: Vec<f32> = (0..512)
        .map(|i| 0.2 * (std::f32::consts::TAU * 200.0 * i as f32 / sr).sin())
        .collect();
    let snapshot = shared.update(&block);
    bass.set_snapshot(&snapshot);
    consonant.set_snapshot(&snapshot);
    dynamic.set_snapshot(&snapshot);
    upward.set_snapshot(&snapshot);
    for fx in [
        &mut bass as &mut dyn Effect,
        &mut consonant,
        &mut dynamic,
        &mut upward,
    ] {
        let mut buffer = block.clone();
        fx.process(&mut buffer);
        assert!(
            buffer.iter().all(|s| s.is_finite()),
            "every consumer must produce finite output under the injected snapshot"
        );
    }
}
