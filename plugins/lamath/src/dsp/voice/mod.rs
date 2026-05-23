mod modulation_state;
mod output_stage;
mod resonator_stack;

#[cfg(test)]
mod tests;

use lindelion_dsp_utils::{
    equal_power_pan,
    math::{midi_note_to_hz, semitones_to_ratio},
};
use lindelion_plugin_shell::ExpressionStream;

use self::{
    modulation_state::{ModulationSources, ModulationState, sanitize_pitch_bend},
    output_stage::OutputStage,
    resonator_stack::ResonatorStack,
};
use super::excitation::{SelectedExcitations, VoiceExcitation};
use crate::{
    ModulationConfig, ModulationDestination, OutputConfig, ResonatorRouting, ResonatorSynthPatch,
    dsp::constants::DSP_FALLBACK_SAMPLE_RATE,
};

const PARAMETER_SMOOTH_MS: f32 = 20.0;
const PARAMETER_EPSILON: f32 = 0.000_001;
const STRUCTURAL_RAMP_MS: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct VoiceTrigger<'a, 'p> {
    pub channel: u8,
    pub midi_note: u8,
    pub expression: VoiceExpression,
    pub modulation: ModulationConfig,
    pub excitations: SelectedExcitations<'a>,
    pub patch: &'p ResonatorSynthPatch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceExpression {
    pub stream: ExpressionStream,
    pub mod_wheel: f32,
}

impl VoiceExpression {
    pub fn note_on(velocity: f32) -> Self {
        Self {
            stream: ExpressionStream::note_on(velocity),
            mod_wheel: 0.0,
        }
    }

    pub fn with_controls(
        velocity: f32,
        pitch_bend: f32,
        pressure: f32,
        brightness: f32,
        mod_wheel: f32,
    ) -> Self {
        Self {
            stream: ExpressionStream {
                pitch_bend,
                pressure,
                brightness,
                velocity,
                gate: true,
            },
            mod_wheel,
        }
        .sanitized()
    }

    pub fn sanitized(self) -> Self {
        Self {
            stream: self.stream.sanitized(),
            mod_wheel: sanitize_unit(self.mod_wheel),
        }
    }

    pub fn with_gate(mut self, gate: bool) -> Self {
        self.stream.gate = gate;
        self
    }
}

impl Default for VoiceExpression {
    fn default() -> Self {
        Self {
            stream: ExpressionStream::default(),
            mod_wheel: 0.0,
        }
    }
}

impl<'a, 'p> VoiceTrigger<'a, 'p> {
    pub fn new(
        midi_note: u8,
        velocity: f32,
        excitation_samples: &'a [f32],
        excitation_sample_rate: f32,
        patch: &'p ResonatorSynthPatch,
    ) -> Self {
        Self {
            channel: 0,
            midi_note,
            expression: VoiceExpression::note_on(velocity),
            modulation: patch.modulation,
            excitations: SelectedExcitations::from_single(
                excitation_samples,
                excitation_sample_rate,
            ),
            patch,
        }
    }

    pub fn with_excitations(
        midi_note: u8,
        velocity: f32,
        excitations: SelectedExcitations<'a>,
        patch: &'p ResonatorSynthPatch,
    ) -> Self {
        Self {
            channel: 0,
            midi_note,
            expression: VoiceExpression::note_on(velocity),
            modulation: patch.modulation,
            excitations,
            patch,
        }
    }
}

#[derive(Debug)]
pub struct Voice<'a> {
    sample_rate: f32,
    excitation: VoiceExcitation<'a>,
    excitation_gain: f32,
    midi_note: u8,
    resonators: ResonatorStack,
    modulation: ModulationState,
    output: OutputStage,
}

