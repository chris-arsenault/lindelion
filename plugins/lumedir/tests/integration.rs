//! Lúmedir M0 integration: the passthrough processor feeds the analysis worker, and the worker
//! yields a correct `SignalSnapshot`.
//!
//! This is gated behind `test-sync-analysis`, which runs Lúmedir's own `DeliveryWorker`
//! **inline on `push`** so `latest_snapshot()` deterministically reflects the fed
//! audio with no background thread, sleep, or wall-clock — keeping it out of the fast/pure
//! `make ci` unit run. Run via `make test-integration`.

use lindelion_plugin_shell::{
    AudioBuffer, AudioInputBuffer, AudioPlugin, ProcessContext, ProcessSetup,
};
use lumedir::Lumedir;

const SR: u32 = 48_000;
const BLOCK: usize = 16_384;

/// The same voiced test tone used by the worker/analyzer unit tests (≈150 Hz fundamental).
fn voiced_tone(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = std::f32::consts::TAU * i as f32 / SR as f32;
            0.5 * (150.0 * t).sin() + 0.25 * (300.0 * t).sin() + 0.12 * (450.0 * t).sin()
        })
        .collect()
}

/// Drive one mono input block through the plugin's `process` and return the worker snapshot.
fn process_block(input: &[f32]) -> lindelion_speech_signals::SignalSnapshot {
    let setup = ProcessSetup {
        sample_rate: SR as f64,
        max_block_size: input.len(),
        ..Default::default()
    };
    let mut plugin = Lumedir::default();
    plugin.reset(setup);

    let mut out_left = vec![0.0_f32; input.len()];
    let mut out_right = vec![0.0_f32; input.len()];
    let context = ProcessContext::new(
        setup,
        AudioBuffer {
            left: &mut out_left,
            right: &mut out_right,
        },
        &[],
    )
    .with_input(AudioInputBuffer::mono(input));
    plugin.process(context);

    plugin.latest_snapshot()
}

#[test]
#[cfg_attr(
    not(feature = "test-sync-analysis"),
    ignore = "needs test-sync-analysis for a deterministic worker snapshot"
)]
fn worker_yields_voiced_snapshot_from_fed_audio() {
    let snapshot = process_block(&voiced_tone(BLOCK));
    assert_eq!(
        snapshot.voicing_state, 2.0,
        "expected voiced from a fed tone, got {snapshot:?}"
    );
    assert!(
        (120.0..180.0).contains(&snapshot.pitch_hz),
        "pitch off: {}",
        snapshot.pitch_hz
    );
}

#[test]
#[cfg_attr(
    not(feature = "test-sync-analysis"),
    ignore = "needs test-sync-analysis for a deterministic worker snapshot"
)]
fn worker_reads_silence_from_fed_silence() {
    let snapshot = process_block(&vec![0.0_f32; BLOCK]);
    assert_eq!(
        snapshot.voicing_state, 0.0,
        "expected silence, got {snapshot:?}"
    );
}

#[test]
#[cfg_attr(
    not(feature = "test-sync-analysis"),
    ignore = "needs test-sync-analysis for a deterministic delivery snapshot"
)]
fn delivery_worker_yields_a_complete_delivery_snapshot() {
    use std::sync::Arc;

    use lumedir::{DeliveryWorker, SharedConfig};

    let worker = DeliveryWorker::new(SR as f32, Arc::new(SharedConfig::default()));
    // Push in small chunks (≈ one frame each), as the off-thread loop drains.
    for chunk in voiced_tone(BLOCK).chunks(256) {
        worker.push(chunk);
    }

    let signal = worker.latest_snapshot();
    assert_eq!(
        signal.voicing_state, 2.0,
        "expected voiced underlying snapshot, got {signal:?}"
    );

    let delivery = worker.latest_delivery();
    assert!(
        (0.0..=1.0).contains(&delivery.clarity),
        "clarity out of range: {delivery:?}"
    );
    assert!((0.0..=1.0).contains(&delivery.pause_fraction));
    assert!(delivery.syllables_per_second.is_finite() && delivery.syllables_per_second >= 0.0);
    assert!(
        delivery.pitch_dynamism_semitones.is_finite() && delivery.pitch_dynamism_semitones >= 0.0
    );
    assert!(delivery.words_per_minute.is_finite());
    // A voiced tone reads as clearly voiced ⇒ non-zero clarity.
    assert!(
        delivery.clarity > 0.0,
        "expected non-zero clarity, got {delivery:?}"
    );
}

/// Soak (off-thread stability): drive the real background worker over a long stream; every published
/// snapshot stays finite + in-range (no NaN/blow-up), the worker keeps draining (no deadlock), and
/// `Drop` joins its thread cleanly. Spawns a thread + sleeps ⇒ gated to the integration suite. Built
/// only for the off-thread build (`test-sync-analysis` runs `push` inline, which is neither what
/// this exercises nor cheap to drag in via `--include-ignored`).
#[cfg(not(feature = "test-sync-analysis"))]
#[test]
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "off-thread soak spawns the worker thread and sleeps; run via make test-integration"
)]
fn worker_soak_is_stable_off_thread() {
    use std::sync::Arc;
    use std::time::Duration;

    use lumedir::{DeliveryWorker, SharedConfig};

    let worker = DeliveryWorker::new(SR as f32, Arc::new(SharedConfig::default()));
    let block = voiced_tone(512);

    for _ in 0..400 {
        worker.push(&block);
        std::thread::sleep(Duration::from_millis(1));
        let d = worker.latest_delivery();
        assert!(
            d.syllables_per_second.is_finite()
                && d.words_per_minute.is_finite()
                && d.pitch_dynamism_semitones.is_finite(),
            "non-finite delivery over the soak: {d:?}"
        );
        assert!((0.0..=1.0).contains(&d.clarity) && (0.0..=1.0).contains(&d.pause_fraction));
    }

    // Dropping the worker joins its background thread cleanly (no hang/panic).
    drop(worker);
}
