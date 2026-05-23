use lindelion_dsp_utils::{analysis::peak_abs, math::snap_to_zero};
use lindelion_plugin_shell::{
    ExpressionSource, ExpressionStream, ManagedVoiceExpression, VoiceLike, VoiceManager,
    VoiceRenderStatus,
};

use super::voice::{Voice, VoiceExpression, VoiceTrigger};
use crate::{OutputConfig, ResonatorRouting};

pub use lindelion_plugin_shell::VoiceSlotState;

const IDLE_LEVEL_THRESHOLD: f32 = 1.0e-6;
const MAX_ENGINE_POLYPHONY: usize = 16;

#[derive(Debug)]
pub struct SynthEngine<'a> {
    voices: VoiceManager<MAX_ENGINE_POLYPHONY, Voice<'a>>,
}

impl<'a> SynthEngine<'a> {
    pub fn new(sample_rate: f32, polyphony: usize) -> Self {
        Self {
            voices: VoiceManager::new(polyphony, || Voice::new(sample_rate)),
        }
    }

    pub fn polyphony(&self) -> usize {
        self.voices.voice_limit()
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.active_voice_count()
    }

    pub fn slot_state(&self, index: usize) -> Option<VoiceSlotState> {
        self.voices.slot_state(index)
    }

    pub fn slot_note(&self, index: usize) -> Option<u8> {
        self.voices.slot_note(index)
    }

    pub fn slot_channel(&self, index: usize) -> Option<u8> {
        self.voices.slot_channel(index)
    }

    pub fn slot_last_level(&self, index: usize) -> Option<f32> {
        self.voices.slot_last_level(index)
    }

    #[cfg(test)]
    pub fn slot_expression(&self, index: usize) -> Option<VoiceExpression> {
        self.voices.slot_expression(index)
    }

    pub fn note_on(&mut self, trigger: VoiceTrigger<'a, '_>) -> usize {
        self.voices.start_voice(
            trigger.channel,
            trigger.midi_note,
            trigger.expression,
            trigger.patch.retrigger_resonators,
            |voice| voice.trigger(trigger),
        )
    }

    pub fn note_off(&mut self, note: u8) {
        self.voices.release_note(note);
    }

    pub fn note_off_for_channel(&mut self, channel: u8, note: u8) {
        self.voices.release_note_for_channel(channel, note);
    }

    pub fn all_notes_off(&mut self) {
        self.voices.release_all();
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        self.voices.set_pitch_bend(semitones);
    }

    pub fn set_expression_controls(
        &mut self,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) {
        self.voices
            .set_expression_controls(pitch_bend, pressure, brightness, mod_wheel);
    }

    pub fn set_expression_controls_for_channel(
        &mut self,
        channel: u8,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) {
        self.voices.set_expression_controls_for_channel(
            channel, pitch_bend, pressure, brightness, mod_wheel,
        );
    }

    pub fn set_poly_pressure(&mut self, channel: u8, note: u8, value: f32) {
        self.voices.set_poly_pressure(channel, note, value);
    }

    pub fn sync_expression_source(&mut self, source: &mut impl ExpressionSource) {
        self.voices.sync_expression_source(source);
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        self.voices
            .for_each_live_voice_mut(|voice| voice.set_output_config(output));
    }

    pub fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        self.voices
            .for_each_live_voice_mut(|voice| voice.set_waveguide_loop_gain(loop_gain));
    }

    pub fn set_routing(&mut self, routing: ResonatorRouting) {
        self.voices
            .for_each_live_voice_mut(|voice| voice.set_routing(routing));
    }

    pub fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());

        self.voices.process_live_voices(|voice| {
            let mut block_peak = 0.0_f32;
            for index in 0..len {
                let (sample_left, sample_right) = voice.process_stereo_sample();
                block_peak = block_peak.max(sample_left.abs()).max(sample_right.abs());
                left[index] = snap_to_zero(left[index] + sample_left);
                right[index] = snap_to_zero(right[index] + sample_right);
            }

            VoiceRenderStatus {
                last_level: block_peak,
                idle: voice.is_excitation_finished() && block_peak < IDLE_LEVEL_THRESHOLD,
            }
        });
    }

    pub fn render_replace(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.render_add(left, right);
    }
}

