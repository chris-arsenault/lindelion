use super::*;
use crate::{
    ModalConfig, ModalPreset, OutputConfig, ResonatorRouting, ResonatorSynthPatch,
    assert_no_allocations,
};
use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};
use lindelion_plugin_shell::VoiceSlotState;

fn stereo_peak(left: &[f32], right: &[f32]) -> f32 {
    peak_abs(left).max(peak_abs(right))
}

#[test]
fn note_on_uses_free_slots_before_stealing() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 3);

    assert_eq!(
        engine.note_on(trigger(60, &excitation, sample_rate, &patch)),
        0
    );
    assert_eq!(
        engine.note_on(trigger(64, &excitation, sample_rate, &patch)),
        1
    );
    assert_eq!(
        engine.note_on(trigger(67, &excitation, sample_rate, &patch)),
        2
    );

    assert_eq!(engine.active_voice_count(), 3);
    assert_eq!(engine.slot_note(0), Some(60));
    assert_eq!(engine.slot_note(1), Some(64));
    assert_eq!(engine.slot_note(2), Some(67));
}

#[test]
fn released_voice_is_stolen_before_active_voice() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 2);

    engine.note_on(trigger(60, &excitation, sample_rate, &patch));
    engine.note_on(trigger(64, &excitation, sample_rate, &patch));
    engine.note_off(60);
    let stolen = engine.note_on(trigger(67, &excitation, sample_rate, &patch));

    assert_eq!(stolen, 0);
    assert_eq!(engine.slot_note(0), Some(67));
    assert_eq!(engine.slot_note(1), Some(64));
    assert_eq!(engine.slot_state(0), Some(VoiceSlotState::Active));
}

#[test]
fn retrigger_on_uses_fresh_slot_for_repeated_released_note() {
    let sample_rate = 48_000.0;
    let mut patch = test_patch();
    patch.retrigger_resonators = true;
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 3);

    engine.note_on(trigger(60, &excitation, sample_rate, &patch));
    engine.note_off(60);
    let fresh = engine.note_on(trigger(60, &excitation, sample_rate, &patch));

    assert_eq!(fresh, 1);
    assert_eq!(engine.active_voice_count(), 2);
}

#[test]
fn oldest_active_voice_is_stolen_when_pool_is_full() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 2);

    engine.note_on(trigger(60, &excitation, sample_rate, &patch));
    engine.note_on(trigger(64, &excitation, sample_rate, &patch));
    let stolen = engine.note_on(trigger(67, &excitation, sample_rate, &patch));

    assert_eq!(stolen, 0);
    assert_eq!(engine.slot_note(0), Some(67));
    assert_eq!(engine.slot_note(1), Some(64));
}

#[test]
fn render_replace_outputs_finite_polyphonic_audio() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 4);
    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];

    engine.note_on(trigger(60, &excitation, sample_rate, &patch));
    engine.note_on(trigger(64, &excitation, sample_rate, &patch));
    engine.note_on(trigger(67, &excitation, sample_rate, &patch));
    engine.render_replace(&mut left, &mut right);

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_1);
    assert!(rms(&right) > 0.000_1);
    assert!(stereo_peak(&left, &right) < 4.0);
    assert!(engine.slot_last_level(0).unwrap() > 0.0);
}

#[test]
fn voice_slots_own_expression_stream_state() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 2);
    let mut trigger = trigger(60, &excitation, sample_rate, &patch);
    trigger.expression = VoiceExpression::with_controls(0.75, 0.5, 0.25, 0.4, 0.5);

    let slot_index = engine.note_on(trigger);
    assert_eq!(engine.slot_expression(slot_index), Some(trigger.expression));

    engine.set_expression_controls(-1.0, 0.8, 0.6, 0.4);
    let live_expression = VoiceExpression::with_controls(0.75, -1.0, 0.8, 0.6, 0.4);
    assert_eq!(engine.slot_expression(slot_index), Some(live_expression));

    engine.note_off(60);
    let mut released_expression = live_expression;
    released_expression.stream.gate = false;
    assert_eq!(
        engine.slot_state(slot_index),
        Some(VoiceSlotState::Released)
    );
    assert_eq!(
        engine.slot_expression(slot_index),
        Some(released_expression)
    );
}

#[test]
fn note_off_voice_routes_gate_only_to_addressed_voice_slot() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 2);
    let slot_a = engine.note_on(channel_trigger(1, 60, &excitation, sample_rate, &patch));
    let slot_b = engine.note_on(channel_trigger(2, 60, &excitation, sample_rate, &patch));

    assert!(engine.note_off_voice(slot_b));

    assert_slot_gate(&engine, slot_a, VoiceSlotState::Active, true);
    assert_slot_gate(&engine, slot_b, VoiceSlotState::Released, false);
}

#[test]
fn all_notes_off_routes_gate_to_every_active_voice_slot() {
    let sample_rate = 48_000.0;
    let patch = test_patch();
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 2);
    let slot_a = engine.note_on(channel_trigger(1, 48, &excitation, sample_rate, &patch));
    let slot_b = engine.note_on(channel_trigger(2, 60, &excitation, sample_rate, &patch));

    engine.all_notes_off();

    assert_slot_gate(&engine, slot_a, VoiceSlotState::Released, false);
    assert_slot_gate(&engine, slot_b, VoiceSlotState::Released, false);
}

#[test]
fn note_on_and_render_do_not_allocate() {
    let sample_rate = 48_000.0;
    let mut patch = test_patch();
    patch.resonator_a = ModalConfig {
        mode_count: 256,
        preset: ModalPreset::Bell,
        ..ModalConfig::default()
    };
    patch.resonator_b = ModalConfig {
        mode_count: 256,
        preset: ModalPreset::GlassBowl,
        ..ModalConfig::default()
    };
    let excitation = impulse();
    let mut engine = SynthEngine::new(sample_rate, 8);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    assert_no_allocations("note_on", || {
        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_on(trigger(64, &excitation, sample_rate, &patch));
        engine.note_on(trigger(67, &excitation, sample_rate, &patch));
    });

    assert_no_allocations("render_replace", || {
        engine.render_replace(&mut left, &mut right);
    });
}

fn trigger<'a>(
    note: u8,
    excitation: &'a [f32],
    sample_rate: f32,
    patch: &'a ResonatorSynthPatch,
) -> VoiceTrigger<'a, 'a> {
    VoiceTrigger::new(note, 1.0, excitation, sample_rate, patch)
}

fn channel_trigger<'a>(
    channel: u8,
    note: u8,
    excitation: &'a [f32],
    sample_rate: f32,
    patch: &'a ResonatorSynthPatch,
) -> VoiceTrigger<'a, 'a> {
    let mut trigger = trigger(note, excitation, sample_rate, patch);
    trigger.channel = channel;
    trigger
}

fn assert_slot_gate(engine: &SynthEngine<'_>, slot: usize, state: VoiceSlotState, gate: bool) {
    assert_eq!(engine.slot_state(slot), Some(state));
    assert_eq!(engine.slot_expression(slot).unwrap().stream.gate, gate);
}

fn impulse() -> Vec<f32> {
    let mut excitation = vec![0.0; 64];
    excitation[0] = 1.0;
    excitation
}

fn test_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a: ModalConfig {
            mode_count: 16,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.6,
            ..ModalConfig::default()
        },
        resonator_b: ModalConfig {
            mode_count: 32,
            preset: ModalPreset::Bell,
            decay_global: 1.0,
            ..ModalConfig::default()
        },
        routing: ResonatorRouting::Parallel {
            mix_a: 0.8,
            mix_b: 0.2,
        },
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            master_gain_db: -6.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}
