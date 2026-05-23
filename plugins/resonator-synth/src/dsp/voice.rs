use ahara_dsp_utils::{
    db_to_gain,
    envelope::{Adsr, AdsrState, EnvelopePhase},
    equal_power_pan,
    filters::{Biquad, BiquadCoefficients},
    math::{finite_clamp, midi_note_to_hz, semitones_to_ratio, snap_to_zero},
    params::{StructuralChangePolicy, StructuralParam},
    smoothing::{SmoothedParam, SmoothedParamSpec},
    soft_saturate,
};
use ahara_plugin_shell::ExpressionStream;

use super::{
    excitation::{SelectedExcitations, VoiceExcitation},
    modal::{ModalBank, ModalBankParams},
    waveguide::{WaveguideParams, WaveguideResonator},
};
use crate::{
    FilterMode, ModalConfig, ModulationConfig, ModulationDestination, ModulationSource,
    OutputConfig, ResonatorConfig, ResonatorRouting, ResonatorSynthPatch, WaveguideConfig,
};

const INTERNAL_HEADROOM_DB: f32 = -12.0;
const PARAMETER_SMOOTH_MS: f32 = 20.0;
const PITCH_BEND_SMOOTH_MS: f32 = 8.0;
const PARAMETER_EPSILON: f32 = 0.000_001;
const FILTER_CUTOFF_EPSILON: f32 = 0.001;
const PARALLEL_MIX_EPSILON: f32 = 0.000_001;
const PITCH_BEND_EPSILON: f32 = 0.000_1;
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
    resonator_a: ResonatorEngine,
    resonator_b: ResonatorEngine,
    routing: StructuralParam<ResonatorRouting>,
    output: OutputConfig,
    modulation: ModulationConfig,
    resonator_modulation_active: bool,
    midi_note: u8,
    base_resonator_a_config: ResonatorConfig,
    base_resonator_b_config: ResonatorConfig,
    resonator_a_config: ResonatorConfig,
    resonator_b_config: ResonatorConfig,
    expression: VoiceExpression,
    velocity: f32,
    aftertouch: f32,
    mod_wheel: f32,
    brightness: f32,
    amp_envelope: Adsr,
    amp_state: AdsrState,
    secondary_envelope: Adsr,
    secondary_state: AdsrState,
    lfo_phase: f32,
    lfo_hold: f32,
    output_filter: Biquad,
    output_filter_mode: StructuralParam<FilterMode>,
    output_filter_cutoff: SmoothedParam,
    output_filter_resonance: SmoothedParam,
    master_gain: SmoothedParam,
    saturation_drive: SmoothedParam,
    master_pan: SmoothedParam,
    parallel_mix_a: SmoothedParam,
    parallel_mix_b: SmoothedParam,
    pitch_bend_semitones: SmoothedParam,
    applied_pitch_bend_semitones: f32,
    waveguide_loop_gain: SmoothedParam,
    applied_waveguide_loop_gain: f32,
    series_conditioner: SeriesConditioner,
}

impl<'a> Voice<'a> {
    pub fn new(sample_rate: f32) -> Self {
        let output = OutputConfig::default();
        let modulation = ModulationConfig::default();
        let routing = ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        };

