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

#[derive(Debug, Clone, Copy)]
pub struct VoiceSlotView<'a, V: VoiceLike> {
    pub state: VoiceSlotState,
    pub channel: Option<u8>,
    pub note: Option<u8>,
    pub last_level: f32,
    pub voice: &'a V,
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

    pub fn clear_voice(&mut self, voice_id: usize) -> bool {
        if voice_id >= self.voice_limit {
            return false;
        }
        let slot = &mut self.slots[voice_id];
        let was_live = slot.state != VoiceSlotState::Idle;
        slot.clear();
        was_live
    }

    pub fn clear_note_for_channel(&mut self, channel: u8, note: u8) {
        let channel = sanitize_channel(channel);
        self.clear_voices_where(|slot| slot.channel == Some(channel) && slot.note == Some(note));
    }

    pub fn clear_voices_where(
        &mut self,
        mut matches: impl FnMut(VoiceSlotView<'_, V>) -> bool,
    ) -> usize {
        let mut cleared = 0;
        for slot in self.live_slots_mut() {
            if slot.state == VoiceSlotState::Idle {
                continue;
            }
            let view = VoiceSlotView {
                state: slot.state,
                channel: slot.channel,
                note: slot.note,
                last_level: slot.last_level,
                voice: &slot.voice,
            };
            if matches(view) {
                slot.clear();
                cleared += 1;
            }
        }
        cleared
    }

    pub fn clear_all(&mut self) {
        for slot in self.live_slots_mut() {
            slot.clear();
        }
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
#[path = "voices_tests.rs"]
mod tests;
