use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct TestExpression {
    stream: ExpressionStream,
    mod_wheel: f32,
}

impl TestExpression {
    fn active(pressure: f32) -> Self {
        Self {
            stream: ExpressionStream {
                pressure,
                velocity: 1.0,
                gate: true,
                ..ExpressionStream::default()
            },
            mod_wheel: 0.0,
        }
    }
}

impl ManagedVoiceExpression for TestExpression {
    fn sanitized(self) -> Self {
        Self {
            stream: self.stream.sanitized(),
            mod_wheel: sanitize_unit(self.mod_wheel),
        }
    }

    fn stream(self) -> ExpressionStream {
        self.stream
    }

    fn set_stream(&mut self, stream: ExpressionStream) {
        self.stream = stream;
    }

    fn set_mod_wheel(&mut self, mod_wheel: f32) {
        self.mod_wheel = mod_wheel;
    }
}

#[derive(Debug, Default)]
struct TestVoice {
    expression: TestExpression,
    clear_count: usize,
}

impl VoiceLike for TestVoice {
    type Expression = TestExpression;

    fn set_expression(&mut self, expression: Self::Expression) {
        self.expression = expression;
    }

    fn clear(&mut self) {
        self.clear_count += 1;
        self.expression = TestExpression::default();
    }
}

#[test]
fn steals_oldest_released_voice_before_sustaining_voices() {
    let mut manager = manager::<3>();

    manager.start_voice(0, 60, TestExpression::active(0.0), true, |_| {});
    manager.start_voice(0, 64, TestExpression::active(0.0), true, |_| {});
    manager.start_voice(0, 67, TestExpression::active(0.0), true, |_| {});
    manager.release_note(64);
    manager.release_note(60);

    let stolen = manager.start_voice(0, 72, TestExpression::active(0.0), true, |_| {});

    assert_eq!(stolen, 1);
    assert_eq!(manager.slot_note(0), Some(60));
    assert_eq!(manager.slot_note(1), Some(72));
    assert_eq!(manager.slot_note(2), Some(67));
}

#[test]
fn retrigger_off_reuses_released_note_but_not_sustaining_note() {
    let mut manager = manager::<2>();

    let active_slot = manager.start_voice(0, 60, TestExpression::active(0.0), false, |_| {});
    let sustaining_retrigger =
        manager.start_voice(0, 60, TestExpression::active(0.0), false, |_| {});

    assert_eq!(active_slot, 0);
    assert_eq!(sustaining_retrigger, 1);

    manager.release_note_for_channel(0, 60);
    let released_retrigger = manager.start_voice(0, 60, TestExpression::active(0.0), false, |_| {});

    assert_eq!(released_retrigger, 0);
}

#[test]
fn release_voice_releases_only_the_addressed_slot() {
    let mut manager = manager::<3>();
    let slot_a = manager.start_voice(0, 60, TestExpression::active(0.2), true, |_| {});
    let slot_b = manager.start_voice(0, 60, TestExpression::active(0.7), true, |_| {});

    assert!(manager.release_voice(slot_b));

    assert_eq!(slot_a, 0);
    assert_eq!(slot_b, 1);
    assert_eq!(manager.slot_state(slot_a), Some(VoiceSlotState::Active));
    assert_eq!(manager.slot_state(slot_b), Some(VoiceSlotState::Released));
    assert!(manager.slot_expression(slot_a).unwrap().stream().gate);
    assert!(!manager.slot_expression(slot_b).unwrap().stream().gate);
}

#[test]
fn clear_note_for_channel_immediately_frees_matching_voice_slots() {
    let mut manager = manager::<3>();
    let matching = manager.start_voice(1, 60, TestExpression::active(0.2), true, |_| {});
    let other_channel = manager.start_voice(2, 60, TestExpression::active(0.7), true, |_| {});

    manager.clear_note_for_channel(1, 60);

    assert_eq!(manager.slot_state(matching), Some(VoiceSlotState::Idle));
    assert_eq!(
        manager.slot_state(other_channel),
        Some(VoiceSlotState::Active)
    );
    assert_eq!(manager.active_voice_count(), 1);
}

#[test]
fn clear_voices_where_uses_shared_slot_view_predicate() {
    let mut manager = manager::<3>();
    let low_pressure = manager.start_voice(1, 60, TestExpression::active(0.2), true, |_| {});
    let high_pressure = manager.start_voice(1, 64, TestExpression::active(0.8), true, |_| {});
    let other_channel = manager.start_voice(2, 67, TestExpression::active(0.9), true, |_| {});

    let cleared = manager.clear_voices_where(|slot| {
        slot.channel == Some(1) && slot.voice.expression.stream.pressure > 0.5
    });

    assert_eq!(cleared, 1);
    assert_eq!(
        manager.slot_state(low_pressure),
        Some(VoiceSlotState::Active)
    );
    assert_eq!(
        manager.slot_state(high_pressure),
        Some(VoiceSlotState::Idle)
    );
    assert_eq!(
        manager.slot_state(other_channel),
        Some(VoiceSlotState::Active)
    );
}

#[test]
fn per_channel_pressure_reaches_only_voices_on_that_channel() {
    let mut manager = manager::<3>();
    let channel_one = manager.start_voice(1, 60, TestExpression::active(0.0), true, |_| {});
    let channel_two = manager.start_voice(2, 60, TestExpression::active(0.0), true, |_| {});

    manager.set_expression_controls_for_channel(2, 0.0, 0.7, 0.0, 0.0);

    assert_eq!(
        manager
            .slot_expression(channel_one)
            .unwrap()
            .stream
            .pressure,
        0.0
    );
    assert_eq!(
        manager
            .slot_expression(channel_two)
            .unwrap()
            .stream
            .pressure,
        0.7
    );
}

fn manager<const N: usize>() -> VoiceManager<N, TestVoice> {
    VoiceManager::new(N, TestVoice::default)
}