        Self {
            sample_rate,
            excitation: VoiceExcitation::default(),
            excitation_gain: 0.0,
            resonator_a: ResonatorEngine::new(sample_rate),
            resonator_b: ResonatorEngine::new(sample_rate),
            routing: StructuralParam::with_ramp_samples(
                routing,
                StructuralChangePolicy::LiveMuteRamp,
                structural_ramp_samples(sample_rate),
            ),
            output,
            modulation,
            resonator_modulation_active: false,
            midi_note: 60,
            base_resonator_a_config: ResonatorConfig::Modal(ModalConfig::default()),
            base_resonator_b_config: ResonatorConfig::Waveguide(WaveguideConfig::default()),
            resonator_a_config: ResonatorConfig::Modal(ModalConfig::default()),
            resonator_b_config: ResonatorConfig::Waveguide(WaveguideConfig::default()),
            expression: VoiceExpression::default(),
            velocity: 0.0,
            aftertouch: 0.0,
            mod_wheel: 0.0,
            brightness: 0.0,
            amp_envelope: modulation.amp_envelope.into(),
            amp_state: AdsrState::default(),
            secondary_envelope: modulation.secondary_envelope.into(),
            secondary_state: AdsrState::default(),
            lfo_phase: 0.0,
            lfo_hold: 0.0,
            output_filter: Biquad::new(output_filter_coefficients(
                sample_rate,
                output.filter_cutoff,
                output.filter_resonance,
                output.filter_mode,
            )),
            output_filter_mode: StructuralParam::with_ramp_samples(
                output.filter_mode,
                StructuralChangePolicy::LiveMuteRamp,
                structural_ramp_samples(sample_rate),
            ),
            output_filter_cutoff: output_filter_cutoff_param(sample_rate, output.filter_cutoff),
            output_filter_resonance: output_filter_resonance_param(
                sample_rate,
                output.filter_resonance,
            ),
            master_gain: master_gain_param(sample_rate, output.master_gain_db),
            saturation_drive: saturation_drive_param(sample_rate, output.saturation_drive),
            master_pan: master_pan_param(sample_rate, output.master_pan),
            parallel_mix_a: parallel_mix_param(sample_rate, parallel_mix_a(routing)),
            parallel_mix_b: parallel_mix_param(sample_rate, parallel_mix_b(routing)),
            pitch_bend_semitones: pitch_bend_param(sample_rate, 0.0),
            applied_pitch_bend_semitones: 0.0,
            waveguide_loop_gain: waveguide_loop_gain_param(sample_rate, 0.92),
            applied_waveguide_loop_gain: 0.92,
            series_conditioner: SeriesConditioner::new(sample_rate),
        }
    }

    pub fn trigger(&mut self, trigger: VoiceTrigger<'a, '_>) {
        let expression = trigger.expression.sanitized();
        let trigger_pitch_bend = pitch_bend_spec().sanitize(expression.stream.pitch_bend);
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
        self.base_resonator_a_config = trigger.patch.resonator_a;
        self.base_resonator_b_config = trigger.patch.resonator_b;
        self.modulation = trigger.modulation;
        self.resonator_modulation_active = modulation_targets_resonators(trigger.modulation);
        self.expression = expression;
        self.velocity = expression.stream.velocity;
        self.aftertouch = expression.stream.pressure;
        self.mod_wheel = expression.mod_wheel;
        self.brightness = expression.stream.brightness;
        self.pitch_bend_semitones.reset(trigger_pitch_bend);
        self.applied_pitch_bend_semitones = trigger_pitch_bend;

        let static_sources = ModulationSources {
            amp_envelope: 1.0,
            secondary_envelope: 0.0,
            lfo: 0.0,
            velocity: expression.stream.velocity,
            aftertouch: expression.stream.pressure,
            mod_wheel: expression.mod_wheel,
            brightness: expression.stream.brightness,
        };
        self.configure_modulated_resonators(
            static_sources,
            trigger.patch.retrigger_resonators,
            true,
        );
        let routing = sanitize_routing(trigger.patch.routing);
        self.routing.reset(routing);
        self.parallel_mix_a.reset(parallel_mix_a(routing));
        self.parallel_mix_b.reset(parallel_mix_b(routing));
        self.output = trigger.patch.output;
        self.amp_envelope = trigger.modulation.amp_envelope.into();
        self.secondary_envelope = trigger.modulation.secondary_envelope.into();
        self.amp_state.reset();
        self.amp_state.note_on();
        self.secondary_state.reset();
        self.secondary_state.note_on();
        self.lfo_phase = 0.0;
        self.lfo_hold = sample_and_hold_value(trigger.midi_note);
        self.output_filter.reset();
        self.output_filter_mode.reset(self.output.filter_mode);
        self.output_filter_cutoff.reset(self.output.filter_cutoff);
        self.output_filter_resonance
            .reset(self.output.filter_resonance);
        self.master_gain
            .reset(output_gain(self.output.master_gain_db));
        self.saturation_drive.reset(self.output.saturation_drive);
        self.master_pan.reset(self.output.master_pan);
        self.applied_waveguide_loop_gain =
            loop_gain_from_configs(self.resonator_a_config, self.resonator_b_config);
        self.waveguide_loop_gain
            .reset(self.applied_waveguide_loop_gain);
        self.series_conditioner.reset(self.sample_rate);
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
        equal_power_pan(sample, self.master_pan.next_sample())
    }

    pub fn is_excitation_finished(&self) -> bool {
        self.excitation.is_finished() && self.amp_state.phase() == EnvelopePhase::Idle
    }

    pub fn note_off(&mut self) {
        self.set_expression(self.expression.with_gate(false));
    }

    pub fn set_pitch_bend(&mut self, pitch_bend_semitones: f32) {
        let mut expression = self.expression;
        expression.stream.pitch_bend = pitch_bend_semitones;
        self.set_expression(expression);
    }

    pub fn set_expression(&mut self, expression: VoiceExpression) {
        let expression = expression.sanitized();
        if self.expression.stream.gate && !expression.stream.gate {
            self.amp_state.note_off();
            self.secondary_state.note_off();
        }
        self.pitch_bend_semitones
            .set_target(expression.stream.pitch_bend);
        self.velocity = expression.stream.velocity;
        self.aftertouch = expression.stream.pressure;
        self.mod_wheel = expression.mod_wheel;
        self.brightness = expression.stream.brightness;
        self.expression = expression;
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        self.output_filter_mode.set_target(output.filter_mode);
        self.output_filter_cutoff.set_target(output.filter_cutoff);
        self.output_filter_resonance
            .set_target(output.filter_resonance);
        self.master_gain
            .set_target(output_gain(output.master_gain_db));
        self.saturation_drive.set_target(output.saturation_drive);
        self.master_pan.set_target(output.master_pan);
        self.output = output;
    }

    pub fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        let loop_gain = finite_clamp(loop_gain, 0.0, 0.999, 0.92);
        if let ResonatorConfig::Waveguide(mut config) = self.base_resonator_a_config {
            config.loop_gain = loop_gain;
            self.base_resonator_a_config = ResonatorConfig::Waveguide(config);
        }
        if let ResonatorConfig::Waveguide(mut config) = self.base_resonator_b_config {
            config.loop_gain = loop_gain;
            self.base_resonator_b_config = ResonatorConfig::Waveguide(config);
        }
        self.waveguide_loop_gain.set_target(loop_gain);
    }

    pub fn set_routing(&mut self, routing: ResonatorRouting) {
        let routing = sanitize_routing(routing);
        if routing_plain(self.routing.current()) != routing_plain(routing) {
            self.routing.set_target(routing);
        } else {
            self.routing.apply_immediate(routing);
        }
        self.parallel_mix_a.set_target(parallel_mix_a(routing));
        self.parallel_mix_b.set_target(parallel_mix_b(routing));
    }

    pub fn clear(&mut self) {
        self.excitation.clear();
        self.excitation_gain = 0.0;
        self.expression = VoiceExpression::default();
        self.velocity = 0.0;
        self.aftertouch = 0.0;
        self.mod_wheel = 0.0;
        self.brightness = 0.0;
        self.resonator_modulation_active = false;
        self.base_resonator_a_config = ResonatorConfig::Modal(ModalConfig::default());
        self.base_resonator_b_config = ResonatorConfig::Waveguide(WaveguideConfig::default());
        self.resonator_a_config = self.base_resonator_a_config;
        self.resonator_b_config = self.base_resonator_b_config;
        self.resonator_a.clear();
        self.resonator_b.clear();
        self.amp_state.reset();
        self.secondary_state.reset();
        self.output_filter.reset();
        self.output_filter_mode.reset(self.output.filter_mode);
        self.output_filter_cutoff.reset(self.output.filter_cutoff);
        self.output_filter_resonance
            .reset(self.output.filter_resonance);
        self.master_gain
            .reset(output_gain(self.output.master_gain_db));
        self.saturation_drive.reset(self.output.saturation_drive);
        self.master_pan.reset(self.output.master_pan);
        self.pitch_bend_semitones.reset(0.0);
        self.applied_pitch_bend_semitones = 0.0;
        self.applied_waveguide_loop_gain =
            loop_gain_from_configs(self.resonator_a_config, self.resonator_b_config);
        self.waveguide_loop_gain
            .reset(self.applied_waveguide_loop_gain);
        let routing = self.routing.current();
        self.parallel_mix_a.reset(parallel_mix_a(routing));
        self.parallel_mix_b.reset(parallel_mix_b(routing));
        self.series_conditioner.reset(self.sample_rate);
    }

    pub fn process_sample(&mut self) -> f32 {
        let structural_gain = self.apply_structural_transitions();
        self.apply_smoothed_pitch_bend();
        self.apply_smoothed_waveguide_loop_gain();

        let sources = self.next_modulation_sources();
        self.apply_live_resonator_modulation(sources);
        let excitation_mod = self.modulation_sum(ModulationDestination::ExcitationGain, sources);
        let excitation = self.excitation.next_sample()
            * self.excitation_gain
            * (1.0 + excitation_mod).clamp(0.0, 2.0);

        let mix_a = self.parallel_mix_a.next_sample();
        let mix_b = self.parallel_mix_b.next_sample();
        let resonator_output = match self.routing.current() {
            ResonatorRouting::Parallel { .. } => {
                let a = self.resonator_a.process_sample(excitation);
                let b = self.resonator_b.process_sample(excitation);
                a * mix_a + b * mix_b
            }
            ResonatorRouting::Series { .. } => {
                let a = self.resonator_a.process_sample(excitation);
                let conditioned = self.series_conditioner.process_sample(a);
                self.resonator_b.process_sample(conditioned)
            }
        };

        let cutoff_mod = self.modulation_sum(ModulationDestination::FilterCutoff, sources);
        let base_cutoff = self.output_filter_cutoff.next_sample();
        let filter_resonance = self.output_filter_resonance.next_sample();
        let filter_cutoff = finite_clamp(
            base_cutoff * 2.0_f32.powf(cutoff_mod * 4.0),
            20.0,
            20_000.0,
            base_cutoff,
        );
        self.output_filter
            .set_coefficients(output_filter_coefficients(
                self.sample_rate,
                filter_cutoff,
                filter_resonance,
                self.output_filter_mode.current(),
            ));
        let filtered = self.output_filter.process(resonator_output);
        let staged = filtered * db_to_gain(INTERNAL_HEADROOM_DB);
        let saturated = soft_saturate(staged, self.saturation_drive.next_sample());
        let amp = sources.amp_envelope;

        snap_to_zero(saturated * amp * self.master_gain.next_sample() * structural_gain)
    }

    fn apply_structural_transitions(&mut self) -> f32 {
        let routing_sample = self.routing.next_sample();
        if routing_sample.change.is_some() {
            self.series_conditioner.reset(self.sample_rate);
        }

        let filter_sample = self.output_filter_mode.next_sample();
        if filter_sample.change.is_some() {
            self.output_filter.reset();
        }

        routing_sample.gain.min(filter_sample.gain)
    }

    fn apply_smoothed_pitch_bend(&mut self) {
        let pitch_bend = self.pitch_bend_semitones.next_sample();
        if (pitch_bend - self.applied_pitch_bend_semitones).abs() <= PITCH_BEND_EPSILON {
            return;
        }

        self.applied_pitch_bend_semitones = pitch_bend;
        let base_frequency = midi_note_to_hz(self.midi_note as f32 + pitch_bend);
        self.resonator_a
            .retune(&self.resonator_a_config, base_frequency);
        self.resonator_b
            .retune(&self.resonator_b_config, base_frequency);
    }

    fn apply_smoothed_waveguide_loop_gain(&mut self) {
        let loop_gain = self.waveguide_loop_gain.next_sample();
        if (loop_gain - self.applied_waveguide_loop_gain).abs() <= PARAMETER_EPSILON {
            return;
        }

        self.applied_waveguide_loop_gain = loop_gain;
        self.resonator_a.set_waveguide_loop_gain(loop_gain);
        self.resonator_b.set_waveguide_loop_gain(loop_gain);
    }

    fn apply_live_resonator_modulation(&mut self, sources: ModulationSources) {
        if self.resonator_modulation_active {
            self.configure_modulated_resonators(sources, false, false);
        }
    }

    fn configure_modulated_resonators(
        &mut self,
        sources: ModulationSources,
        reset_state: bool,
        force: bool,
    ) {
        let resonator_a_config = modulated_resonator_config(
            self.base_resonator_a_config,
            self.modulation_sum(ModulationDestination::ResonatorADamping, sources),
            self.modulation_sum(ModulationDestination::ResonatorAPosition, sources),
        );
        let resonator_b_config = modulated_resonator_config(
            self.base_resonator_b_config,
            self.modulation_sum(ModulationDestination::ResonatorBDamping, sources),
            self.modulation_sum(ModulationDestination::ResonatorBPosition, sources),
        );
        let base_frequency =
            midi_note_to_hz(self.midi_note as f32 + self.applied_pitch_bend_semitones);

        if force || resonator_a_config != self.resonator_a_config {
            self.resonator_a
                .configure(&resonator_a_config, base_frequency, reset_state);
            self.resonator_a_config = resonator_a_config;
        }

        if force || resonator_b_config != self.resonator_b_config {
            self.resonator_b
                .configure(&resonator_b_config, base_frequency, reset_state);
            self.resonator_b_config = resonator_b_config;
        }
    }

    fn next_modulation_sources(&mut self) -> ModulationSources {
        self.apply_expression_values();
        let amp_envelope = self
            .amp_state
            .next_sample(self.amp_envelope, self.sample_rate);
        let secondary_envelope = self
            .secondary_state
            .next_sample(self.secondary_envelope, self.sample_rate);
        let lfo = self.next_lfo_sample();

        ModulationSources {
            amp_envelope,
            secondary_envelope,
            lfo,
            velocity: self.velocity,
            aftertouch: self.aftertouch,
            mod_wheel: self.mod_wheel,
            brightness: self.brightness,
        }
    }

    fn next_lfo_sample(&mut self) -> f32 {
        self.apply_expression_values();
        let lfo_rate_mod = self.modulation_sum(
            ModulationDestination::LfoRate,
            ModulationSources {
                amp_envelope: self.amp_state.value(),
                secondary_envelope: self.secondary_state.value(),
                lfo: 0.0,
                velocity: self.velocity,
                aftertouch: self.aftertouch,
                mod_wheel: self.mod_wheel,
                brightness: self.brightness,
            },
        );
        let rate_hz = (self.modulation.lfo.rate_hz * (1.0 + lfo_rate_mod).clamp(0.01, 16.0))
            .clamp(0.01, 100.0);
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();

        match self.modulation.lfo.shape {
            crate::LfoShape::Sine => (std::f32::consts::TAU * self.lfo_phase).sin(),
            crate::LfoShape::Triangle => 4.0 * (self.lfo_phase - 0.5).abs() - 1.0,
            crate::LfoShape::Saw => self.lfo_phase * 2.0 - 1.0,
            crate::LfoShape::Square => {
                if self.lfo_phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            crate::LfoShape::SampleAndHold => self.lfo_hold,
        }
    }

    fn modulation_sum(
        &self,
        destination: ModulationDestination,
        sources: ModulationSources,
    ) -> f32 {
        modulation_sum_from(self.modulation, destination, sources)
    }

    fn apply_expression_values(&mut self) {
        let expression = self.expression.sanitized();
        self.velocity = expression.stream.velocity;
        self.aftertouch = expression.stream.pressure;
        self.mod_wheel = expression.mod_wheel;
        self.brightness = expression.stream.brightness;
        self.expression = expression;
    }
}

