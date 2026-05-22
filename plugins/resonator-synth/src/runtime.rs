use ahara_plugin_shell::{ControlEvent, MidiEvent, NoteEvent, ParameterId};

use crate::{
    ExcitationSlot, ResonatorSynthPatch,
    dsp::{
        ExcitationSelector, MAX_EXCITATION_LAYERS, RuntimeExcitationSlot, SelectedExcitations,
        SynthEngine, VoiceTrigger,
    },
};

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
    controls: RealtimeControls,
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
            controls: RealtimeControls::default(),
        }
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);

        for event in events {
            self.handle_event(*event);
        }

        self.engine.render_add(left, right);
    }

    pub fn active_voice_count(&self) -> usize {
        self.engine.active_voice_count()
    }

    pub fn patch(&self) -> &ResonatorSynthPatch {
        &self.runtime_patch.patch
    }

    pub fn set_parameter_plain(&mut self, parameter: ParameterId, value: f32) {
        match parameter.0 {
            1 => self.set_master_gain_db(value),
            2 => self.set_loop_gain(value),
            3 => self.set_filter_cutoff(value),
            _ => {}
        }
    }

    fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::Note(NoteEvent::On {
                note,
                velocity,
                channel: _,
            }) if velocity > 0.0 => self.note_on(note, velocity),
            MidiEvent::Note(NoteEvent::On {
                note,
                velocity: _,
                channel: _,
            })
            | MidiEvent::Note(NoteEvent::Off {
                note,
                velocity: _,
                channel: _,
            }) => self.engine.note_off(note),
            MidiEvent::Control(control) => self.handle_control(control),
        }
    }

    fn handle_control(&mut self, control: ControlEvent) {
        match control {
            ControlEvent::PitchBend {
                semitones,
                channel: _,
            } => {
                let range = self
                    .runtime_patch
                    .patch
                    .modulation
                    .pitch_bend_range_semitones
                    .abs()
                    .max(0.0);
                self.controls.pitch_bend_semitones = semitones.clamp(-range, range);
                self.engine
                    .set_pitch_bend(self.controls.pitch_bend_semitones);
            }
            ControlEvent::ChannelPressure { value, channel: _ } => {
                self.controls.aftertouch = value.clamp(0.0, 1.0);
            }
            ControlEvent::ContinuousController {
                controller,
                value,
                channel: _,
            } => match controller {
                1 => self.controls.mod_wheel = value.clamp(0.0, 1.0),
                74 => self.controls.brightness = value.clamp(0.0, 1.0),
                _ => {}
            },
        }
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        let selected = self.selector.select(&self.runtime_patch.slots, velocity);
        let excitations = if selected.is_empty() {
            SelectedExcitations::from_single(&BUILTIN_EXCITATION, BUILTIN_EXCITATION_SAMPLE_RATE)
        } else {
            selected
        };
        let modulation = self.runtime_patch.patch.modulation;
        let mut trigger =
            VoiceTrigger::with_excitations(note, velocity, excitations, &self.runtime_patch.patch);
        trigger.pitch_bend_semitones = self.controls.pitch_bend_semitones;
        trigger.aftertouch = self.controls.aftertouch;
        trigger.mod_wheel = self.controls.mod_wheel;
        trigger.brightness = self.controls.brightness;
        trigger.modulation = modulation;
        self.engine.note_on(trigger);
    }

    fn set_master_gain_db(&mut self, gain_db: f32) {
        self.runtime_patch.patch.output.master_gain_db = gain_db.clamp(-60.0, 12.0);
        self.engine
            .set_output_config(self.runtime_patch.patch.output);
    }

    fn set_loop_gain(&mut self, loop_gain: f32) {
        let loop_gain = loop_gain.clamp(0.0, 0.999);
        set_patch_loop_gain(&mut self.runtime_patch.patch, loop_gain);
        self.engine.set_waveguide_loop_gain(loop_gain);
    }

    fn set_filter_cutoff(&mut self, cutoff_hz: f32) {
        self.runtime_patch.patch.output.filter_cutoff = cutoff_hz.clamp(20.0, 20_000.0);
        self.engine
            .set_output_config(self.runtime_patch.patch.output);
    }

    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RealtimeControls {
    pitch_bend_semitones: f32,
    aftertouch: f32,
    mod_wheel: f32,
    brightness: f32,
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

fn set_patch_loop_gain(patch: &mut ResonatorSynthPatch, loop_gain: f32) {
    if let crate::ResonatorConfig::Waveguide(mut config) = patch.resonator_a {
        config.loop_gain = loop_gain;
        patch.resonator_a = crate::ResonatorConfig::Waveguide(config);
    }
    if let crate::ResonatorConfig::Waveguide(mut config) = patch.resonator_b {
        config.loop_gain = loop_gain;
        patch.resonator_b = crate::ResonatorConfig::Waveguide(config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FilterMode, ModalConfig, ModalPreset, OutputConfig, ResonatorConfig, ResonatorRouting,
        assert_no_allocations,
    };
    use ahara_dsp_utils::analysis::{assert_all_finite, rms};

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
            MidiEvent::Control(ControlEvent::ContinuousController {
                channel: 0,
                controller: 1,
                value: 0.5,
            }),
        ];
        assert_no_allocations("processor process controls", || {
            processor.process(&controls, &mut left, &mut right);
        });
    }

    #[test]
    fn state_roundtrip_preserves_patch() {
        let mut synth = crate::ResonatorSynth::default();
        let mut patch = test_patch();
        patch.name = "Roundtrip".to_string();
        synth.set_patch_for_test(patch.clone());

        let state = ahara_plugin_shell::AudioPlugin::state(&synth);
        let mut restored = crate::ResonatorSynth::default();
        ahara_plugin_shell::AudioPlugin::load_state(&mut restored, state);

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
}
