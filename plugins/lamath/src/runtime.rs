use lindelion_plugin_shell::{
    ControlEvent, ExpressionSource, MIDI_CHANNEL_COUNT, MidiEvent, MidiExpressionControl,
    MidiExpressionControlRoute, MidiExpressionMapping, MidiExpressionSource, MidiExpressionUpdate,
    MidiVoiceExpression, NoteEvent, ParameterId,
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
pub struct RuntimePatch<'a> {
    pub patch: ResonatorSynthPatch,
    pub slots: [Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
}

impl RuntimePatch<'static> {
    pub fn with_builtin_excitation(patch: ResonatorSynthPatch) -> Self {
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
    pub fn new(
        patch: ResonatorSynthPatch,
        slots: [Option<RuntimeExcitationSlot<'a>>; MAX_EXCITATION_LAYERS],
    ) -> Self {
        Self { patch, slots }
    }
}

#[derive(Debug)]
pub struct ResonatorProcessor<'a> {
    sample_rate: f32,
    runtime_patch: RuntimePatch<'a>,
    engine: SynthEngine<'a>,
    selector: ExcitationSelector,
    expression_source: MidiExpressionSource<MIDI_EXPRESSION_VOICES>,
}

impl ResonatorProcessor<'static> {
    pub fn with_builtin_excitation(sample_rate: f32, patch: ResonatorSynthPatch) -> Self {
        Self::new(sample_rate, RuntimePatch::with_builtin_excitation(patch))
    }
}

impl<'a> ResonatorProcessor<'a> {
    pub fn new(sample_rate: f32, runtime_patch: RuntimePatch<'a>) -> Self {
        let polyphony = runtime_patch.patch.polyphony.clamp(1, 16) as usize;
        Self {
            sample_rate,
            runtime_patch,
            engine: SynthEngine::new(sample_rate, polyphony),
            selector: ExcitationSelector::default(),
            expression_source: MidiExpressionSource::default(),
        }
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);

        for event in events {
            self.handle_event(*event);
        }

