use crate::{ExpressionSource, ExpressionStream, MidiVoiceExpression};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceSlotState {
    Idle,
    Active,
    Released,
}

pub trait ManagedVoiceExpression: Copy + Default {
    fn sanitized(self) -> Self;
    fn stream(self) -> ExpressionStream;
    fn set_stream(&mut self, stream: ExpressionStream);
    fn set_mod_wheel(&mut self, mod_wheel: f32);

    fn with_gate(mut self, gate: bool) -> Self {
        let mut stream = self.stream();
        stream.gate = gate;
        self.set_stream(stream);
        self.sanitized()
    }
}

pub trait VoiceLike {
    type Expression: ManagedVoiceExpression;

    fn set_expression(&mut self, expression: Self::Expression);
    fn clear(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceRenderStatus {
    pub last_level: f32,
    pub idle: bool,
}

pub struct VoiceManager<const N: usize, V: VoiceLike> {
    slots: [VoiceSlot<V>; N],
    voice_limit: usize,
    clock: u64,
}

impl<const N: usize, V> fmt::Debug for VoiceManager<N, V>
where
    V: VoiceLike + fmt::Debug,
    V::Expression: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceManager")
            .field("slots", &self.live_slots())
            .field("voice_limit", &self.voice_limit)
            .field("clock", &self.clock)
            .finish()
    }
}

