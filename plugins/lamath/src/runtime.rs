use lindelion_plugin_shell::ExpressionSource;
use lindelion_plugin_shell::{
    ControlEvent, MIDI_CHANNEL_COUNT, MidiEvent, MidiExpressionControl, MidiExpressionControlRoute,
    MidiExpressionMapping, MidiExpressionSource, MidiExpressionUpdate, MidiVoiceExpression,
    NoteEvent, ParameterId,
};

use crate::{
    ExcitationSlot, RESONATOR_BRIGHTNESS_CONTROLLER, RESONATOR_MOD_WHEEL_CONTROLLER,
    ResonatorSynthPatch,
    dsp::{
        ExcitationSelector, MAX_EXCITATION_LAYERS, RuntimeExcitationSlot, SelectedExcitations,
        SynthEngine, VoiceExpression, VoiceTrigger,
    },
};

const MIDI_EXPRESSION_VOICES: usize = MIDI_CHANNEL_COUNT;
const GLOBAL_EXPRESSION_CHANNEL: u8 = 0;
const RESONATOR_EXPRESSION_CONTROL_ROUTES: &[MidiExpressionControlRoute] = &[
    MidiExpressionControlRoute::new(
        RESONATOR_MOD_WHEEL_CONTROLLER,
        MidiExpressionControl::ModWheel,
    ),
    MidiExpressionControlRoute::new(
        RESONATOR_BRIGHTNESS_CONTROLLER,
        MidiExpressionControl::Brightness,
    ),
];

const fn resonator_expression_mapping() -> MidiExpressionMapping<'static> {
    MidiExpressionMapping::new(RESONATOR_EXPRESSION_CONTROL_ROUTES)
}

pub const BUILTIN_EXCITATION_SAMPLE_RATE: f32 = 48_000.0;
pub static BUILTIN_EXCITATION: [f32; 64] = [
    1.0,
    -0.74,
    0.52,
    -0.37,
    0.29,
    -0.21,
    0.16,
    -0.12,
    0.091,
    -0.068,
    0.051,
    -0.038,
    0.029,
    -0.022,
    0.017,
    -0.013,
    0.010,
    -0.0077,
    0.0059,
    -0.0045,
    0.0034,
    -0.0026,
    0.0020,
    -0.0015,
    0.0012,
    -0.0009,
    0.00068,
    -0.00052,
    0.00040,
    -0.00030,
    0.00023,
    -0.00018,
    0.00013,
    -0.00010,
    0.000078,
    -0.000059,
    0.000045,
    -0.000034,
    0.000026,
    -0.000020,
    0.000015,
    -0.000012,
    0.0000088,
    -0.0000067,
    0.0000051,
    -0.0000039,
    0.0000030,
    -0.0000023,
    0.0000017,
    -0.0000013,
    0.0000010,
    -0.00000077,
    0.00000059,
    -0.00000045,
    0.00000034,
    -0.00000026,
    0.00000020,
    -0.00000015,
    0.00000012,
    -0.000000088,
    0.000000067,
    -0.000000051,
    0.000000039,
    0.0,
];

#[derive(Debug, Clone)]
pub(crate) struct RuntimePatch<'a> {
    patch: ResonatorSynthPatch,
    slots: [Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
}

impl RuntimePatch<'static> {
    pub(crate) fn with_builtin_excitation(patch: ResonatorSynthPatch) -> Self {
        let slot_config = patch.excitation_slots.first().cloned().unwrap_or_default();
        Self {
            patch,
            slots: [
                Some(runtime_slot_from_config(
                    &slot_config,
                    &BUILTIN_EXCITATION,
                    BUILTIN_EXCITATION_SAMPLE_RATE,
                )),
                None,
                None,
                None,
            ],
        }
    }
}

impl<'a> RuntimePatch<'a> {
    pub(crate) fn new(
        patch: ResonatorSynthPatch,
        slots: [Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
    ) -> Self {
        Self { patch, slots }
    }
}

#[derive(Debug)]
pub(crate) struct ResonatorProcessor<'a> {
    runtime_patch: RuntimePatch<'a>,
    engine: SynthEngine<'a>,
    selector: ExcitationSelector,
    expression_source: MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
}

impl ResonatorProcessor<'static> {
    pub(crate) fn with_builtin_excitation(sample_rate: f32, patch: ResonatorSynthPatch) -> Self {
        Self::new(sample_rate, RuntimePatch::with_builtin_excitation(patch))
    }
}

impl<'a> ResonatorProcessor<'a> {
    pub(crate) fn new(sample_rate: f32, runtime_patch: RuntimePatch<'a>) -> Self {
        let polyphony = runtime_patch.patch.polyphony.clamp(1, 16) as usize;
        Self {
            runtime_patch,
            engine: SynthEngine::new(sample_rate, polyphony),
            selector: ExcitationSelector::default(),
            expression_source: MidiExpressionSource::default(),
        }
    }