impl<'a> Voice<'a> {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            excitation: VoiceExcitation::default(),
            excitation_gain: 0.0,
            midi_note: 60,
            resonators: ResonatorStack::new(sample_rate),
            modulation: ModulationState::new(sample_rate),
            output: OutputStage::new(sample_rate),
        }
    }

    pub fn trigger(&mut self, trigger: VoiceTrigger<'a, '_>) {
        let expression = trigger.expression.sanitized();
        let trigger_pitch_bend = sanitize_pitch_bend(expression.stream.pitch_bend);
        let excitation_pitch_ratio =
            semitones_to_ratio(trigger.midi_note as f32 - 60.0 + trigger_pitch_bend);

        self.excitation.trigger(
            trigger.excitations,
            self.sample_rate,
            excitation_pitch_ratio,
        );
        self.excitation_gain = velocity_to_gain(
            expression.stream.velocity,
            trigger.modulation.velocity_to_excitation_depth,
        );
        self.midi_note = trigger.midi_note;
        self.modulation.trigger(
            trigger.midi_note,
            expression,
            trigger.modulation,
            trigger_pitch_bend,
        );
        self.resonators
            .set_base_configs(trigger.patch.resonator_a, trigger.patch.resonator_b);

        let static_sources = self.modulation.static_sources();
        self.resonators.configure_modulated(
            trigger.modulation,
            static_sources,
            self.base_frequency(),
            trigger.patch.retrigger_resonators,
            true,
        );
        self.resonators.reset_routing(trigger.patch.routing);
        self.output.reset(trigger.patch.output);
        self.modulation
            .reset_waveguide_loop_gain(self.resonators.current_loop_gain());
        self.resonators.reset_series_conditioner(self.sample_rate);
    }

    pub fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());

        for index in 0..len {
            let (sample_left, sample_right) = self.process_stereo_sample();
            left[index] += sample_left;
            right[index] += sample_right;
        }
    }

    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        let sample = self.process_sample();
        equal_power_pan(sample, self.output.next_pan())
    }

    pub fn is_excitation_finished(&self) -> bool {
        self.excitation.is_finished() && self.modulation.is_amp_idle()
    }

    pub fn note_off(&mut self) {
        self.modulation.note_off();
    }

    pub fn set_pitch_bend(&mut self, pitch_bend_semitones: f32) {
        self.modulation.set_pitch_bend(pitch_bend_semitones);
    }

    pub fn set_expression(&mut self, expression: VoiceExpression) {
        self.modulation.set_expression(expression);
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        self.output.set_config(output);
    }

    pub fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        self.resonators.set_base_waveguide_loop_gain(loop_gain);
        self.modulation.set_waveguide_loop_gain(loop_gain);
    }

    pub fn set_routing(&mut self, routing: ResonatorRouting) {
        self.resonators.set_routing(routing);
    }

    pub fn clear(&mut self) {
        self.excitation.clear();
        self.excitation_gain = 0.0;
        self.resonators.clear(self.sample_rate);
        self.modulation.clear(self.resonators.current_loop_gain());
        self.output.clear();
    }

    pub fn process_sample(&mut self) -> f32 {
        let structural_gain = self.apply_structural_transitions();
        self.apply_smoothed_modulation_targets();

        let sources = self.modulation.next_sources(self.sample_rate);
        self.apply_live_resonator_modulation(sources);
        let excitation_mod = self
            .modulation
            .modulation_sum(ModulationDestination::ExcitationGain, sources);
        let excitation = self.excitation.next_sample()
            * self.excitation_gain
            * (1.0 + excitation_mod).clamp(0.0, 2.0);

        let resonator_output = self.resonators.process_sample(excitation);

        let cutoff_mod = self
            .modulation
            .modulation_sum(ModulationDestination::FilterCutoff, sources);
        self.output.process_sample(
            resonator_output,
            self.sample_rate,
            cutoff_mod,
            sources.amp_envelope,
            structural_gain,
        )
    }

    fn apply_structural_transitions(&mut self) -> f32 {
        self.resonators
            .apply_structural_transitions(self.sample_rate)
            .min(self.output.apply_structural_transitions())
    }

    fn apply_smoothed_modulation_targets(&mut self) {
        if let Some(pitch_bend) = self.modulation.next_pitch_bend_change() {
            self.resonators
                .retune(midi_note_to_hz(self.midi_note as f32 + pitch_bend));
        }

        if let Some(loop_gain) = self.modulation.next_waveguide_loop_gain_change() {
            self.resonators.set_waveguide_loop_gain(loop_gain);
        }
    }

    fn apply_live_resonator_modulation(&mut self, sources: ModulationSources) {
        if self.modulation.resonator_modulation_active() {
            self.resonators.configure_modulated(
                self.modulation.config(),
                sources,
                self.base_frequency(),
                false,
                false,
            );
        }
    }

    fn base_frequency(&self) -> f32 {
        midi_note_to_hz(self.midi_note as f32 + self.modulation.applied_pitch_bend_semitones())
    }
}

fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn velocity_to_gain(velocity: f32, depth: f32) -> f32 {
    let velocity = velocity.clamp(0.0, 1.0);
    let depth = depth.clamp(0.0, 1.0);
    (1.0 - depth) + velocity * depth
}

pub(super) fn structural_ramp_samples(sample_rate: f32) -> usize {
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        DSP_FALLBACK_SAMPLE_RATE
    };
    (sample_rate * STRUCTURAL_RAMP_MS * 0.001)
        .round()
        .clamp(8.0, 256.0) as usize
}