#[derive(Debug, Clone, Copy)]
struct ModulationSources {
    amp_envelope: f32,
    secondary_envelope: f32,
    lfo: f32,
    velocity: f32,
    aftertouch: f32,
    mod_wheel: f32,
    brightness: f32,
}

fn source_value(source: ModulationSource, values: ModulationSources) -> f32 {
    match source {
        ModulationSource::SecondaryEnvelope => values.secondary_envelope,
        ModulationSource::Lfo => values.lfo,
        ModulationSource::Velocity => values.velocity,
        ModulationSource::Aftertouch => values.aftertouch,
        ModulationSource::ModWheel => values.mod_wheel,
        ModulationSource::Brightness => values.brightness,
    }
}

fn sample_and_hold_value(seed: u8) -> f32 {
    let mut value = u32::from(seed)
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223);
    value ^= value >> 16;
    (value as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn modulation_sum_from(
    modulation: ModulationConfig,
    destination: ModulationDestination,
    sources: ModulationSources,
) -> f32 {
    modulation
        .slots
        .iter()
        .filter(|slot| slot.enabled && slot.destination == destination)
        .map(|slot| source_value(slot.source, sources) * slot.amount)
        .sum::<f32>()
        .clamp(-1.0, 1.0)
}

fn modulation_targets_resonators(modulation: ModulationConfig) -> bool {
    modulation.slots.iter().any(|slot| {
        slot.enabled
            && matches!(
                slot.destination,
                ModulationDestination::ResonatorADamping
                    | ModulationDestination::ResonatorBDamping
                    | ModulationDestination::ResonatorAPosition
                    | ModulationDestination::ResonatorBPosition
            )
    })
}

fn velocity_to_gain(velocity: f32, depth: f32) -> f32 {
    let velocity = velocity.clamp(0.0, 1.0);
    let depth = depth.clamp(0.0, 1.0);
    (1.0 - depth) + velocity * depth
}

fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn sanitize_output_filter_cutoff(cutoff_hz: f32) -> f32 {
    finite_clamp(cutoff_hz, 20.0, 20_000.0, 20_000.0)
}

fn output_gain(gain_db: f32) -> f32 {
    db_to_gain(finite_clamp(gain_db, -60.0, 12.0, 0.0))
}

fn output_filter_cutoff_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(
        20.0,
        20_000.0,
        20_000.0,
        PARAMETER_SMOOTH_MS,
        FILTER_CUTOFF_EPSILON,
    )
}

