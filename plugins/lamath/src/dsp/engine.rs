use lindelion_dsp_utils::math::snap_to_zero;
use lindelion_plugin_shell::{
    ExpressionSource, ExpressionStream, ManagedVoiceExpression, VoiceLike, VoiceManager,
    VoiceRenderStatus,
};

use super::{
    excitation::LiveExcitationBlock,
    voice::{Voice, VoiceExpression, VoiceTrigger},
};
use crate::{OutputConfig, ResonatorRouting};

#[cfg(test)]
use lindelion_plugin_shell::VoiceSlotState;

const IDLE_LEVEL_THRESHOLD: f32 = 1.0e-6;
const MAX_ENGINE_POLYPHONY: usize = 16;

#[derive(Debug)]
pub struct SynthEngine<'a> {
    voices: VoiceManager<MAX_ENGINE_POLYPHONY, Voice<'a>>,
}

impl<'a> SynthEngine<'a> {
    #[cfg(test)]
    pub fn new(sample_rate: f32, polyphony: usize) -> Self {
        Self::with_live_latch_capacity(sample_rate, polyphony, 0)
    }

    pub fn with_live_latch_capacity(
        sample_rate: f32,
        polyphony: usize,
        live_latch_capacity_samples: usize,
    ) -> Self {
        Self {
            voices: VoiceManager::new(polyphony, || {
                Voice::with_live_latch_capacity(sample_rate, live_latch_capacity_samples)
            }),
        }
    }

    pub fn polyphony(&self) -> usize {
        self.voices.voice_limit()
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.active_voice_count()
    }

    #[cfg(test)]
    pub fn slot_state(&self, index: usize) -> Option<VoiceSlotState> {
        self.voices.slot_state(index)
    }

    pub fn slot_note(&self, index: usize) -> Option<u8> {
        self.voices.slot_note(index)
    }

    pub fn slot_channel(&self, index: usize) -> Option<u8> {
        self.voices.slot_channel(index)
    }

    #[cfg(test)]
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

    #[cfg(test)]
    pub fn note_off(&mut self, note: u8) {
        self.voices.release_note(note);
    }

    pub fn note_off_voice(&mut self, voice_id: usize) -> bool {
        self.voices.release_voice(voice_id)
    }

    #[cfg(test)]
    pub fn all_notes_off(&mut self) {
        self.voices.release_all();
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

    pub fn continue_live_latch_captures(&mut self, sidechain: &[f32]) {
        if sidechain.is_empty() {
            return;
        }

        self.voices
            .for_each_live_voice_mut(|voice| voice.continue_live_latch_capture(sidechain));
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        self.voices
            .for_each_live_voice_mut(|voice| voice.set_output_config(output));
    }

    pub fn set_routing(&mut self, routing: ResonatorRouting) {
        self.voices
            .for_each_live_voice_mut(|voice| voice.set_routing(routing));
    }

    pub fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.render_add_with_live_excitation(left, right, LiveExcitationBlock::disabled());
    }

    pub fn render_add_with_live_excitation(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        live_excitation: LiveExcitationBlock<'_>,
    ) {
        let len = left.len().min(right.len());

        self.voices.process_live_voices(|voice| {
            let mut block_peak = 0.0_f32;
            let mut invalid_sample = false;
            for index in 0..len {
                let (sample_left, sample_right) = voice
                    .process_stereo_sample_with_live_excitation(live_excitation.sample_at(index));
                if !sample_left.is_finite() || !sample_right.is_finite() {
                    voice.clear();
                    invalid_sample = true;
                    break;
                }
                block_peak = block_peak.max(sample_left.abs()).max(sample_right.abs());
                left[index] = snap_to_zero(left[index] + sample_left);
                right[index] = snap_to_zero(right[index] + sample_right);
            }

            VoiceRenderStatus {
                last_level: if invalid_sample { 0.0 } else { block_peak },
                idle: invalid_sample
                    || (voice.is_excitation_finished() && block_peak < IDLE_LEVEL_THRESHOLD),
            }
        });
    }

    #[cfg(test)]
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

#[cfg(test)]
mod tests;
