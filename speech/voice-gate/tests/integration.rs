//! Voice Gate model-integration suite (heavy: loads Silero and runs ONNX Runtime inference).
//! Every test is `#[ignore]`d so it does **not** run in `make ci` (running inference in parallel
//! with the workspace's unit tests saturates the CPU). Run via `make test-models` (or
//! `cargo test -p lindelion-speech-voice-gate --test integration -- --include-ignored`). Cheap,
//! model-free contract tests live in `tests/contract.rs` and in the crate's unit tests.

use std::path::PathBuf;

use lindelion_effect::Effect;
use lindelion_fidelity::{BatteryOptions, assert_bounded_allocation, run_general_battery_with};
use lindelion_sample_library::decode_wav_mono;
use lindelion_speech_voice_gate::{MODEL_SAMPLE_RATE, VoiceGate};

lindelion_test_allocator::install_test_allocator!();

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

fn white_noise(n: usize, seed: u64, amp: f32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let u = (s >> 33) as f32 / (1u64 << 31) as f32;
            (u - 1.0) * amp
        })
        .collect()
}

fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f64 / ratio;
            let i0 = src.floor() as usize;
            let frac = (src - i0 as f64) as f32;
            let a = input.get(i0).copied().unwrap_or(0.0);
            let b = input.get(i0 + 1).copied().unwrap_or(a);
            a + (b - a) * frac
        })
        .collect()
}

/// 48 kHz mono speech, resampled from the 44.1 kHz stereo-downmixed fixture.
fn speech_48k() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/audio/vocal_spoken.wav")
        .canonicalize()
        .expect("fixture present");
    let decoded = decode_wav_mono(&path).expect("decode vocal_spoken.wav");
    resample_linear(&decoded.samples, decoded.sample_rate, 48_000)
}

fn prepared() -> VoiceGate {
    let mut effect = VoiceGate::new();
    effect.prepare(MODEL_SAMPLE_RATE, 1024);
    effect
}

fn process(effect: &mut VoiceGate, signal: &[f32]) -> Vec<f32> {
    let mut out = signal.to_vec();
    effect.process(&mut out);
    out
}

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn opens_on_speech_closes_on_noise() {
    // Real spoken-word (the target-mic fixture) followed by broadband noise: the gate passes the
    // speech (gain ~1) and closes on the noise (gain -> floor), since Silero scores noise as
    // non-speech.
    let speech = speech_48k();
    let noise = white_noise(96_000, 7, rms(&speech).max(0.05));
    let mut signal = speech.clone();
    signal.extend_from_slice(&noise);

    let out = process(&mut prepared(), &signal);

    // Speech region (skip attack/warm-up): gate open, energy retained.
    let warm = 16_000;
    let speech_ratio = rms(&out[warm..speech.len()]) / rms(&signal[warm..speech.len()]);
    // Noise region after the gate has had time to close (hold + release settle).
    let noise_start = speech.len() + 20_000;
    let noise_ratio = rms(&out[noise_start..]) / rms(&signal[noise_start..]);

    eprintln!("speech_ratio {speech_ratio:.3}, noise_ratio {noise_ratio:.3}");
    assert!(
        speech_ratio > 0.6,
        "gate should pass speech (ratio {speech_ratio:.3})"
    );
    assert!(
        noise_ratio < 0.3,
        "gate should attenuate non-speech noise (ratio {noise_ratio:.3})"
    );
    assert!(
        speech_ratio > noise_ratio + 0.4,
        "speech must be passed more than noise"
    );
}

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn passes_general_battery() {
    // Latency is scoped out (ADR-0014); the gate reports zero latency and is validated above.
    let mut effect = VoiceGate::new();
    run_general_battery_with(
        &mut effect,
        MODEL_SAMPLE_RATE,
        BatteryOptions {
            check_latency: false,
        },
    )
    .expect("voice gate passes general battery");
}

#[test]
#[ignore = "heavy ONNX Runtime inference; run via `make test-models`"]
fn inference_allocation_is_bounded() {
    // ADR-0014: inference may allocate a bounded amount inline. The block spans several VAD chunks
    // so inference actually runs in the measured call.
    let mut effect = VoiceGate::new();
    assert_bounded_allocation(&mut effect, 4096, 4_000);
}