fn output_filter_resonance_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(0.0, 0.999, 0.0, PARAMETER_SMOOTH_MS, PARAMETER_EPSILON)
}

fn master_gain_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(
        db_to_gain(-60.0),
        db_to_gain(12.0),
        1.0,
        PARAMETER_SMOOTH_MS,
        PARAMETER_EPSILON,
    )
}

fn saturation_drive_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(0.0, 1.0, 0.0, PARAMETER_SMOOTH_MS, PARAMETER_EPSILON)
}

fn master_pan_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(-1.0, 1.0, 0.0, PARAMETER_SMOOTH_MS, PARAMETER_EPSILON)
}

fn parallel_mix_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(0.0, 1.0, 0.5, PARAMETER_SMOOTH_MS, PARALLEL_MIX_EPSILON)
}

fn pitch_bend_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(-96.0, 96.0, 0.0, PITCH_BEND_SMOOTH_MS, PITCH_BEND_EPSILON)
}

fn waveguide_loop_gain_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(0.0, 0.999, 0.92, PARAMETER_SMOOTH_MS, PARAMETER_EPSILON)
}

fn output_filter_cutoff_param(sample_rate: f32, cutoff_hz: f32) -> SmoothedParam {
    SmoothedParam::with_initial(output_filter_cutoff_spec(), sample_rate, cutoff_hz)
}

