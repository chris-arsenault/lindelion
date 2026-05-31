//! Light, model-free contract tests that run in `make ci`. These exercise the effect's behaviour
//! that does not require the ONNX Runtime model (parameter handling, state I/O, and the
//! non-native-rate passthrough that never builds a session). The heavy tests that load the model
//! and run inference live in `tests/integration.rs` and are run separately via `make test-models`.

use lindelion_effect::Effect;
use lindelion_speech_denoiser::{PARAM_ATTEN_LIMIT_DB, PARAM_MIX, SpeechDenoiser};

#[test]
fn non_native_rate_is_passthrough_with_no_latency() {
    // At a non-48 kHz rate the effect never builds a session: pure passthrough, zero latency.
    let mut effect = SpeechDenoiser::new();
    effect.prepare(44_100.0, 1024);
    assert_eq!(effect.latency_samples(), 0);
    let input: Vec<f32> = (0..2048).map(|n| (n as f32 * 0.01).sin() * 0.3).collect();
    let mut output = input.clone();
    effect.process(&mut output);
    assert_eq!(output, input, "non-48k must pass through untouched");
}

#[test]
fn latency_is_zero_before_prepare() {
    let effect = SpeechDenoiser::new();
    assert_eq!(effect.latency_samples(), 0);
}

#[test]
fn parameters_are_clamped() {
    let mut effect = SpeechDenoiser::new();
    effect.set_parameter(PARAM_MIX, 150.0); // over 100%
    effect.set_parameter(PARAM_ATTEN_LIMIT_DB, 999.0); // over the dB max
    let state = effect.save_state();
    let mix = f32::from_le_bytes([state[0], state[1], state[2], state[3]]);
    let atten = f32::from_le_bytes([state[4], state[5], state[6], state[7]]);
    assert_eq!(mix, 100.0, "mix clamps to 100%");
    assert_eq!(atten, 100.0, "atten clamps to 100 dB");
}

#[test]
fn state_round_trips() {
    let mut a = SpeechDenoiser::new();
    a.set_parameter(PARAM_MIX, 40.0);
    a.set_parameter(PARAM_ATTEN_LIMIT_DB, 24.0);
    a.set_bypassed(true);
    let saved = a.save_state();

    let mut b = SpeechDenoiser::new();
    b.load_state(&saved);
    assert_eq!(
        b.save_state(),
        saved,
        "state must survive a save/load round-trip"
    );
    assert!(b.is_bypassed());
}

#[test]
fn exposes_named_parameters() {
    let effect = SpeechDenoiser::new();
    assert_eq!(effect.name(), "Speech Denoiser");
    let names: Vec<&str> = effect.parameters().iter().map(|p| p.name).collect();
    assert_eq!(names, ["Mix", "Atten Limit"]);
}
