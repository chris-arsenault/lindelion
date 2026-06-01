//! End-to-end chain smoke including the NN denoiser slot. Heavy (the DFN3 model loads and runs
//! inference), so it is `#[ignore]`d and runs via `make test-models` (ADR-0018). This is a runtime
//! smoke — finite, non-clipping, latency includes the NN slot — not the fidelity gates (M6). The
//! provisional topologies stay NN-free, so the chain is built from an explicit slot list.

use std::path::PathBuf;

use caloma::order::SignalOrder;
use caloma::patch::{CalomaPatch, default_patch_for};
use caloma::runtime::ChainRuntime;
use caloma::slot::SlotId;
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_signals::SignalSnapshot;

fn fixture(name: &str) -> (Vec<f32>, f32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio")
        .join(name)
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode fixture");
    // Scale to a realistic ~-12 dBFS operating level (the fixtures are peak-normalized to -3 dBFS).
    let peak = decoded
        .samples
        .iter()
        .fold(0.0_f32, |m, x| m.max(x.abs()))
        .max(1e-6);
    let gain = 0.25 / peak;
    (
        decoded.samples.iter().map(|s| s * gain).collect(),
        decoded.sample_rate as f32,
    )
}

#[test]
#[ignore = "builds the DFN3 model; run via `make test-models`"]
fn chain_with_nn_denoiser_is_finite_non_clipping_with_latency() {
    let (dry, sr) = fixture("speech_clean_continuous_48k.wav");

    let slots = [SlotId::HighPass, SlotId::SpeechDenoiser, SlotId::Limiter];
    let mut runtime = ChainRuntime::from_slots(&slots, sr, 512);

    // Latency includes the denoiser's (which dominates the chain).
    let denoiser_latency =
        ChainRuntime::from_slots(&[SlotId::SpeechDenoiser], sr, 512).latency_samples();
    assert!(denoiser_latency > 0, "denoiser must report latency");
    assert!(
        runtime.latency_samples() >= denoiser_latency,
        "chain latency must include the denoiser: {} < {}",
        runtime.latency_samples(),
        denoiser_latency
    );

    let mut patch = CalomaPatch::default();
    patch.high_pass.enabled = true;
    patch.speech_denoiser.enabled = true;
    patch.limiter.enabled = true;

    let mut wet = dry.clone();
    let mut start = 0;
    while start < wet.len() {
        let end = (start + 512).min(wet.len());
        runtime.process(&mut wet[start..end], &patch, &SignalSnapshot::default());
        start = end;
    }

    assert!(
        wet.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
        "NN chain output must be finite and non-clipping"
    );
}

#[test]
#[ignore = "builds each real order's NN chain; run via `make test-models`"]
fn each_real_order_builds_a_valid_chain_and_processes() {
    let (dry, sr) = fixture("speech_clean_continuous_48k.wav");
    // A ~1 s segment is enough for a finiteness smoke and keeps three NN chains tractable.
    let segment = &dry[..dry.len().min(48_000)];

    for order in SignalOrder::ALL {
        let mut runtime = ChainRuntime::new(order, sr, 512);
        let latency = runtime.latency_samples();
        assert!(
            latency > 0 && latency < sr as usize,
            "{order:?} latency must be finite and reasonable (got {latency})"
        );

        let patch: CalomaPatch = default_patch_for(order);
        let mut wet = segment.to_vec();
        let mut start = 0;
        while start < wet.len() {
            let end = (start + 512).min(wet.len());
            runtime.process(&mut wet[start..end], &patch, &SignalSnapshot::default());
            start = end;
        }

        assert!(
            wet.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "{order:?} output must be finite and non-clipping"
        );
    }
}