    pub(crate) fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);

        for event in events {
            self.handle_event(*event);
        }

        self.engine
            .sync_expression_source(&mut self.expression_source);
        self.engine.render_add(left, right);
    }

    // Kept as the source-agnostic expression path; the current plugin process uses MIDI expression.
    #[allow(dead_code)]
    pub(crate) fn process_with_expression_source(
        &mut self,
        source: &mut impl ExpressionSource,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        left.fill(0.0);
        right.fill(0.0);

        for event in events {
            self.handle_event_with_expression_source(*event, source);
        }

        self.engine.sync_expression_source(source);
        self.engine.render_add(left, right);
    }

    pub(crate) fn active_voice_count(&self) -> usize {
        self.engine.active_voice_count()
    }

    pub(crate) fn replace_patch_config(&mut self, patch: ResonatorSynthPatch) {
        self.runtime_patch.patch = patch;
    }

    pub(crate) fn set_parameter_plain(&mut self, parameter: ParameterId, value: f32) {
        let Some(binding) = crate::parameter_binding(parameter.0) else {
            return;
        };
        let target = binding.runtime_target();
        if !target.is_active() {
            return;
        }

        binding.apply_plain(&mut self.runtime_patch.patch, value);
        match target {
            crate::RuntimeParameterTarget::None => {}
            crate::RuntimeParameterTarget::Output => self
                .engine
                .set_output_config(self.runtime_patch.patch.output),
            crate::RuntimeParameterTarget::Routing => {
                self.engine.set_routing(self.runtime_patch.patch.routing);
            }
        }
    }

    pub(crate) fn set_pitch_bend_normalized(&mut self, normalized: f32) {
        let range = self.pitch_bend_range();
        self.set_pitch_bend_semitones((normalized.clamp(0.0, 1.0) * 2.0 - 1.0) * range);
    }

    fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => {
                self.note_on(channel, note, velocity);
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

    fn handle_event_with_expression_source(
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
        let selected = self.selector.select(&self.runtime_patch.slots, velocity);
        let excitations = if selected.is_empty() {
            SelectedExcitations::from_single(&BUILTIN_EXCITATION, BUILTIN_EXCITATION_SAMPLE_RATE)
        } else {
            selected
        };
        let modulation = self.runtime_patch.patch.modulation;
        let mut trigger =
            VoiceTrigger::with_excitations(note, velocity, excitations, &self.runtime_patch.patch);
        trigger.channel = channel;
        trigger.expression = self
            .expression_source
            .note_expression(channel, velocity)
            .into();
        trigger.modulation = modulation;
        let slot = self.engine.note_on(trigger);
        self.expression_source
            .begin_voice(slot as u32, channel, velocity);
        slot
    }

    fn note_off(&mut self, channel: u8, note: u8) {
        let channel = sanitize_channel(channel);
        for index in 0..self.engine.polyphony() {
            if self.engine.slot_channel(index) == Some(channel)
                && self.engine.slot_note(index) == Some(note)
            {
                self.expression_source.set_voice_gate(index as u32, false);
            }
        }
        self.engine.note_off_for_channel(channel, note);
    }

    fn note_off_with_expression_source(
        &mut self,
        channel: u8,
        note: u8,
        source: &mut impl ExpressionSource,
    ) {
        let channel = sanitize_channel(channel);
        for index in 0..self.engine.polyphony() {
            if self.engine.slot_channel(index) == Some(channel)
                && self.engine.slot_note(index) == Some(note)
            {
                self.expression_source.set_voice_gate(index as u32, false);
                source.voice_released(index as u32);
            }
        }
        self.engine.note_off_for_channel(channel, note);
    }

    fn set_pitch_bend_semitones(&mut self, semitones: f32) {
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

    fn pitch_bend_range(&self) -> f32 {
        self.runtime_patch
            .patch
            .modulation
            .pitch_bend_range_semitones
            .abs()
            .max(0.0)
    }
}

impl From<MidiVoiceExpression> for VoiceExpression {
    fn from(expression: MidiVoiceExpression) -> Self {
        Self {
            stream: expression.stream,
            mod_wheel: expression.mod_wheel,
        }
        .sanitized()
    }
}

fn sanitize_channel(channel: u8) -> u8 {
    channel.min((MIDI_CHANNEL_COUNT - 1) as u8)
}

pub(crate) fn runtime_slot_from_config<'a>(
    config: &ExcitationSlot,
    samples: &'a [f32],
    sample_rate: f32,
) -> RuntimeExcitationSlot<'a> {
    RuntimeExcitationSlot {
        samples,
        sample_rate,
        gain_db: config.gain_db,
        velocity_low: config.velocity_low,
        velocity_high: config.velocity_high,
        start_offset_samples: config.start_offset_ms.max(0.0) * sample_rate * 0.001,
        velocity_start_offset_samples: config.velocity_start_mod_ms * sample_rate * 0.001,
        looped: config.looping,
        pitch_track: config.pitch_track,
        round_robin_group: config.round_robin_group,
    }
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