fn output_filter_resonance_param(sample_rate: f32, resonance: f32) -> SmoothedParam {
    SmoothedParam::with_initial(output_filter_resonance_spec(), sample_rate, resonance)
}

fn master_gain_param(sample_rate: f32, gain_db: f32) -> SmoothedParam {
    SmoothedParam::with_initial(master_gain_spec(), sample_rate, output_gain(gain_db))
}

fn saturation_drive_param(sample_rate: f32, drive: f32) -> SmoothedParam {
    SmoothedParam::with_initial(saturation_drive_spec(), sample_rate, drive)
}

fn master_pan_param(sample_rate: f32, pan: f32) -> SmoothedParam {
    SmoothedParam::with_initial(master_pan_spec(), sample_rate, pan)
}

fn parallel_mix_param(sample_rate: f32, mix: f32) -> SmoothedParam {
    SmoothedParam::with_initial(parallel_mix_spec(), sample_rate, mix)
}

fn pitch_bend_param(sample_rate: f32, semitones: f32) -> SmoothedParam {
    SmoothedParam::with_initial(pitch_bend_spec(), sample_rate, semitones)
}

fn waveguide_loop_gain_param(sample_rate: f32, loop_gain: f32) -> SmoothedParam {
    SmoothedParam::with_initial(waveguide_loop_gain_spec(), sample_rate, loop_gain)
}

fn structural_ramp_samples(sample_rate: f32) -> usize {
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    (sample_rate * STRUCTURAL_RAMP_MS * 0.001)
        .round()
        .clamp(8.0, 256.0) as usize
}

fn loop_gain_from_configs(resonator_a: ResonatorConfig, resonator_b: ResonatorConfig) -> f32 {
    match (resonator_a, resonator_b) {
        (ResonatorConfig::Waveguide(config), _) => finite_clamp(config.loop_gain, 0.0, 0.999, 0.92),
        (_, ResonatorConfig::Waveguide(config)) => finite_clamp(config.loop_gain, 0.0, 0.999, 0.92),
        _ => 0.92,
    }
}

fn output_filter_coefficients(
    sample_rate: f32,
    cutoff_hz: f32,
    resonance: f32,
    mode: FilterMode,
) -> BiquadCoefficients {
    let cutoff_hz = sanitize_output_filter_cutoff(cutoff_hz);
    let q = 0.707 + finite_clamp(resonance, 0.0, 0.999, 0.0) * 8.0;
    match mode {
        FilterMode::LowPass => BiquadCoefficients::lowpass(sample_rate, cutoff_hz, q),
        FilterMode::BandPass => BiquadCoefficients::bandpass(sample_rate, cutoff_hz, q),
        FilterMode::HighPass => BiquadCoefficients::highpass(sample_rate, cutoff_hz, q),
    }
}

