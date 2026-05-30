//! Light, model-free contract tests that run in `make ci`. The heavy tests that load Silero and
//! run inference live in `tests/integration.rs` (run via `make test-models`).

use lindelion_effect::Effect;
use lindelion_speech_voice_gate::{PARAM_REDUCTION_DB, PARAM_THRESHOLD, VoiceGate};

#[test]
fn non_native_rate_is_passthrough_with_no_latency() {
    // At a non-48 kHz rate the gate never builds a session: pure passthrough, zero latency.
    let mut effect = VoiceGate::new();
    effect.prepare(44_100.0, 1024);
    assert_eq!(effect.latency_samples(), 0);
    let input: Vec<f32> = (0..2048).map(|n| (n as f32 * 0.01).sin() * 0.3).collect();
    let mut output = input.clone();
    effect.process(&mut output);
    assert_eq!(output, input, "non-48k must pass through untouched");
}

#[test]
fn latency_is_always_zero() {
    let mut effect = VoiceGate::new();
    assert_eq!(effect.latency_samples(), 0);
    effect.prepare(48_000.0, 1024);
    assert_eq!(effect.latency_samples(), 0);
}

#[test]
fn parameters_are_clamped() {
    let mut effect = VoiceGate::new();
    effect.set_parameter(PARAM_THRESHOLD, 5.0); // over 1.0
    effect.set_parameter(PARAM_REDUCTION_DB, 999.0); // over 80 dB
    let s = effect.save_state();
    let threshold = f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
    let reduction = f32::from_le_bytes([s[16], s[17], s[18], s[19]]);
    assert_eq!(threshold, 1.0, "threshold clamps to 1.0");
    assert_eq!(reduction, 80.0, "reduction clamps to 80 dB");
}

#[test]
fn state_round_trips() {
    let mut a = VoiceGate::new();
    a.set_parameter(PARAM_THRESHOLD, 0.7);
    a.set_parameter(PARAM_REDUCTION_DB, 18.0);
    a.set_bypassed(true);
    let saved = a.save_state();

    let mut b = VoiceGate::new();
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
    let effect = VoiceGate::new();
    assert_eq!(effect.name(), "Voice Gate");
    let names: Vec<&str> = effect.parameters().iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        ["Threshold", "Attack", "Hold", "Release", "Reduction"]
    );
}
