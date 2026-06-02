use super::*;
use lindelion_dsp_utils::math::{midi_note_to_hz, semitones_to_ratio};

/// Returned by the note-on path when a shared-body strike was enqueued instead of a
/// voice — no voice slot was allocated, so this is deliberately not a valid slot index.
/// The live note-on callers discard the return value.
const SHARED_BODY_STRIKE_SLOT: usize = usize::MAX;

impl<'a> ResonatorProcessor<'a> {
    pub(super) fn handle_event_with_live_latch(
        &mut self,
        event: MidiEvent,
        sidechain: &[f32],
        live_policy: LiveExcitationPolicy,
    ) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => {
                self.note_on_with_latch_capture(channel, note, velocity, sidechain, 0, live_policy);
            }
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity: _,
            })
            | MidiEvent::Note(NoteEvent::Off {
                channel,
                note,
                velocity: _,
            }) => self.note_off(channel, note),
            MidiEvent::Control(control) => self.handle_control(control),
        }
    }

    pub(super) fn handle_event_with_expression_source(
        &mut self,
        event: MidiEvent,
        source: &mut impl ExpressionSource,
    ) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => {
                let slot = self.note_on(channel, note, velocity);
                source.voice_started(slot as u32, channel, note, velocity);
            }
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity: _,
            })
            | MidiEvent::Note(NoteEvent::Off {
                channel,
                note,
                velocity: _,
            }) => self.note_off_with_expression_source(channel, note, source),
            MidiEvent::Control(control) => self.handle_control(control),
        }
    }

    fn handle_control(&mut self, control: ControlEvent) {
        match control {
            ControlEvent::PolyPressure {
                channel,
                note,
                value,
            } => self.engine.set_poly_pressure(channel, note, value),
            _ => {
                if let Some(update) = self.expression_source.apply_control_with_mapping(
                    control,
                    self.pitch_bend_range(),
                    resonator_expression_mapping(),
                ) {
                    self.sync_expression_update_to_engine(update);
                }
            }
        }
    }

    fn note_on(&mut self, channel: u8, note: u8, velocity: f32) -> usize {
        let slot = self.start_voice(channel, note, velocity);
        if self.audio_note_state.clear_if_slot(slot).is_some() {
            self.audio_expression_source.voice_released(slot as u32);
        }
        slot
    }

    #[allow(clippy::too_many_arguments)]
    fn note_on_with_latch_capture(
        &mut self,
        channel: u8,
        note: u8,
        velocity: f32,
        sidechain: &[f32],
        onset_offset: usize,
        live_policy: LiveExcitationPolicy,
    ) -> usize {
        // Shared-body idiophone mode (ADR-0031, M2): with the toggle on and the patch
        // carrying an idiophone slot, a note-on strikes the runtime-owned body instead
        // of allocating a voice — bypassing voice allocation and polyphony entirely. The
        // body mirrors only the idiophone slot(s); a Waveguide slot is silenced in the
        // body (whole-note strike — the toggle no-ops only when no idiophone slot is
        // present). Note-off does nothing to the ring; explicit damp is M4.
        if self.runtime_patch.patch.shared_body.enabled && self.patch_has_idiophone_slot() {
            self.strike_shared_body(note, velocity);
            return SHARED_BODY_STRIKE_SLOT;
        }
        let live_latch = live_policy.latch_capture(&self.live_latch_state, sidechain, onset_offset);
        let slot = Self::start_voice_in_runtime(
            &self.runtime_patch,
            &mut self.selector,
            &mut self.engine,
            &mut self.expression_source,
            channel,
            note,
            velocity,
            live_latch,
        );
        if self.audio_note_state.clear_if_slot(slot).is_some() {
            self.audio_expression_source.voice_released(slot as u32);
        }
        slot
    }

    /// The shared body mirrors the patch's idiophone resonator stack (Modal/Mesh); the
    /// toggle no-ops when neither slot is idiophone (ADR-0031, decision 2).
    fn patch_has_idiophone_slot(&self) -> bool {
        self.runtime_patch.patch.resonator_a.is_idiophone()
            || self.runtime_patch.patch.resonator_b.is_idiophone()
    }

    /// Enqueue a strike onto the shared body from a note-on (ADR-0031, M2). Reuses the
    /// voice's excitation selection (with the builtin fallback) and the `Voice::trigger`
    /// force/pitch formulas; pitch bend is left out of the struck path in M2.
    fn strike_shared_body(&mut self, note: u8, velocity: f32) {
        let selected = self.selector.select(&self.runtime_patch.slots, velocity);
        let selected = if selected.is_empty() {
            SelectedExcitations::from_single(&BUILTIN_EXCITATION, BUILTIN_EXCITATION_SAMPLE_RATE)
        } else {
            selected
        };
        let depth = self
            .runtime_patch
            .patch
            .modulation
            .velocity_to_excitation_depth;
        self.shared_body.strike(BodyStrike {
            selected,
            force_gain: velocity_to_gain(velocity, depth),
            base_frequency: midi_note_to_hz(note as f32),
            pitch_ratio: semitones_to_ratio(note as f32 - 60.0),
        });
    }

    fn start_voice(&mut self, channel: u8, note: u8, velocity: f32) -> usize {
        self.start_voice_with_latch(channel, note, velocity, None)
    }

    fn start_voice_with_latch(
        &mut self,
        channel: u8,
        note: u8,
        velocity: f32,
        live_latch: Option<LiveExcitationLatchCapture<'_>>,
    ) -> usize {
        Self::start_voice_in_runtime(
            &self.runtime_patch,
            &mut self.selector,
            &mut self.engine,
            &mut self.expression_source,
            channel,
            note,
            velocity,
            live_latch,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_voice_in_runtime(
        runtime_patch: &RuntimePatch<'a>,
        selector: &mut ExcitationSelector,
        engine: &mut SynthEngine<'a>,
        expression_source: &mut MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
        channel: u8,
        note: u8,
        velocity: f32,
        live_latch: Option<LiveExcitationLatchCapture<'_>>,
    ) -> usize {
        let selected = selector.select(&runtime_patch.slots, velocity);
        let excitations = if selected.is_empty() {
            SelectedExcitations::from_single(&BUILTIN_EXCITATION, BUILTIN_EXCITATION_SAMPLE_RATE)
        } else {
            selected
        };
        let modulation = runtime_patch.patch.modulation;
        let mut trigger =
            VoiceTrigger::with_excitations(note, velocity, excitations, &runtime_patch.patch);
        trigger.channel = channel;
        trigger.expression = expression_source.note_expression(channel, velocity).into();
        trigger.modulation = modulation;
        trigger.live_latch = live_latch;
        let slot = engine.note_on(trigger);
        expression_source.begin_voice(slot as u32, channel, velocity);
        slot
    }

    pub(super) fn note_off(&mut self, channel: u8, note: u8) {
        let channel = sanitize_channel(channel);
        let mut slots = [0_usize; MIDI_EXPRESSION_VOICES];
        let count = self.collect_midi_owned_note_slots(channel, note, &mut slots);
        for slot in slots[..count].iter().copied() {
            self.release_midi_owned_voice(slot);
        }
    }

    fn note_off_with_expression_source(
        &mut self,
        channel: u8,
        note: u8,
        source: &mut impl ExpressionSource,
    ) {
        let channel = sanitize_channel(channel);
        let mut slots = [0_usize; MIDI_EXPRESSION_VOICES];
        let count = self.collect_midi_owned_note_slots(channel, note, &mut slots);
        for slot in slots[..count].iter().copied() {
            self.release_midi_owned_voice(slot);
            source.voice_released(slot as u32);
        }
    }

    fn collect_midi_owned_note_slots(&self, channel: u8, note: u8, target: &mut [usize]) -> usize {
        let mut count = 0;
        for index in 0..self.engine.polyphony() {
            if self.audio_note_state.owns_slot(index) {
                continue;
            }
            if self.engine.slot_channel(index) != Some(channel)
                || self.engine.slot_note(index) != Some(note)
            {
                continue;
            }
            if count < target.len() {
                target[count] = index;
                count += 1;
            }
        }
        count
    }

    fn release_midi_owned_voice(&mut self, slot: usize) {
        self.expression_source.set_voice_gate(slot as u32, false);
        self.engine.note_off_voice(slot);
    }

    pub(super) fn set_pitch_bend_semitones(&mut self, semitones: f32) {
        self.set_channel_pitch_bend(GLOBAL_EXPRESSION_CHANNEL, semitones);
    }

    fn set_channel_pitch_bend(&mut self, channel: u8, semitones: f32) {
        let update =
            self.expression_source
                .set_pitch_bend(channel, semitones, self.pitch_bend_range());
        self.sync_expression_update_to_engine(update);
    }

    fn sync_expression_update_to_engine(&mut self, update: MidiExpressionUpdate) {
        let expression = VoiceExpression::from(update.expression);
        if update.channel == GLOBAL_EXPRESSION_CHANNEL {
            self.engine.set_expression_controls(
                expression.stream.pitch_bend,
                expression.stream.pressure,
                expression.stream.brightness,
                expression.mod_wheel,
            );
        } else {
            self.engine.set_expression_controls_for_channel(
                update.channel,
                expression.stream.pitch_bend,
                expression.stream.pressure,
                expression.stream.brightness,
                expression.mod_wheel,
            );
        }
    }

    pub(super) fn pitch_bend_range(&self) -> f32 {
        self.runtime_patch
            .patch
            .modulation
            .pitch_bend_range_semitones
            .abs()
            .max(0.0)
    }
}
