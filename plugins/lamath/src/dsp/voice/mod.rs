mod output_stage;
mod resonator_stack;
mod surrounding;

#[cfg(test)]
mod tests;

use lindelion_dsp_utils::{
    energy::EnergyFollower,
    equal_power_pan,
    math::{midi_note_to_hz, semitones_to_ratio},
    smoothing::{SmoothedParam, SmoothedParamSpec},
};
use lindelion_plugin_shell::ExpressionStream;

pub(crate) use self::output_stage::OutputStage;
pub(crate) use self::surrounding::SurroundingStage;
use super::excitation::{LiveExcitationLatchCapture, SelectedExcitations, VoiceExcitation};
use crate::{
    OutputConfig, ResonatorRouting, ResonatorSynthPatch, dsp::constants::DSP_FALLBACK_SAMPLE_RATE,
};

const STRUCTURAL_RAMP_MS: f32 = 1.0;
const PITCH_BEND_SMOOTH_MS: f32 = 8.0;
const PITCH_BEND_EPSILON: f32 = 0.000_1;

pub(crate) use resonator_stack::ResonatorStack;

/// Modal-only Lamath does not run an oversampled resonator path, so the reported
/// processing latency is zero.
pub(crate) const RESONATOR_OVERSAMPLING_LATENCY_SAMPLES: u32 = 0;

#[derive(Debug, Clone, Copy)]
pub struct VoiceTrigger<'a, 'p> {
    pub channel: u8,
    pub midi_note: u8,
    pub expression: VoiceExpression,
    pub excitations: SelectedExcitations<'a>,
    pub live_latch: Option<LiveExcitationLatchCapture<'p>>,
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

    #[cfg(test)]
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
    #[cfg(test)]
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
            excitations: SelectedExcitations::from_single(
                excitation_samples,
                excitation_sample_rate,
            ),
            live_latch: None,
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
            excitations,
            live_latch: None,
            patch,
        }
    }
}

#[derive(Debug)]
pub struct Voice<'a> {
    sample_rate: f32,
    excitation: VoiceExcitation<'a>,
    live_latch: super::excitation::VoiceLiveExcitationLatch,
    excitation_gain: f32,
    midi_note: u8,
    expression: VoiceExpression,
    pitch_bend_semitones: SmoothedParam,
    applied_pitch_bend_semitones: f32,
    resonators: ResonatorStack,
    surrounding: SurroundingStage,
    output: OutputStage,
    energy_follower: EnergyFollower,
    energy: f32,
}

impl<'a> Voice<'a> {
    #[cfg(test)]
    pub fn new(sample_rate: f32) -> Self {
        Self::with_live_latch_capacity(sample_rate, 0)
    }

    pub fn with_live_latch_capacity(sample_rate: f32, live_latch_capacity_samples: usize) -> Self {
        Self {
            sample_rate,
            excitation: VoiceExcitation::default(),
            live_latch: super::excitation::VoiceLiveExcitationLatch::with_capacity(
                live_latch_capacity_samples,
            ),
            excitation_gain: 0.0,
            midi_note: 60,
            expression: VoiceExpression::default(),
            pitch_bend_semitones: pitch_bend_param(sample_rate, 0.0),
            applied_pitch_bend_semitones: 0.0,
            resonators: ResonatorStack::new(sample_rate),
            surrounding: SurroundingStage::new(sample_rate),
            output: OutputStage::new(sample_rate),
            energy_follower: EnergyFollower::new(sample_rate),
            energy: 0.0,
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
        if let Some(capture) = trigger.live_latch {
            self.live_latch.trigger(capture);
        } else {
            self.live_latch.clear();
        }

        self.excitation_gain = velocity_to_gain(expression.stream.velocity, 1.0);
        self.midi_note = trigger.midi_note;
        self.expression = expression;
        self.pitch_bend_semitones.reset(trigger_pitch_bend);
        self.applied_pitch_bend_semitones = trigger_pitch_bend;
        self.energy_follower.reset();
        self.energy = 0.0;

        self.resonators
            .set_base_configs(trigger.patch.resonator_a, trigger.patch.resonator_b);
        self.resonators.configure(
            self.base_frequency(),
            trigger.patch.retrigger_resonators,
            true,
        );
        self.resonators.reset_routing(trigger.patch.routing);
        self.resonators.reset_series_conditioner(self.sample_rate);
        self.surrounding.set_config(trigger.patch.surrounding);
        self.surrounding.trigger();
        self.output.reset(trigger.patch.output);
    }

    #[cfg(test)]
    pub fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());