impl<const N: usize, V: VoiceLike> VoiceManager<N, V> {
    pub fn new(voice_limit: usize, mut make_voice: impl FnMut() -> V) -> Self {
        assert!(N > 0, "VoiceManager requires at least one slot");
        Self {
            slots: std::array::from_fn(|_| VoiceSlot::new(make_voice())),
            voice_limit: voice_limit.max(1).min(N),
            clock: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn voice_limit(&self) -> usize {
        self.voice_limit
    }

    pub fn active_voice_count(&self) -> usize {
        self.live_slots()
            .iter()
            .filter(|slot| slot.state != VoiceSlotState::Idle)
            .count()
    }

    pub fn slot_state(&self, index: usize) -> Option<VoiceSlotState> {
        self.live_slots().get(index).map(|slot| slot.state)
    }

    pub fn slot_note(&self, index: usize) -> Option<u8> {
        self.live_slots().get(index).and_then(|slot| slot.note)
    }

    pub fn slot_channel(&self, index: usize) -> Option<u8> {
        self.live_slots().get(index).and_then(|slot| slot.channel)
    }

    pub fn slot_last_level(&self, index: usize) -> Option<f32> {
        self.live_slots().get(index).map(|slot| slot.last_level)
    }

    pub fn slot_expression(&self, index: usize) -> Option<V::Expression> {
        self.live_slots().get(index).map(|slot| slot.expression)
    }

    pub fn start_voice(
        &mut self,
        channel: u8,
        note: u8,
        expression: V::Expression,
        retrigger_voice: bool,
        start: impl FnOnce(&mut V),
    ) -> usize {
        self.clock = self.clock.wrapping_add(1);

        let channel = sanitize_channel(channel);
        let expression = expression.sanitized();
        let slot_index = self.choose_voice_slot(channel, note, retrigger_voice);
        let slot = &mut self.slots[slot_index];

        start(&mut slot.voice);
        slot.channel = Some(channel);
        slot.note = Some(note);
        slot.per_note_pressure = None;
        slot.expression = expression;
        slot.state = VoiceSlotState::Active;
        slot.started_at = self.clock;
        slot.released_at = None;
        slot.last_level = expression.stream().velocity;
        slot.voice.set_expression(expression);

        slot_index
    }

    pub fn release_note(&mut self, note: u8) {
        self.release_matching(|slot| slot.note == Some(note));
    }

    pub fn release_note_for_channel(&mut self, channel: u8, note: u8) {
        let channel = sanitize_channel(channel);
        self.release_matching(|slot| slot.channel == Some(channel) && slot.note == Some(note));
    }

    pub fn release_voice(&mut self, voice_id: usize) -> bool {
        if voice_id >= self.voice_limit {
            return false;
        }
        self.clock = self.clock.wrapping_add(1);
        let released_at = self.clock;
        let slot = &mut self.slots[voice_id];
        let was_active = slot.state == VoiceSlotState::Active;
        release_slot(slot, released_at);
        was_active
    }

    pub fn release_all(&mut self) {
        self.clock = self.clock.wrapping_add(1);
        let released_at = self.clock;

        for slot in self.live_slots_mut() {
            release_slot(slot, released_at);
        }
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        for slot in self.live_slots_mut() {
            if slot.state == VoiceSlotState::Idle {
                continue;
            }

            let mut expression = slot.expression;
            let mut stream = expression.stream();
            stream.pitch_bend = semitones;
            expression.set_stream(stream);
            slot.expression = expression.sanitized();
            slot.voice.set_expression(slot.expression);
        }
    }

    pub fn set_expression_controls(
        &mut self,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) {
        self.set_expression_controls_matching(
            |_| true,
            pitch_bend,
            pressure,
            brightness,
            mod_wheel,
        );
    }

    pub fn set_expression_controls_for_channel(
        &mut self,
        channel: u8,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) {
        let channel = sanitize_channel(channel);
        self.set_expression_controls_matching(
            |slot| slot.channel == Some(channel),
            pitch_bend,
            pressure,
            brightness,
            mod_wheel,
        );
    }

    pub fn set_poly_pressure(&mut self, channel: u8, note: u8, value: f32) {
        let channel = sanitize_channel(channel);
        let pressure = sanitize_unit(value);

        for slot in self.live_slots_mut() {
            if slot.state == VoiceSlotState::Idle {
                continue;
            }

            if slot.channel == Some(channel) && slot.note == Some(note) {
                slot.per_note_pressure = Some(pressure);
                let mut expression = slot.expression;
                let mut stream = expression.stream();
                stream.pressure = pressure;
                expression.set_stream(stream);
                slot.expression = expression.sanitized();
                slot.voice.set_expression(slot.expression);
            }
        }
    }

    pub fn sync_expression_source(&mut self, source: &mut impl ExpressionSource) {
        for index in 0..self.voice_limit {
            let slot = &mut self.slots[index];
            if slot.state == VoiceSlotState::Idle {
                continue;
            }

            let was_active = slot.state == VoiceSlotState::Active;
            let mut stream = source.next_block(index as u32).sanitized();
            if slot.state == VoiceSlotState::Released {
                stream.gate = false;
            }
            if let Some(pressure) = slot.per_note_pressure {
                stream.pressure = pressure;
            }
            if was_active && !stream.gate {
                self.clock = self.clock.wrapping_add(1);
                slot.state = VoiceSlotState::Released;
                slot.released_at = Some(self.clock);
            }

            let mut expression = slot.expression;
            expression.set_stream(stream);
            slot.expression = expression.sanitized();
            slot.voice.set_expression(slot.expression);
        }
    }

    pub fn for_each_live_voice_mut(&mut self, mut visit: impl FnMut(&mut V)) {
        for slot in self.live_slots_mut() {
            if slot.state != VoiceSlotState::Idle {
                visit(&mut slot.voice);
            }
        }
    }

    pub fn process_live_voices(&mut self, mut process: impl FnMut(&mut V) -> VoiceRenderStatus) {
        for slot in self.live_slots_mut() {
            if slot.state == VoiceSlotState::Idle {
                continue;
            }

            slot.voice.set_expression(slot.expression);
            let status = process(&mut slot.voice);
            slot.last_level = status.last_level;

            if slot.state == VoiceSlotState::Released && status.idle {
                slot.clear();
            }
        }
    }

    fn release_matching(&mut self, matches: impl Fn(&VoiceSlot<V>) -> bool) {
        self.clock = self.clock.wrapping_add(1);
        let released_at = self.clock;

        for slot in self.live_slots_mut() {
            if matches(slot) {
                release_slot(slot, released_at);
            }
        }
    }

    fn set_expression_controls_matching(
        &mut self,
        matches: impl Fn(&VoiceSlot<V>) -> bool,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) {
        for slot in self.live_slots_mut() {
            if slot.state == VoiceSlotState::Idle || !matches(slot) {
                continue;
            }

            let mut expression = slot.expression;
            let mut stream = expression.stream();
            stream.pitch_bend = pitch_bend;
            stream.pressure = slot.per_note_pressure.unwrap_or(pressure);
            stream.brightness = brightness;
            expression.set_stream(stream);
            expression.set_mod_wheel(mod_wheel);
            slot.expression = expression.sanitized();
            slot.voice.set_expression(slot.expression);
        }
    }

    fn choose_voice_slot(&self, channel: u8, note: u8, retrigger_voice: bool) -> usize {
        let slots = self.live_slots();

        if !retrigger_voice
            && let Some(index) = slots.iter().position(|slot| {
                slot.state == VoiceSlotState::Released
                    && slot.channel == Some(channel)
                    && slot.note == Some(note)
            })
        {
            return index;
        }

        if let Some(index) = slots
            .iter()
            .position(|slot| slot.state == VoiceSlotState::Idle)
        {
            return index;
        }

        if let Some((index, _)) = slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.state == VoiceSlotState::Released)
            .min_by(|(_, a), (_, b)| {
                a.released_at
                    .cmp(&b.released_at)
                    .then_with(|| a.last_level.total_cmp(&b.last_level))
            })
        {
            return index;
        }

        slots
            .iter()
            .enumerate()
            .min_by_key(|(_, slot)| slot.started_at)
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn live_slots(&self) -> &[VoiceSlot<V>] {
        &self.slots[..self.voice_limit]
    }

    fn live_slots_mut(&mut self) -> &mut [VoiceSlot<V>] {
        &mut self.slots[..self.voice_limit]
    }
}

#[derive(Debug)]
struct VoiceSlot<V: VoiceLike> {
    voice: V,
    state: VoiceSlotState,
    channel: Option<u8>,
    note: Option<u8>,
    per_note_pressure: Option<f32>,
    expression: V::Expression,
    started_at: u64,
    released_at: Option<u64>,
    last_level: f32,
}

impl<V: VoiceLike> VoiceSlot<V> {
    fn new(voice: V) -> Self {
        Self {
            voice,
            state: VoiceSlotState::Idle,
            channel: None,
            note: None,
            per_note_pressure: None,
            expression: V::Expression::default(),
            started_at: 0,
            released_at: None,
            last_level: 0.0,
        }
    }

    fn clear(&mut self) {
        self.voice.clear();
        self.state = VoiceSlotState::Idle;
        self.channel = None;
        self.note = None;
        self.per_note_pressure = None;
        self.expression = V::Expression::default();
        self.released_at = None;
        self.last_level = 0.0;
    }
}

impl ManagedVoiceExpression for MidiVoiceExpression {
    fn sanitized(self) -> Self {
        MidiVoiceExpression::sanitized(self)
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

fn release_slot<V: VoiceLike>(slot: &mut VoiceSlot<V>, released_at: u64) {
    if slot.state == VoiceSlotState::Active {
        slot.state = VoiceSlotState::Released;
        slot.released_at = Some(released_at);
        slot.expression = slot.expression.with_gate(false);
        slot.voice.set_expression(slot.expression);
    }
}

fn sanitize_channel(channel: u8) -> u8 {
    channel.min(15)
}

fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
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
        let released_retrigger =
            manager.start_voice(0, 60, TestExpression::active(0.0), false, |_| {});

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
}