fn modulated_resonator_config(
    config: ResonatorConfig,
    damping_mod: f32,
    position_mod: f32,
) -> ResonatorConfig {
    match config {
        ResonatorConfig::Modal(mut config) => {
            config.decay_global =
                (config.decay_global * 2.0_f32.powf(damping_mod * 2.0)).clamp(0.01, 10.0);
            config.position_of_strike =
                (config.position_of_strike + position_mod * 0.5).clamp(0.001, 0.999);
            ResonatorConfig::Modal(config)
        }
        ResonatorConfig::Waveguide(mut config) => {
            config.loop_gain = (config.loop_gain + damping_mod * 0.25).clamp(0.0, 0.999);
            config.position_of_strike =
                (config.position_of_strike + position_mod * 0.5).clamp(0.001, 0.999);
            ResonatorConfig::Waveguide(config)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorKind {
    Silent,
    Modal,
    Waveguide,
}

#[derive(Debug)]
struct ResonatorEngine {
    kind: ResonatorKind,
    modal: ModalBank,
    waveguide: WaveguideResonator,
    waveguide_params: WaveguideParams,
}

impl ResonatorEngine {
    fn new(sample_rate: f32) -> Self {
        Self {
            kind: ResonatorKind::Silent,
            modal: ModalBank::with_capacity(sample_rate, 256, ModalBankParams::default()),
            waveguide: WaveguideResonator::new(sample_rate, 20.0),
            waveguide_params: WaveguideParams::default(),
        }
    }

    fn configure(&mut self, config: &ResonatorConfig, base_frequency: f32, reset_state: bool) {
        if !reset_state && self.try_configure_preserving_state(config, base_frequency) {
            return;
        }

        match config {
            ResonatorConfig::Modal(config) => {
                self.kind = ResonatorKind::Modal;
                self.modal
                    .configure(modal_params_from_config(config, base_frequency));
                self.modal.reset();
            }
            ResonatorConfig::Waveguide(config) => {
                self.kind = ResonatorKind::Waveguide;
                self.waveguide_params = waveguide_params_from_config(config, base_frequency);
                self.waveguide.reset();
            }
        }
    }

    fn try_configure_preserving_state(
        &mut self,
        config: &ResonatorConfig,
        base_frequency: f32,
    ) -> bool {
        match (self.kind, config) {
            (ResonatorKind::Modal, ResonatorConfig::Modal(config)) => {
                let params = modal_params_from_config(config, base_frequency);
                if self.modal.modes().len() == params.mode_count {
                    self.modal.retune(params);
                    true
                } else {
                    false
                }
            }
            (ResonatorKind::Waveguide, ResonatorConfig::Waveguide(config)) => {
                self.waveguide_params = waveguide_params_from_config(config, base_frequency);
                true
            }
            _ => false,
        }
    }

    fn retune(&mut self, config: &ResonatorConfig, base_frequency: f32) {
        match (self.kind, config) {
            (ResonatorKind::Modal, ResonatorConfig::Modal(config)) => {
                self.modal
                    .retune(modal_params_from_config(config, base_frequency));
            }
            (ResonatorKind::Waveguide, ResonatorConfig::Waveguide(config)) => {
                self.waveguide_params = waveguide_params_from_config(config, base_frequency);
            }
            _ => self.configure(config, base_frequency, true),
        }
    }

    fn clear(&mut self) {
        self.kind = ResonatorKind::Silent;
        self.modal.reset();
        self.waveguide.reset();
    }

    fn process_sample(&mut self, input: f32) -> f32 {
        match self.kind {
            ResonatorKind::Silent => 0.0,
            ResonatorKind::Modal => self.modal.process_sample(input),
            ResonatorKind::Waveguide => self.waveguide.process_sample(input, self.waveguide_params),
        }
    }

    fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        if self.kind == ResonatorKind::Waveguide {
            self.waveguide_params.loop_gain = loop_gain.clamp(0.0, 0.999);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SeriesConditioner {
    highpass: Biquad,
    fast_env: f32,
    slow_env: f32,
}

impl SeriesConditioner {
    fn new(sample_rate: f32) -> Self {
        Self {
            highpass: Biquad::new(BiquadCoefficients::highpass(sample_rate, 80.0, 0.707)),
            fast_env: 0.0,
            slow_env: 0.0,
        }
    }

    fn reset(&mut self, sample_rate: f32) {
        self.highpass
            .set_coefficients(BiquadCoefficients::highpass(sample_rate, 80.0, 0.707));
        self.highpass.reset();
        self.fast_env = 0.0;
        self.slow_env = 0.0;
    }

    fn process_sample(&mut self, input: f32) -> f32 {
        let highpassed = self.highpass.process(input);
        let magnitude = highpassed.abs();

        self.fast_env += 0.01 * (magnitude - self.fast_env);
        self.slow_env += 0.000_2 * (magnitude - self.slow_env);

        let transient_bias =
            ((self.fast_env - self.slow_env) / (self.fast_env + 1.0e-6)).clamp(0.0, 1.0);
        highpassed * (0.04 + transient_bias * 0.96)
    }
}

fn modal_params_from_config(config: &ModalConfig, base_frequency: f32) -> ModalBankParams {
    ModalBankParams {
        fundamental_hz: tuned_frequency(base_frequency, config.semitone_offset, config.cent_offset),
        mode_count: config.mode_count as usize,
        preset: config.preset,
        inharmonicity: config.inharmonicity,
        brightness: config.brightness,
        decay_global: config.decay_global,
        decay_tilt: config.decay_tilt,
        position_of_strike: config.position_of_strike,
    }
}

fn waveguide_params_from_config(config: &WaveguideConfig, base_frequency: f32) -> WaveguideParams {
    WaveguideParams {
        style: config.style,
        frequency_hz: tuned_frequency(base_frequency, config.semitone_offset, config.cent_offset),
        loop_filter_cutoff: config.loop_filter_cutoff,
        loop_filter_resonance: config.loop_filter_resonance,
        loop_gain: config.loop_gain,
        loop_nonlinearity: config.loop_nonlinearity,
        position_of_strike: config.position_of_strike,
        boundary_reflection: config.boundary_reflection,
    }
}

fn tuned_frequency(base_frequency: f32, semitone_offset: i8, cent_offset: f32) -> f32 {
    base_frequency * semitones_to_ratio(semitone_offset as f32 + cent_offset / 100.0)
}

fn sanitize_routing(routing: ResonatorRouting) -> ResonatorRouting {
    match routing {
        ResonatorRouting::Parallel { mix_a, mix_b } => ResonatorRouting::Parallel {
            mix_a: finite_clamp(mix_a, 0.0, 1.0, 0.5),
            mix_b: finite_clamp(mix_b, 0.0, 1.0, 0.5),
        },
        ResonatorRouting::Series { mix_a, mix_b } => ResonatorRouting::Series {
            mix_a: finite_clamp(mix_a, 0.0, 1.0, 0.5),
            mix_b: finite_clamp(mix_b, 0.0, 1.0, 0.5),
        },
    }
}

fn routing_plain(routing: ResonatorRouting) -> u8 {
    match routing {
        ResonatorRouting::Parallel { .. } => 0,
        ResonatorRouting::Series { .. } => 1,
    }
}

fn parallel_mix_a(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_a, .. } => mix_a,
        ResonatorRouting::Series { mix_a, .. } => mix_a,
    }
}

fn parallel_mix_b(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_b, .. } => mix_b,
        ResonatorRouting::Series { mix_b, .. } => mix_b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{ExcitationLayer, SelectedExcitations};
    use crate::{ModalPreset, WaveguideConfig};
    use ahara_dsp_utils::analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms};

    #[test]
    fn parallel_voice_renders_nonzero_stereo_output() {
        let sample_rate = 48_000.0;
        let patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 0.75,
            mix_b: 0.25,
        });
        let excitation = impulse(256);
        let mut voice = Voice::new(sample_rate);
        let mut left = vec![0.0; 8_192];
        let mut right = vec![0.0; 8_192];

        voice.trigger(VoiceTrigger::new(57, 1.0, &excitation, sample_rate, &patch));
        voice.render_add(&mut left, &mut right);

        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(rms(&left) > 0.000_1);
        assert!(rms(&right) > 0.000_1);
    }

    #[test]
    fn layered_excitation_trigger_renders_louder_than_single_layer() {
        let sample_rate = 48_000.0;
        let patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        });
        let excitation_a = impulse(64);
        let excitation_b = impulse(64);
        let mut single_selected = SelectedExcitations::default();
        let mut selected = SelectedExcitations::default();
        single_selected.push(ExcitationLayer {
            gain: 0.1,
            ..ExcitationLayer::new(&excitation_a, sample_rate)
        });
        selected.push(ExcitationLayer {
            gain: 0.1,
            ..ExcitationLayer::new(&excitation_a, sample_rate)
        });
        selected.push(ExcitationLayer {
            gain: 0.1,
            ..ExcitationLayer::new(&excitation_b, sample_rate)
        });

        let mut single_voice = Voice::new(sample_rate);
        let mut single_left = vec![0.0; 8_192];
        let mut single_right = vec![0.0; 8_192];
        let mut layered_voice = Voice::new(sample_rate);
        let mut layered_left = vec![0.0; 8_192];
        let mut layered_right = vec![0.0; 8_192];
        single_voice.trigger(VoiceTrigger::with_excitations(
            60,
            1.0,
            single_selected,
            &patch,
        ));
        layered_voice.trigger(VoiceTrigger::with_excitations(60, 1.0, selected, &patch));
        single_voice.render_add(&mut single_left, &mut single_right);
        layered_voice.render_add(&mut layered_left, &mut layered_right);

        assert!(rms(&layered_left) > rms(&single_left) * 1.8);
    }

    #[test]
    fn master_pan_moves_signal_to_right_channel() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        });
        patch.output.master_pan = 1.0;
        let excitation = impulse(64);
        let mut voice = Voice::new(sample_rate);
        let mut left = vec![0.0; 4_096];
        let mut right = vec![0.0; 4_096];

        voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));
        voice.render_add(&mut left, &mut right);

        assert!(rms(&right) > 0.000_1);
        assert!(rms(&left) < rms(&right) * 0.001);
    }

    #[test]
    fn master_gain_changes_render_level() {
        let sample_rate = 48_000.0;
        let excitation = impulse(64);
        let mut unity_patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        });
        let mut quiet_patch = unity_patch.clone();
        unity_patch.output.master_gain_db = 0.0;
        quiet_patch.output.master_gain_db = -12.0;

        let unity = render_mono_energy(sample_rate, &unity_patch, &excitation);
        let quiet = render_mono_energy(sample_rate, &quiet_patch, &excitation);

        assert!(quiet < unity * 0.35, "quiet={quiet}, unity={unity}");
        assert!(quiet > unity * 0.20, "quiet={quiet}, unity={unity}");
    }

    #[test]
    fn output_lowpass_reduces_high_frequency_content() {
        let sample_rate = 48_000.0;
        let excitation = impulse(64);
        let mut open_patch = bright_modal_patch();
        let mut dark_patch = open_patch.clone();
        open_patch.output.filter_cutoff = 20_000.0;
        dark_patch.output.filter_cutoff = 800.0;

        let open = render_left(sample_rate, &open_patch, &excitation);
        let dark = render_left(sample_rate, &dark_patch, &excitation);

        assert!(
            dft_magnitude_at(&dark[512..], sample_rate, 6_000.0)
                < dft_magnitude_at(&open[512..], sample_rate, 6_000.0) * 0.6
        );
    }

    #[test]
    fn series_voice_stays_finite_and_bounded() {
        let sample_rate = 48_000.0;
        let mut patch = test_patch(ResonatorRouting::Series {
            mix_a: 0.5,
            mix_b: 0.5,
        });
        patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.995,
            loop_filter_cutoff: 18_000.0,
            loop_nonlinearity: 0.4,
            ..WaveguideConfig::default()
        });
        let excitation = impulse(256);
        let rendered = render_left(sample_rate, &patch, &excitation);

        assert_all_finite(&rendered);
        assert!(peak_abs(&rendered) < 4.0);
        assert!(rms(&rendered) > 0.000_001);
    }

    #[test]
    fn series_conditioner_deemphasizes_steady_state_after_onset() {
        let sample_rate = 48_000.0;
        let mut conditioner = SeriesConditioner::new(sample_rate);
        let mut output = Vec::with_capacity(24_000);

        for index in 0..24_000 {
            let input = (std::f32::consts::TAU * 220.0 * index as f32 / sample_rate).sin();
            output.push(conditioner.process_sample(input));
        }

        let onset_rms = rms(&output[64..1_088]);
        let steady_rms = rms(&output[20_000..23_000]);
        assert!(
            steady_rms < onset_rms * 0.35,
            "onset_rms={onset_rms}, steady_rms={steady_rms}"
        );
    }

    #[test]
    fn retrigger_off_preserves_resonator_state_for_reused_voice() {
        let sample_rate = 48_000.0;
        let preserved = render_silent_retrigger_after_impulse(sample_rate, false);
        let reset = render_silent_retrigger_after_impulse(sample_rate, true);

        assert!(preserved > 0.000_01, "preserved={preserved}");
        assert!(
            reset < preserved * 0.05,
            "reset={reset}, preserved={preserved}"
        );
    }

    #[test]
    fn held_voice_accepts_live_routing_changes() {
        let sample_rate = 48_000.0;
        let patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 0.8,
            mix_b: 0.2,
        });
        let excitation = impulse(64);
        let mut voice = Voice::new(sample_rate);

        voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));
        assert_voice_routing_kind(&voice, 0);
        assert_eq!(voice.routing.policy(), StructuralChangePolicy::LiveMuteRamp);

        voice.set_routing(ResonatorRouting::Series {
            mix_a: 0.8,
            mix_b: 0.2,
        });
        assert_voice_routing_kind(&voice, 0);
        assert!(voice.routing.has_pending());
        drain_structural_transitions(&mut voice);
        assert_voice_routing_kind(&voice, 1);

        voice.set_routing(ResonatorRouting::Parallel {
            mix_a: 0.1,
            mix_b: 0.9,
        });
        assert_voice_routing_kind(&voice, 1);
        drain_structural_transitions(&mut voice);
        assert_voice_routing_kind(&voice, 0);
        assert_parallel_mix_targets(&voice, 0.1, 0.9);
        assert_parallel_mix_is_smoothing(&voice);
    }

    #[test]
    fn filter_mode_is_structural_and_applies_with_mute_ramp() {
        let sample_rate = 48_000.0;
        let excitation = impulse(64);
        let patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        });
        let mut voice = Voice::new(sample_rate);
        voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));

        let mut output = patch.output;
        output.filter_mode = FilterMode::HighPass;
        voice.set_output_config(output);

        assert_eq!(
            voice.output_filter_mode.policy(),
            StructuralChangePolicy::LiveMuteRamp
        );
        assert_eq!(voice.output_filter_mode.current(), FilterMode::LowPass);
        assert!(voice.output_filter_mode.has_pending());
        drain_structural_transitions(&mut voice);
        assert_eq!(voice.output_filter_mode.current(), FilterMode::HighPass);
        assert!(!voice.output_filter_mode.has_pending());
    }

    #[test]
    fn live_parameter_changes_are_smoothed_per_sample() {
        let sample_rate = 48_000.0;
        let excitation = impulse(64);
        let mut patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 0.0,
            mix_b: 1.0,
        });
        patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.92,
            ..WaveguideConfig::default()
        });
        let mut voice = Voice::new(sample_rate);
        voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));

        let mut output = patch.output;
        output.master_gain_db = -60.0;
        output.saturation_drive = 1.0;
        output.master_pan = 1.0;
        output.filter_cutoff = 200.0;
        output.filter_resonance = 0.8;
        voice.set_output_config(output);
        voice.set_routing(ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        });
        voice.set_waveguide_loop_gain(0.1);
        voice.set_pitch_bend(2.0);

        assert_output_params_are_smoothing(&voice);
        assert_parallel_mix_is_smoothing(&voice);
        assert_resonator_controls_are_smoothing(&voice);
        assert_master_gain_is_ramping_down(&mut voice);
    }

    fn assert_voice_routing_kind(voice: &Voice<'_>, expected: u8) {
        assert_eq!(routing_plain(voice.routing.current()), expected);
    }

    fn assert_parallel_mix_targets(voice: &Voice<'_>, mix_a: f32, mix_b: f32) {
        assert_eq!(voice.parallel_mix_a.target(), mix_a);
        assert_eq!(voice.parallel_mix_b.target(), mix_b);
    }

    fn assert_parallel_mix_is_smoothing(voice: &Voice<'_>) {
        assert!(voice.parallel_mix_a.is_smoothing());
        assert!(voice.parallel_mix_b.is_smoothing());
    }

    fn assert_output_params_are_smoothing(voice: &Voice<'_>) {
        assert!(voice.master_gain.is_smoothing());
        assert!(voice.saturation_drive.is_smoothing());
        assert!(voice.master_pan.is_smoothing());
        assert!(voice.output_filter_cutoff.is_smoothing());
        assert!(voice.output_filter_resonance.is_smoothing());
    }

    fn assert_resonator_controls_are_smoothing(voice: &Voice<'_>) {
        assert!(voice.waveguide_loop_gain.is_smoothing());
        assert!(voice.pitch_bend_semitones.is_smoothing());
    }

    fn assert_master_gain_is_ramping_down(voice: &mut Voice<'_>) {
        let first_gain = voice.master_gain.next_sample();
        assert!(first_gain < output_gain(0.0));
        assert!(first_gain > output_gain(-60.0));
    }

    fn drain_structural_transitions(voice: &mut Voice<'_>) {
        for _ in 0..(structural_ramp_samples(voice.sample_rate) * 2 + 1) {
            voice.apply_structural_transitions();
        }
    }

    fn render_mono_energy(
        sample_rate: f32,
        patch: &ResonatorSynthPatch,
        excitation: &[f32],
    ) -> f32 {
        let left = render_left(sample_rate, patch, excitation);
        rms(&left)
    }

    fn render_left(sample_rate: f32, patch: &ResonatorSynthPatch, excitation: &[f32]) -> Vec<f32> {
        let mut voice = Voice::new(sample_rate);
        let mut left = vec![0.0; 8_192];
        let mut right = vec![0.0; 8_192];
        voice.trigger(VoiceTrigger::new(60, 1.0, excitation, sample_rate, patch));
        voice.render_add(&mut left, &mut right);
        left
    }

    fn render_silent_retrigger_after_impulse(sample_rate: f32, retrigger_resonators: bool) -> f32 {
        let mut patch = test_patch(ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        });
        patch.retrigger_resonators = retrigger_resonators;
        patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.985,
            loop_filter_cutoff: 12_000.0,
            ..WaveguideConfig::default()
        });
        let excitation = impulse(64);
        let mut voice = Voice::new(sample_rate);
        voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));

        for _ in 0..512 {
            voice.process_sample();
        }

        voice.trigger(VoiceTrigger::with_excitations(
            60,
            1.0,
            SelectedExcitations::default(),
            &patch,
        ));

        let mut output = Vec::with_capacity(1024);
        for _ in 0..1024 {
            output.push(voice.process_sample());
        }

        rms(&output)
    }

    fn impulse(len: usize) -> Vec<f32> {
        let mut excitation = vec![0.0; len];
        excitation[0] = 1.0;
        excitation
    }

    fn test_patch(routing: ResonatorRouting) -> ResonatorSynthPatch {
        ResonatorSynthPatch {
            resonator_a: ResonatorConfig::Modal(ModalConfig {
                mode_count: 24,
                preset: ModalPreset::GenericStrike,
                decay_global: 0.8,
                brightness: 0.6,
                ..ModalConfig::default()
            }),
            resonator_b: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.94,
                loop_filter_cutoff: 10_000.0,
                ..WaveguideConfig::default()
            }),
            routing,
            output: OutputConfig {
                filter_cutoff: 20_000.0,
                filter_resonance: 0.0,
                saturation_drive: 0.0,
                master_gain_db: 0.0,
                master_pan: 0.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        }
    }

    fn bright_modal_patch() -> ResonatorSynthPatch {
        ResonatorSynthPatch {
            resonator_a: ResonatorConfig::Modal(ModalConfig {
                mode_count: 64,
                preset: ModalPreset::GenericStrike,
                decay_global: 1.0,
                decay_tilt: 0.0,
                brightness: 1.0,
                ..ModalConfig::default()
            }),
            resonator_b: ResonatorConfig::Modal(ModalConfig {
                mode_count: 1,
                ..ModalConfig::default()
            }),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            output: OutputConfig {
                filter_cutoff: 20_000.0,
                filter_resonance: 0.0,
                saturation_drive: 0.0,
                master_gain_db: 0.0,
                master_pan: 0.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        }
    }
}