impl ManagedVoiceExpression for VoiceExpression {
    fn sanitized(self) -> Self {
        VoiceExpression::sanitized(self)
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

impl<'a> VoiceLike for Voice<'a> {
    type Expression = VoiceExpression;

    fn set_expression(&mut self, expression: Self::Expression) {
        Voice::set_expression(self, expression);
    }

    fn clear(&mut self) {
        Voice::clear(self);
    }
}

pub fn stereo_peak(left: &[f32], right: &[f32]) -> f32 {
    peak_abs(left).max(peak_abs(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModalConfig, ModalPreset, OutputConfig, ResonatorConfig, ResonatorRouting,
        ResonatorSynthPatch, WaveguideConfig, assert_no_allocations,
    };
    use lindelion_dsp_utils::analysis::{assert_all_finite, rms};

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
    fn released_same_note_voice_is_reused_when_retrigger_is_off() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch();
        patch.retrigger_resonators = false;
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 3);

        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_off(60);
        let reused = engine.note_on(trigger(60, &excitation, sample_rate, &patch));

        assert_eq!(reused, 0);
        assert_eq!(engine.slot_state(0), Some(VoiceSlotState::Active));
        assert_eq!(engine.active_voice_count(), 1);
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
    fn aggressive_series_dense_chord_stays_finite_and_bounded() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch();
        patch.polyphony = 8;
        patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.995,
            loop_filter_cutoff: 18_000.0,
            loop_filter_resonance: 0.85,
            loop_nonlinearity: 0.4,
            ..WaveguideConfig::default()
        });
        patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.995,
            loop_filter_cutoff: 18_000.0,
            loop_filter_resonance: 0.95,
            loop_nonlinearity: 0.5,
            ..WaveguideConfig::default()
        });
        patch.routing = ResonatorRouting::Series {
            mix_a: 1.0,
            mix_b: 1.0,
        };
        patch.output.master_gain_db = -9.0;
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 8);
        let mut left = vec![0.0; 32_768];
        let mut right = vec![0.0; 32_768];

        for note in [36, 40, 43, 47, 50, 55, 59, 64] {
            engine.note_on(trigger(note, &excitation, sample_rate, &patch));
        }
        engine.render_replace(&mut left, &mut right);

        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(stereo_peak(&left, &right) < 8.0);
        assert!(rms(&left) > 0.000_001);
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
        assert_eq!(
            engine.slot_state(slot_index),
            Some(VoiceSlotState::Released)
        );
        assert_eq!(
            engine.slot_expression(slot_index),
            Some(live_expression.with_gate(false))
        );
    }

    #[test]
    fn note_off_for_channel_routes_gate_only_to_owned_voice_slot() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 2);
        let slot_a = engine.note_on(channel_trigger(1, 60, &excitation, sample_rate, &patch));
        let slot_b = engine.note_on(channel_trigger(2, 60, &excitation, sample_rate, &patch));

        engine.note_off_for_channel(2, 60);

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
    fn poly_pressure_updates_only_matching_voice_slot() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 3);
        let slot_a = engine.note_on(channel_trigger(0, 60, &excitation, sample_rate, &patch));
        let slot_b = engine.note_on(channel_trigger(1, 64, &excitation, sample_rate, &patch));

        engine.set_expression_controls(0.0, 0.2, 0.0, 0.0);
        engine.set_poly_pressure(1, 64, 0.9);

        assert_eq!(engine.slot_expression(slot_a).unwrap().stream.pressure, 0.2);
        assert_eq!(engine.slot_expression(slot_b).unwrap().stream.pressure, 0.9);

        engine.set_expression_controls(0.0, 0.4, 0.0, 0.0);
        assert_eq!(engine.slot_expression(slot_a).unwrap().stream.pressure, 0.4);
        assert_eq!(engine.slot_expression(slot_b).unwrap().stream.pressure, 0.9);

        engine.set_poly_pressure(0, 64, 0.1);
        assert_eq!(engine.slot_expression(slot_b).unwrap().stream.pressure, 0.9);
    }

    #[test]
    fn channel_expression_controls_update_only_owned_voice_slots() {
        let sample_rate = 48_000.0;
        let patch = test_patch();
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 3);
        let slot_a = engine.note_on(channel_trigger(1, 60, &excitation, sample_rate, &patch));
        let slot_b = engine.note_on(channel_trigger(2, 64, &excitation, sample_rate, &patch));

        engine.set_expression_controls_for_channel(2, 1.25, 0.5, 0.6, 0.7);

        assert_expression_controls(engine.slot_expression(slot_a).unwrap(), 0.0, 0.0, 0.0, 0.0);
        assert_expression_controls(engine.slot_expression(slot_b).unwrap(), 1.25, 0.5, 0.6, 0.7);

        engine.set_expression_controls(0.25, 0.2, 0.3, 0.4);

        assert_expression_controls(engine.slot_expression(slot_a).unwrap(), 0.25, 0.2, 0.3, 0.4);
        assert_expression_controls(engine.slot_expression(slot_b).unwrap(), 0.25, 0.2, 0.3, 0.4);
    }

    #[test]
    fn released_quiet_voice_eventually_becomes_idle() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch();
        patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.1,
            ..WaveguideConfig::default()
        });
        patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.0,
            ..WaveguideConfig::default()
        });
        patch.routing = ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        };
        let excitation = impulse();
        let mut engine = SynthEngine::new(sample_rate, 1);
        let mut left = vec![0.0; 16_384];
        let mut right = vec![0.0; 16_384];

        engine.note_on(trigger(60, &excitation, sample_rate, &patch));
        engine.note_off(60);
        engine.render_replace(&mut left, &mut right);
        engine.render_replace(&mut left, &mut right);

        assert_eq!(engine.slot_state(0), Some(VoiceSlotState::Idle));
    }

    #[test]
    fn note_on_and_render_do_not_allocate() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch();
        patch.resonator_a = ResonatorConfig::Modal(ModalConfig {
            mode_count: 256,
            preset: ModalPreset::Bell,
            ..ModalConfig::default()
        });
        patch.resonator_b = ResonatorConfig::Modal(ModalConfig {
            mode_count: 256,
            preset: ModalPreset::GlassBowl,
            ..ModalConfig::default()
        });
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

        assert_no_allocations("voice_stealing_note_on", || {
            for note in 68..80 {
                engine.note_on(trigger(note, &excitation, sample_rate, &patch));
            }
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

    fn assert_expression_controls(
        expression: VoiceExpression,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) {
        assert_eq!(expression.stream.pitch_bend, pitch_bend);
        assert_eq!(expression.stream.pressure, pressure);
        assert_eq!(expression.stream.brightness, brightness);
        assert_eq!(expression.mod_wheel, mod_wheel);
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
            resonator_a: ResonatorConfig::Modal(ModalConfig {
                mode_count: 16,
                preset: ModalPreset::GenericStrike,
                decay_global: 0.4,
                ..ModalConfig::default()
            }),
            resonator_b: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.9,
                ..WaveguideConfig::default()
            }),
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
}