        for index in 0..len {
            let (sample_left, sample_right) = self.process_stereo_sample_with_live_excitation(0.0);
            left[index] += sample_left;
            right[index] += sample_right;
        }
    }

    pub(super) fn process_stereo_sample_with_live_excitation(
        &mut self,
        live_excitation: f32,
    ) -> (f32, f32) {
        let sample = self.process_sample_with_live_excitation(live_excitation);
        equal_power_pan(sample, self.output.next_pan())
    }

    pub fn is_excitation_finished(&self) -> bool {
        self.excitation.is_finished() && self.live_latch.is_finished()
    }

    pub fn continue_live_latch_capture(&mut self, sidechain: &[f32]) {
        self.live_latch.continue_capture(sidechain);
    }

    pub fn set_expression(&mut self, expression: VoiceExpression) {
        let expression = expression.sanitized();
        self.pitch_bend_semitones
            .set_target(sanitize_pitch_bend(expression.stream.pitch_bend));
        self.expression = expression;
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        self.output.set_config(output);
    }

    pub fn set_routing(&mut self, routing: ResonatorRouting) {
        self.resonators.set_routing(routing);
    }

    pub fn clear(&mut self) {
        self.excitation.clear();
        self.live_latch.clear();
        self.excitation_gain = 0.0;
        self.expression = VoiceExpression::default();
        self.pitch_bend_semitones.reset(0.0);
        self.applied_pitch_bend_semitones = 0.0;
        self.resonators.clear(self.sample_rate);
        self.surrounding.reset();
        self.output.clear();
        self.energy_follower.reset();
        self.energy = 0.0;
    }

    fn process_sample_with_live_excitation(&mut self, live_excitation: f32) -> f32 {
        let structural_gain = self.apply_structural_transitions();
        self.apply_smoothed_expression_targets();

        let excitation =
            (self.excitation.next_sample() + self.live_latch.next_sample() + live_excitation)
                * self.excitation_gain;
        let resonator_output = self.resonators.process_sample(excitation);
        self.energy = self.energy_follower.observe(resonator_output);
        let staged_output = self.resonators.staged_output();
        let surrounded = self
            .surrounding
            .process(staged_output, self.effort(), self.energy);
        self.output
            .process_sample(surrounded, self.sample_rate, 0.0, 1.0, structural_gain)
    }

    fn apply_structural_transitions(&mut self) -> f32 {
        self.resonators
            .apply_structural_transitions(self.sample_rate)
            .min(self.output.apply_structural_transitions())
    }

    fn apply_smoothed_expression_targets(&mut self) {
        let pitch_bend = self.pitch_bend_semitones.next_sample();
        if (pitch_bend - self.applied_pitch_bend_semitones).abs() > PITCH_BEND_EPSILON {
            self.applied_pitch_bend_semitones = pitch_bend;
            self.resonators.retune(self.base_frequency());
        }
    }

    fn base_frequency(&self) -> f32 {
        midi_note_to_hz(self.midi_note as f32 + self.applied_pitch_bend_semitones)
    }

    fn effort(&self) -> f32 {
        let stream = self.expression.stream.sanitized();
        sanitize_unit(stream.velocity.max(stream.pressure))
    }
}

fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn sanitize_pitch_bend(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-96.0, 96.0)
    } else {
        0.0
    }
}

pub(crate) fn velocity_to_gain(velocity: f32, depth: f32) -> f32 {
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

fn pitch_bend_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(-96.0, 96.0, 0.0, PITCH_BEND_SMOOTH_MS, PITCH_BEND_EPSILON)
}

fn pitch_bend_param(sample_rate: f32, semitones: f32) -> SmoothedParam {
    SmoothedParam::with_initial(pitch_bend_spec(), sample_rate, semitones)
}