        self.engine
            .sync_expression_source(&mut self.expression_source);
        self.engine.render_add(left, right);
    }

    pub fn process_with_expression_source(
        &mut self,
        source: &mut impl ExpressionSource,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        left.fill(0.0);
        right.fill(0.0);

        for event in events {
            self.handle_event(*event);
        }

        self.engine.sync_expression_source(source);
        self.engine.render_add(left, right);
    }

    pub fn active_voice_count(&self) -> usize {
        self.engine.active_voice_count()
    }

    pub fn patch(&self) -> &ResonatorSynthPatch {
        &self.runtime_patch.patch
    }

    pub fn replace_patch_config(&mut self, patch: ResonatorSynthPatch) {
        self.runtime_patch.patch = patch;
    }

    pub fn set_parameter_plain(&mut self, parameter: ParameterId, value: f32) {
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

    pub fn set_pitch_bend_normalized(&mut self, normalized: f32) {
        let range = self.pitch_bend_range();
        self.set_pitch_bend_semitones((normalized.clamp(0.0, 1.0) * 2.0 - 1.0) * range);
    }

    fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                channel,
                note,
                velocity,
            }) if velocity > 0.0 => self.note_on(channel, note, velocity),
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

    fn note_on(&mut self, channel: u8, note: u8, velocity: f32) {
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

    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
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

pub fn runtime_slot_from_config<'a>(
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
mod tests {
    use super::*;
    use crate::{
        FilterMode, ModalConfig, ModalPreset, ModulationDestination, ModulationSlot,
        ModulationSource, OutputConfig, ResonatorConfig, ResonatorRouting, assert_no_allocations,
    };
    use lindelion_dsp_utils::{
        analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
        math::midi_note_to_hz,
    };
    use lindelion_plugin_shell::{ExpressionStream, ManualExpressionSource};

    #[test]
    fn processor_handles_note_events_and_renders_audio() {
        let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        processor.process(
            &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 60,
                velocity: 1.0,
            })],
            &mut left,
            &mut right,
        );

        assert_eq!(processor.active_voice_count(), 1);
        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(rms(&left) > 0.000_001);
    }

    #[test]
    fn processor_audio_path_does_not_allocate() {
        let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        assert_no_allocations("processor process note-on", || {
            processor.process(&events, &mut left, &mut right);
        });
        assert_no_allocations("processor process render-only", || {
            processor.process(&[], &mut left, &mut right);
        });
        let controls = [
            MidiEvent::Control(ControlEvent::PitchBend {
                channel: 0,
                semitones: 1.5,
            }),
            MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 0.75,
            }),
            MidiEvent::Control(ControlEvent::PolyPressure {
                channel: 0,
                note: 60,
                value: 0.65,
            }),
            MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 1,
                value: 0.5,
            }),
            MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 74,
                value: 0.25,
            }),
        ];
        assert_no_allocations("processor process controls", || {
            processor.process(&controls, &mut left, &mut right);
        });

        assert_live_control_path_does_not_allocate(
            "processor process live pressure resonator damping",
            aftertouch_resonator_damping_patch(),
            ControlEvent::ChannelPressure {
                channel: 0,
                value: 0.85,
            },
            &events,
            &mut left,
            &mut right,
        );
        assert_live_control_path_does_not_allocate(
            "processor process live mod wheel resonator damping",
            mod_wheel_resonator_damping_patch(),
            ControlEvent::ContinuousController {
                channel: 0,
                controller: 1,
                value: 0.85,
            },
            &events,
            &mut left,
            &mut right,
        );
        assert_live_control_path_does_not_allocate(
            "processor process live brightness resonator damping",
            brightness_resonator_damping_patch(),
            ControlEvent::ContinuousController {
                channel: 0,
                controller: 74,
                value: 0.85,
            },
            &events,
            &mut left,
            &mut right,
        );
        assert_live_control_path_does_not_allocate(
            "processor process live poly pressure resonator damping",
            poly_pressure_resonator_damping_patch(),
            ControlEvent::PolyPressure {
                channel: 0,
                note: 60,
                value: 0.85,
            },
            &events,
            &mut left,
            &mut right,
        );
    }

    fn assert_live_control_path_does_not_allocate(
        label: &str,
        patch: ResonatorSynthPatch,
        control: ControlEvent,
        note_events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
        processor.process(note_events, left, right);
        assert_no_allocations(label, || {
            processor.process(&[MidiEvent::Control(control)], left, right);
        });
    }

    #[test]
    fn held_voice_consumes_expression_stream_updates_each_block() {
        let sample_rate = 48_000.0;
        let mut neutral_processor =
            ResonatorProcessor::with_builtin_excitation(sample_rate, expression_filter_patch());
        let mut pressed_processor =
            ResonatorProcessor::with_builtin_excitation(sample_rate, expression_filter_patch());
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 256];
        let mut warmup_right = vec![0.0; 256];
        neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        pressed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 4096];
        let mut neutral_right = vec![0.0; 4096];
        let mut pressed_left = vec![0.0; 4096];
        let mut pressed_right = vec![0.0; 4096];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        pressed_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 1.0,
            })],
            &mut pressed_left,
            &mut pressed_right,
        );

        assert_eq!(neutral_processor.active_voice_count(), 1);
        assert_eq!(pressed_processor.active_voice_count(), 1);
        assert_all_finite(&neutral_left);
        assert_all_finite(&neutral_right);
        assert_all_finite(&pressed_left);
        assert_all_finite(&pressed_right);
        assert!(rms(&neutral_left) > 0.000_001);
        assert!(
            mean_abs_difference(&neutral_left, &pressed_left) > rms(&neutral_left) * 0.05,
            "neutral_rms={}, pressed_rms={}, diff={}",
            rms(&neutral_left),
            rms(&pressed_left),
            mean_abs_difference(&neutral_left, &pressed_left)
        );
    }

    #[test]
    fn non_midi_expression_source_drives_pressure_and_brightness_without_midi_events() {
        let sample_rate = 48_000.0;
        let patch = external_expression_filter_patch();
        let mut neutral_processor =
            ResonatorProcessor::with_builtin_excitation(sample_rate, patch.clone());
        let mut driven_processor = ResonatorProcessor::with_builtin_excitation(sample_rate, patch);
        let mut neutral_source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
        let mut driven_source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
        let neutral_stream = ExpressionStream {
            velocity: 1.0,
            gate: true,
            ..ExpressionStream::default()
        };
        let driven_stream = ExpressionStream {
            pressure: 0.75,
            brightness: 0.85,
            ..neutral_stream
        };
        assert!(neutral_source.set_voice_stream(0, neutral_stream));
        assert!(driven_source.set_voice_stream(0, driven_stream));

        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process_with_expression_source(
            &mut neutral_source,
            &note_on,
            &mut warmup_left,
            &mut warmup_right,
        );
        driven_processor.process_with_expression_source(
            &mut driven_source,
            &note_on,
            &mut warmup_left,
            &mut warmup_right,
        );

        let mut neutral_left = vec![0.0; 4096];
        let mut neutral_right = vec![0.0; 4096];
        let mut driven_left = vec![0.0; 4096];
        let mut driven_right = vec![0.0; 4096];
        neutral_processor.process_with_expression_source(
            &mut neutral_source,
            &[],
            &mut neutral_left,
            &mut neutral_right,
        );
        driven_processor.process_with_expression_source(
            &mut driven_source,
            &[],
            &mut driven_left,
            &mut driven_right,
        );

        let neutral = expression_for_slot(&neutral_processor, 0, 60);
        let driven = expression_for_slot(&driven_processor, 0, 60);
        assert_eq!(neutral.stream.pressure, 0.0);
        assert_eq!(neutral.stream.brightness, 0.0);
        assert_eq!(driven.stream.pressure, 0.75);
        assert_eq!(driven.stream.brightness, 0.85);
        assert_all_finite(&neutral_left);
        assert_all_finite(&driven_left);
        assert!(
            mean_abs_difference(&neutral_left, &driven_left) > rms(&neutral_left) * 0.05,
            "neutral_rms={}, driven_rms={}, diff={}",
            rms(&neutral_left),
            rms(&driven_left),
            mean_abs_difference(&neutral_left, &driven_left)
        );
    }

    #[test]
    fn channel_pressure_modulates_resonator_damping_for_held_voice() {
        let sample_rate = 48_000.0;
        let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            aftertouch_resonator_damping_patch(),
        );
        let mut pressed_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            aftertouch_resonator_damping_patch(),
        );
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 48,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        pressed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 8192];
        let mut neutral_right = vec![0.0; 8192];
        let mut pressed_left = vec![0.0; 8192];
        let mut pressed_right = vec![0.0; 8192];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        pressed_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 1.0,
            })],
            &mut pressed_left,
            &mut pressed_right,
        );

        assert_eq!(neutral_processor.active_voice_count(), 1);
        assert_eq!(pressed_processor.active_voice_count(), 1);
        assert_all_finite(&neutral_left);
        assert_all_finite(&neutral_right);
        assert_all_finite(&pressed_left);
        assert_all_finite(&pressed_right);
        assert!(rms(&neutral_left) > 0.000_001);
        assert!(
            rms(&pressed_left) > rms(&neutral_left) * 1.25,
            "neutral_rms={}, pressed_rms={}, diff={}",
            rms(&neutral_left),
            rms(&pressed_left),
            mean_abs_difference(&neutral_left, &pressed_left)
        );
    }

    #[test]
    fn poly_pressure_modulates_only_target_note_for_held_voices() {
        let sample_rate = 48_000.0;
        let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let mut pressed_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let notes = [
            MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 48,
                velocity: 1.0,
            }),
            MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 60,
                velocity: 1.0,
            }),
        ];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process(&notes, &mut warmup_left, &mut warmup_right);
        pressed_processor.process(&notes, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 8192];
        let mut neutral_right = vec![0.0; 8192];
        let mut pressed_left = vec![0.0; 8192];
        let mut pressed_right = vec![0.0; 8192];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::PolyPressure {
                channel: 0,
                note: 48,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        pressed_processor.process(
            &[MidiEvent::Control(ControlEvent::PolyPressure {
                channel: 0,
                note: 48,
                value: 1.0,
            })],
            &mut pressed_left,
            &mut pressed_right,
        );

        assert_eq!(neutral_processor.active_voice_count(), 2);
        assert_eq!(pressed_processor.active_voice_count(), 2);
        assert_all_finite(&neutral_left);
        assert_all_finite(&neutral_right);
        assert_all_finite(&pressed_left);
        assert_all_finite(&pressed_right);
        assert!(rms(&neutral_left) > 0.000_001);
        assert!(
            mean_abs_difference(&neutral_left, &pressed_left) > rms(&neutral_left) * 0.05,
            "neutral_rms={}, pressed_rms={}, diff={}",
            rms(&neutral_left),
            rms(&pressed_left),
            mean_abs_difference(&neutral_left, &pressed_left)
        );
    }

    #[test]
    fn member_channel_pressure_modulates_only_owned_voices() {
        let sample_rate = 48_000.0;
        let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let mut wrong_channel_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let mut matching_channel_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let notes = [
            MidiEvent::Note(NoteEvent::On {
                channel: 1,
                note: 48,
                velocity: 1.0,
            }),
            MidiEvent::Note(NoteEvent::On {
                channel: 2,
                note: 60,
                velocity: 1.0,
            }),
        ];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process(&notes, &mut warmup_left, &mut warmup_right);
        wrong_channel_processor.process(&notes, &mut warmup_left, &mut warmup_right);
        matching_channel_processor.process(&notes, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 8192];
        let mut neutral_right = vec![0.0; 8192];
        let mut wrong_left = vec![0.0; 8192];
        let mut wrong_right = vec![0.0; 8192];
        let mut matching_left = vec![0.0; 8192];
        let mut matching_right = vec![0.0; 8192];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 3,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        wrong_channel_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 3,
                value: 1.0,
            })],
            &mut wrong_left,
            &mut wrong_right,
        );
        matching_channel_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 1,
                value: 1.0,
            })],
            &mut matching_left,
            &mut matching_right,
        );

        assert_eq!(neutral_processor.active_voice_count(), 2);
        assert_eq!(wrong_channel_processor.active_voice_count(), 2);
        assert_eq!(matching_channel_processor.active_voice_count(), 2);
        assert_all_finite(&neutral_left);
        assert_all_finite(&wrong_left);
        assert_all_finite(&matching_left);
        assert!(rms(&neutral_left) > 0.000_001);
        assert!(
            mean_abs_difference(&neutral_left, &wrong_left) < 1.0e-7,
            "neutral/wrong diff={}",
            mean_abs_difference(&neutral_left, &wrong_left)
        );
        assert!(
            mean_abs_difference(&neutral_left, &matching_left) > rms(&neutral_left) * 0.05,
            "neutral_rms={}, matching_rms={}, diff={}",
            rms(&neutral_left),
            rms(&matching_left),
            mean_abs_difference(&neutral_left, &matching_left)
        );
    }

    #[test]
    fn channel_zero_pressure_remains_global_for_ordinary_midi() {
        let sample_rate = 48_000.0;
        let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let mut global_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            poly_pressure_resonator_damping_patch(),
        );
        let notes = [
            MidiEvent::Note(NoteEvent::On {
                channel: 1,
                note: 48,
                velocity: 1.0,
            }),
            MidiEvent::Note(NoteEvent::On {
                channel: 2,
                note: 60,
                velocity: 1.0,
            }),
        ];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process(&notes, &mut warmup_left, &mut warmup_right);
        global_processor.process(&notes, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 8192];
        let mut neutral_right = vec![0.0; 8192];
        let mut global_left = vec![0.0; 8192];
        let mut global_right = vec![0.0; 8192];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        global_processor.process(
            &[MidiEvent::Control(ControlEvent::ChannelPressure {
                channel: 0,
                value: 1.0,
            })],
            &mut global_left,
            &mut global_right,
        );

        assert_all_finite(&neutral_left);
        assert_all_finite(&global_left);
        assert!(
            rms(&global_left) > rms(&neutral_left) * 1.25,
            "neutral_rms={}, global_rms={}, diff={}",
            rms(&neutral_left),
            rms(&global_left),
            mean_abs_difference(&neutral_left, &global_left)
        );
    }

    #[test]
    fn midi_controllers_keep_independent_member_channel_state() {
        let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
        let mut left = vec![0.0; 128];
        let mut right = vec![0.0; 128];

        processor.process(
            &[
                MidiEvent::Control(ControlEvent::PitchBend {
                    channel: 2,
                    semitones: 1.5,
                }),
                MidiEvent::Control(ControlEvent::ChannelPressure {
                    channel: 2,
                    value: 0.6,
                }),
                MidiEvent::Control(ControlEvent::ContinuousController {
                    channel: 2,
                    controller: 1,
                    value: 0.7,
                }),
                MidiEvent::Control(ControlEvent::ContinuousController {
                    channel: 2,
                    controller: 74,
                    value: 0.8,
                }),
            ],
            &mut left,
            &mut right,
        );

        let untouched = processor.expression_source.channel_expression(1);
        let updated = processor.expression_source.channel_expression(2);
        assert_eq!(untouched.stream.pitch_bend, 0.0);
        assert_eq!(untouched.stream.pressure, 0.0);
        assert_eq!(untouched.mod_wheel, 0.0);
        assert_eq!(untouched.stream.brightness, 0.0);
        assert_eq!(updated.stream.pitch_bend, 1.5);
        assert_eq!(updated.stream.pressure, 0.6);
        assert_eq!(updated.mod_wheel, 0.7);
        assert_eq!(updated.stream.brightness, 0.8);
    }

    #[test]
    fn mod_wheel_modulates_resonator_damping_for_held_voice() {
        let sample_rate = 48_000.0;
        let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            mod_wheel_resonator_damping_patch(),
        );
        let mut pushed_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            mod_wheel_resonator_damping_patch(),
        );
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 48,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        pushed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 8192];
        let mut neutral_right = vec![0.0; 8192];
        let mut pushed_left = vec![0.0; 8192];
        let mut pushed_right = vec![0.0; 8192];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 1,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        pushed_processor.process(
            &[MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 1,
                value: 1.0,
            })],
            &mut pushed_left,
            &mut pushed_right,
        );

        assert_eq!(neutral_processor.active_voice_count(), 1);
        assert_eq!(pushed_processor.active_voice_count(), 1);
        assert_all_finite(&neutral_left);
        assert_all_finite(&neutral_right);
        assert_all_finite(&pushed_left);
        assert_all_finite(&pushed_right);
        assert!(rms(&neutral_left) > 0.000_001);
        assert!(
            rms(&pushed_left) > rms(&neutral_left) * 1.25,
            "neutral_rms={}, pushed_rms={}, diff={}",
            rms(&neutral_left),
            rms(&pushed_left),
            mean_abs_difference(&neutral_left, &pushed_left)
        );
    }

    #[test]
    fn brightness_modulates_resonator_damping_for_held_voice() {
        let sample_rate = 48_000.0;
        let mut neutral_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            brightness_resonator_damping_patch(),
        );
        let mut bright_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            brightness_resonator_damping_patch(),
        );
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 48,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 512];
        let mut warmup_right = vec![0.0; 512];
        neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        bright_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

        let mut neutral_left = vec![0.0; 8192];
        let mut neutral_right = vec![0.0; 8192];
        let mut bright_left = vec![0.0; 8192];
        let mut bright_right = vec![0.0; 8192];
        neutral_processor.process(
            &[MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 74,
                value: 0.0,
            })],
            &mut neutral_left,
            &mut neutral_right,
        );
        bright_processor.process(
            &[MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 74,
                value: 1.0,
            })],
            &mut bright_left,
            &mut bright_right,
        );

        assert_eq!(neutral_processor.active_voice_count(), 1);
        assert_eq!(bright_processor.active_voice_count(), 1);
        assert_all_finite(&neutral_left);
        assert_all_finite(&neutral_right);
        assert_all_finite(&bright_left);
        assert_all_finite(&bright_right);
        assert!(rms(&neutral_left) > 0.000_001);
        assert!(
            rms(&bright_left) > rms(&neutral_left) * 1.25,
            "neutral_rms={}, bright_rms={}, diff={}",
            rms(&neutral_left),
            rms(&bright_left),
            mean_abs_difference(&neutral_left, &bright_left)
        );
    }

    #[test]
    fn pitch_bend_parameter_retunes_active_voice() {
        let sample_rate = 48_000.0;
        let mut center_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_waveguide_patch(),
        );
        let mut bent_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_waveguide_patch(),
        );
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 128];
        let mut warmup_right = vec![0.0; 128];

        center_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        bent_processor.process(
            &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 60,
                velocity: 1.0,
            })],
            &mut warmup_left,
            &mut warmup_right,
        );
        bent_processor.set_pitch_bend_normalized(1.0);

        let mut center_left = vec![0.0; 8192];
        let mut center_right = vec![0.0; 8192];
        let mut bent_left = vec![0.0; 8192];
        let mut bent_right = vec![0.0; 8192];
        center_processor.process(&[], &mut center_left, &mut center_right);
        bent_processor.process(&[], &mut bent_left, &mut bent_right);

        assert_all_finite(&center_left);
        assert_all_finite(&bent_left);
        assert!(peak_abs(&center_left) > 0.000_001);
        assert!(peak_abs(&bent_left) > 0.000_001);

        let center_frequency = midi_note_to_hz(60.0);
        let bent_frequency = midi_note_to_hz(62.0);
        assert!(
            dft_magnitude_at(&center_left, sample_rate, center_frequency)
                > dft_magnitude_at(&center_left, sample_rate, bent_frequency)
        );
        assert!(
            dft_magnitude_at(&bent_left, sample_rate, bent_frequency)
                > dft_magnitude_at(&bent_left, sample_rate, center_frequency)
        );
    }

    #[test]
    fn member_channel_pitch_bend_updates_only_owned_held_voice_expression() {
        let sample_rate = 48_000.0;
        let mut processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_polyphonic_waveguide_patch(),
        );
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        processor.process(&two_member_channel_notes(), &mut left, &mut right);
        processor.process(
            &[MidiEvent::Control(ControlEvent::PitchBend {
                channel: 1,
                semitones: 2.0,
            })],
            &mut left,
            &mut right,
        );

        let bent = expression_for_slot(&processor, 1, 48);
        let untouched = expression_for_slot(&processor, 2, 60);
        assert_eq!(bent.stream.pitch_bend, 2.0);
        assert_eq!(untouched.stream.pitch_bend, 0.0);
    }

    #[test]
    fn channel_zero_pitch_bend_updates_all_held_voice_expressions() {
        let sample_rate = 48_000.0;
        let mut processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_polyphonic_waveguide_patch(),
        );
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        processor.process(&two_member_channel_notes(), &mut left, &mut right);
        processor.process(
            &[MidiEvent::Control(ControlEvent::PitchBend {
                channel: 0,
                semitones: 2.0,
            })],
            &mut left,
            &mut right,
        );

        assert_eq!(
            expression_for_slot(&processor, 1, 48).stream.pitch_bend,
            2.0
        );
        assert_eq!(
            expression_for_slot(&processor, 2, 60).stream.pitch_bend,
            2.0
        );
    }

    #[test]
    fn member_channel_pitch_bend_retunes_owned_voice_render() {
        let sample_rate = 48_000.0;
        let mut center_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_waveguide_patch(),
        );
        let mut bent_processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_waveguide_patch(),
        );
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 1,
            note: 60,
            velocity: 1.0,
        })];
        let mut warmup_left = vec![0.0; 128];
        let mut warmup_right = vec![0.0; 128];

        center_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        bent_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
        bent_processor.process(
            &[MidiEvent::Control(ControlEvent::PitchBend {
                channel: 1,
                semitones: 2.0,
            })],
            &mut warmup_left,
            &mut warmup_right,
        );

        let mut center_left = vec![0.0; 8192];
        let mut center_right = vec![0.0; 8192];
        let mut bent_left = vec![0.0; 8192];
        let mut bent_right = vec![0.0; 8192];
        center_processor.process(&[], &mut center_left, &mut center_right);
        bent_processor.process(&[], &mut bent_left, &mut bent_right);

        assert_all_finite(&center_left);
        assert_all_finite(&bent_left);
        assert_frequency_dominates(&center_left, sample_rate, 60.0, 62.0);
        assert_frequency_dominates(&bent_left, sample_rate, 62.0, 60.0);
    }

    #[test]
    fn runtime_note_off_events_route_gate_through_owned_expression_streams() {
        let sample_rate = 48_000.0;
        let mut processor = ResonatorProcessor::with_builtin_excitation(
            sample_rate,
            pitch_tracking_polyphonic_waveguide_patch(),
        );
        let mut left = vec![0.0; 128];
        let mut right = vec![0.0; 128];

        processor.process(&two_member_channel_notes(), &mut left, &mut right);
        processor.process(
            &[MidiEvent::Note(NoteEvent::Off {
                channel: 2,
                note: 60,
                velocity: 0.0,
            })],
            &mut left,
            &mut right,
        );
        assert_slot_expression_gate(&processor, 1, 48, true);
        assert_slot_expression_gate(&processor, 2, 60, false);

        processor.process(
            &[MidiEvent::Note(NoteEvent::On {
                channel: 1,
                note: 48,
                velocity: 0.0,
            })],
            &mut left,
            &mut right,
        );
        assert_slot_expression_gate(&processor, 1, 48, false);
        assert_slot_expression_gate(&processor, 2, 60, false);
    }

    #[test]
    fn state_roundtrip_preserves_patch() {
        let mut synth = crate::ResonatorSynth::default();
        let mut patch = test_patch();
        patch.name = "Roundtrip".to_string();
        synth.set_patch_for_test(patch.clone());

        let state = lindelion_plugin_shell::AudioPlugin::state(&synth);
        let mut restored = crate::ResonatorSynth::default();
        lindelion_plugin_shell::AudioPlugin::load_state(&mut restored, state);

        assert_eq!(restored.patch().name, "Roundtrip");
        assert_eq!(restored.patch().output.filter_mode, FilterMode::BandPass);
    }

    fn test_patch() -> ResonatorSynthPatch {
        ResonatorSynthPatch {
            name: "Runtime Test".to_string(),
            polyphony: 4,
            resonator_a: ResonatorConfig::Modal(ModalConfig {
                mode_count: 12,
                preset: ModalPreset::GenericStrike,
                decay_global: 0.35,
                ..ModalConfig::default()
            }),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            output: OutputConfig {
                filter_mode: FilterMode::BandPass,
                filter_cutoff: 4_000.0,
                master_gain_db: -6.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        }
    }

    fn pitch_tracking_waveguide_patch() -> ResonatorSynthPatch {
        ResonatorSynthPatch {
            name: "Pitch Tracking".to_string(),
            polyphony: 1,
            resonator_a: ResonatorConfig::Waveguide(crate::WaveguideConfig {
                loop_gain: 0.99,
                loop_filter_cutoff: 12_000.0,
                ..crate::WaveguideConfig::default()
            }),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            output: OutputConfig {
                filter_cutoff: 20_000.0,
                master_gain_db: 0.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        }
    }

    fn pitch_tracking_polyphonic_waveguide_patch() -> ResonatorSynthPatch {
        let mut patch = pitch_tracking_waveguide_patch();
        patch.polyphony = 2;
        patch
    }

    fn two_member_channel_notes() -> [MidiEvent; 2] {
        [
            MidiEvent::Note(NoteEvent::On {
                channel: 1,
                note: 48,
                velocity: 1.0,
            }),
            MidiEvent::Note(NoteEvent::On {
                channel: 2,
                note: 60,
                velocity: 1.0,
            }),
        ]
    }

    fn expression_for_slot(
        processor: &ResonatorProcessor<'_>,
        channel: u8,
        note: u8,
    ) -> VoiceExpression {
        (0..processor.engine.polyphony())
            .find(|index| {
                processor.engine.slot_channel(*index) == Some(channel)
                    && processor.engine.slot_note(*index) == Some(note)
            })
            .and_then(|index| processor.engine.slot_expression(index))
            .unwrap()
    }

    fn assert_slot_expression_gate(
        processor: &ResonatorProcessor<'_>,
        channel: u8,
        note: u8,
        gate: bool,
    ) {
        assert_eq!(
            expression_for_slot(processor, channel, note).stream.gate,
            gate
        );
    }

    fn assert_frequency_dominates(
        samples: &[f32],
        sample_rate: f32,
        high_note: f32,
        low_note: f32,
    ) {
        let high = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(high_note));
        let low = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(low_note));
        assert!(
            high > low,
            "note {high_note} magnitude {high} should exceed note {low_note} magnitude {low}"
        );
    }

    fn expression_filter_patch() -> ResonatorSynthPatch {
        let mut patch = test_patch();
        patch.output.filter_mode = FilterMode::LowPass;
        patch.output.filter_cutoff = 300.0;
        patch.output.filter_resonance = 0.0;
        patch.output.master_gain_db = 0.0;
        patch.modulation.slots[0] = ModulationSlot {
            enabled: true,
            source: ModulationSource::Aftertouch,
            destination: ModulationDestination::FilterCutoff,
            amount: 1.0,
        };
        patch
    }

    fn external_expression_filter_patch() -> ResonatorSynthPatch {
        let mut patch = expression_filter_patch();
        patch.polyphony = 1;
        patch.modulation.slots[0].amount = 0.5;
        patch.modulation.slots[1] = ModulationSlot {
            enabled: true,
            source: ModulationSource::Brightness,
            destination: ModulationDestination::FilterCutoff,
            amount: 0.5,
        };
        patch
    }

    fn aftertouch_resonator_damping_patch() -> ResonatorSynthPatch {
        resonator_damping_patch(ModulationSource::Aftertouch)
    }

    fn poly_pressure_resonator_damping_patch() -> ResonatorSynthPatch {
        let mut patch = resonator_damping_patch(ModulationSource::Aftertouch);
        patch.polyphony = 2;
        patch
    }

    fn mod_wheel_resonator_damping_patch() -> ResonatorSynthPatch {
        resonator_damping_patch(ModulationSource::ModWheel)
    }

    fn brightness_resonator_damping_patch() -> ResonatorSynthPatch {
        resonator_damping_patch(ModulationSource::Brightness)
    }

    fn resonator_damping_patch(source: ModulationSource) -> ResonatorSynthPatch {
        let mut patch = test_patch();
        patch.polyphony = 1;
        patch.resonator_a = ResonatorConfig::Waveguide(crate::WaveguideConfig {
            loop_gain: 0.62,
            loop_filter_cutoff: 12_000.0,
            ..crate::WaveguideConfig::default()
        });
        patch.routing = ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        };
        patch.output.filter_mode = FilterMode::LowPass;
        patch.output.filter_cutoff = 20_000.0;
        patch.output.master_gain_db = 0.0;
        patch.modulation.slots[0] = ModulationSlot {
            enabled: true,
            source,
            destination: ModulationDestination::ResonatorADamping,
            amount: 1.0,
        };
        patch
    }

    fn mean_abs_difference(left: &[f32], right: &[f32]) -> f32 {
        let len = left.len().min(right.len()).max(1);
        left.iter()
            .zip(right.iter())
            .map(|(left, right)| (left - right).abs())
            .sum::<f32>()
            / len as f32
    }
}
