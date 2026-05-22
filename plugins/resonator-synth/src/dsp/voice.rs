use ahara_dsp_utils::{
    db_to_gain,
    envelope::{Adsr, AdsrState, EnvelopePhase},
    equal_power_pan,
    filters::{Biquad, BiquadCoefficients, Svf, SvfMode},
    math::{midi_note_to_hz, semitones_to_ratio, snap_to_zero},
    soft_saturate,
};

use super::{
    excitation::{SelectedExcitations, VoiceExcitation},
    modal::{ModalBank, ModalBankParams},
    waveguide::{WaveguideParams, WaveguideResonator},
};
use crate::{
    FilterMode, ModalConfig, ModulationConfig, ModulationDestination, ModulationSource,
    OutputConfig, ResonatorConfig, ResonatorRouting, ResonatorSynthPatch, WaveguideConfig,
};

#[derive(Debug, Clone, Copy)]
pub struct VoiceTrigger<'a, 'p> {
    pub midi_note: u8,
    pub pitch_bend_semitones: f32,
    pub velocity: f32,
    pub aftertouch: f32,
    pub mod_wheel: f32,
    pub brightness: f32,
    pub modulation: ModulationConfig,
    pub excitations: SelectedExcitations<'a>,
    pub patch: &'p ResonatorSynthPatch,
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
            midi_note,
            pitch_bend_semitones: 0.0,
            velocity,
            aftertouch: 0.0,
            mod_wheel: 0.0,
            brightness: 0.0,
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
            midi_note,
            pitch_bend_semitones: 0.0,
            velocity,
            aftertouch: 0.0,
            mod_wheel: 0.0,
            brightness: 0.0,
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
    routing: ResonatorRouting,
    output: OutputConfig,
    modulation: ModulationConfig,
    midi_note: u8,
    resonator_a_config: ResonatorConfig,
    resonator_b_config: ResonatorConfig,
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
    output_filter: Svf,
    series_conditioner: SeriesConditioner,
}

impl<'a> Voice<'a> {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            excitation: VoiceExcitation::default(),
            excitation_gain: 0.0,
            resonator_a: ResonatorEngine::new(sample_rate),
            resonator_b: ResonatorEngine::new(sample_rate),
            routing: ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
            output: OutputConfig::default(),
            modulation: ModulationConfig::default(),
            midi_note: 60,
            resonator_a_config: ResonatorConfig::Modal(ModalConfig::default()),
            resonator_b_config: ResonatorConfig::Waveguide(WaveguideConfig::default()),
            velocity: 0.0,
            aftertouch: 0.0,
            mod_wheel: 0.0,
            brightness: 0.0,
            amp_envelope: ModulationConfig::default().amp_envelope.into(),
            amp_state: AdsrState::default(),
            secondary_envelope: ModulationConfig::default().secondary_envelope.into(),
            secondary_state: AdsrState::default(),
            lfo_phase: 0.0,
            lfo_hold: 0.0,
            output_filter: Svf::new(sample_rate),
            series_conditioner: SeriesConditioner::new(sample_rate),
        }
    }

    pub fn trigger(&mut self, trigger: VoiceTrigger<'a, '_>) {
        let base_frequency =
            midi_note_to_hz(trigger.midi_note as f32 + trigger.pitch_bend_semitones);
        let excitation_pitch_ratio =
            semitones_to_ratio(trigger.midi_note as f32 - 60.0 + trigger.pitch_bend_semitones);

        self.excitation.trigger(
            trigger.excitations,
            self.sample_rate,
            excitation_pitch_ratio,
        );
        self.excitation_gain = velocity_to_gain(
            trigger.velocity,
            trigger.modulation.velocity_to_excitation_depth,
        );
        let static_sources = ModulationSources {
            amp_envelope: 1.0,
            secondary_envelope: 0.0,
            lfo: 0.0,
            velocity: trigger.velocity.clamp(0.0, 1.0),
            aftertouch: trigger.aftertouch.clamp(0.0, 1.0),
            mod_wheel: trigger.mod_wheel.clamp(0.0, 1.0),
        };
        let resonator_a = modulated_resonator_config(
            trigger.patch.resonator_a,
            modulation_sum_from(
                trigger.modulation,
                ModulationDestination::ResonatorADamping,
                static_sources,
            ),
            modulation_sum_from(
                trigger.modulation,
                ModulationDestination::ResonatorAPosition,
                static_sources,
            ),
        );
        let resonator_b = modulated_resonator_config(
            trigger.patch.resonator_b,
            modulation_sum_from(
                trigger.modulation,
                ModulationDestination::ResonatorBDamping,
                static_sources,
            ),
            modulation_sum_from(
                trigger.modulation,
                ModulationDestination::ResonatorBPosition,
                static_sources,
            ),
        );
        self.resonator_a.configure(&resonator_a, base_frequency);
        self.resonator_b.configure(&resonator_b, base_frequency);
        self.midi_note = trigger.midi_note;
        self.resonator_a_config = resonator_a;
        self.resonator_b_config = resonator_b;
        self.routing = trigger.patch.routing;
        self.output = trigger.patch.output;
        self.modulation = trigger.modulation;
        self.velocity = trigger.velocity.clamp(0.0, 1.0);
        self.aftertouch = trigger.aftertouch.clamp(0.0, 1.0);
        self.mod_wheel = trigger.mod_wheel.clamp(0.0, 1.0);
        self.brightness = trigger.brightness.clamp(0.0, 1.0);
        self.amp_envelope = trigger.modulation.amp_envelope.into();
        self.secondary_envelope = trigger.modulation.secondary_envelope.into();
        self.amp_state.reset();
        self.amp_state.note_on();
        self.secondary_state.reset();
        self.secondary_state.note_on();
        self.lfo_phase = 0.0;
        self.lfo_hold = sample_and_hold_value(trigger.midi_note);
        self.output_filter.reset();
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
        equal_power_pan(sample, self.output.master_pan)
    }

    pub fn is_excitation_finished(&self) -> bool {
        self.excitation.is_finished() && self.amp_state.phase() == EnvelopePhase::Idle
    }

    pub fn note_off(&mut self) {
        self.amp_state.note_off();
        self.secondary_state.note_off();
    }

    pub fn set_pitch_bend(&mut self, pitch_bend_semitones: f32) {
        let base_frequency = midi_note_to_hz(self.midi_note as f32 + pitch_bend_semitones);
        self.resonator_a
            .configure(&self.resonator_a_config, base_frequency);
        self.resonator_b
            .configure(&self.resonator_b_config, base_frequency);
    }

    pub fn set_output_config(&mut self, output: OutputConfig) {
        self.output = output;
    }

    pub fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        let loop_gain = loop_gain.clamp(0.0, 0.999);
        if let ResonatorConfig::Waveguide(mut config) = self.resonator_a_config {
            config.loop_gain = loop_gain;
            self.resonator_a_config = ResonatorConfig::Waveguide(config);
            self.resonator_a.set_waveguide_loop_gain(loop_gain);
        }
        if let ResonatorConfig::Waveguide(mut config) = self.resonator_b_config {
            config.loop_gain = loop_gain;
            self.resonator_b_config = ResonatorConfig::Waveguide(config);
            self.resonator_b.set_waveguide_loop_gain(loop_gain);
        }
    }

    pub fn clear(&mut self) {
        self.excitation.clear();
        self.excitation_gain = 0.0;
        self.resonator_a.clear();
        self.resonator_b.clear();
        self.amp_state.reset();
        self.secondary_state.reset();
        self.output_filter.reset();
        self.series_conditioner.reset(self.sample_rate);
    }

    pub fn process_sample(&mut self) -> f32 {
        let sources = self.next_modulation_sources();
        let excitation_mod = self.modulation_sum(ModulationDestination::ExcitationGain, sources);
        let excitation = self.excitation.next_sample()
            * self.excitation_gain
            * (1.0 + excitation_mod).clamp(0.0, 2.0);

        let resonator_output = match self.routing {
            ResonatorRouting::Parallel { mix_a, mix_b } => {
                let a = self.resonator_a.process_sample(excitation);
                let b = self.resonator_b.process_sample(excitation);
                a * mix_a + b * mix_b
            }
            ResonatorRouting::Series => {
                let a = self.resonator_a.process_sample(excitation);
                let conditioned = self.series_conditioner.process_sample(a);
                self.resonator_b.process_sample(conditioned)
            }
        };

        let cutoff_mod = self.modulation_sum(ModulationDestination::FilterCutoff, sources);
        let filter_cutoff =
            (self.output.filter_cutoff * 2.0_f32.powf(cutoff_mod * 4.0)).clamp(20.0, 20_000.0);
        let filtered = if self.output_filter_is_bypassed() {
            resonator_output
        } else {
            self.output_filter.set_params(
                filter_cutoff,
                self.output.filter_resonance,
                svf_mode(self.output.filter_mode),
            );
            self.output_filter.process(resonator_output)
        };
        let saturated = soft_saturate(filtered, self.output.saturation_drive);
        let amp = sources.amp_envelope;

        snap_to_zero(saturated * amp * db_to_gain(self.output.master_gain_db))
    }

    fn output_filter_is_bypassed(&self) -> bool {
        matches!(self.output.filter_mode, FilterMode::LowPass)
            && self.output.filter_cutoff >= self.sample_rate * 0.415
    }

    fn next_modulation_sources(&mut self) -> ModulationSources {
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
        }
    }

    fn next_lfo_sample(&mut self) -> f32 {
        let lfo_rate_mod = self.modulation_sum(
            ModulationDestination::LfoRate,
            ModulationSources {
                amp_envelope: self.amp_state.value(),
                secondary_envelope: self.secondary_state.value(),
                lfo: 0.0,
                velocity: self.velocity,
                aftertouch: self.aftertouch,
                mod_wheel: self.mod_wheel,
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
}

#[derive(Debug, Clone, Copy)]
struct ModulationSources {
    amp_envelope: f32,
    secondary_envelope: f32,
    lfo: f32,
    velocity: f32,
    aftertouch: f32,
    mod_wheel: f32,
}

fn source_value(source: ModulationSource, values: ModulationSources) -> f32 {
    match source {
        ModulationSource::SecondaryEnvelope => values.secondary_envelope,
        ModulationSource::Lfo => values.lfo,
        ModulationSource::Velocity => values.velocity,
        ModulationSource::Aftertouch => values.aftertouch,
        ModulationSource::ModWheel => values.mod_wheel,
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

fn velocity_to_gain(velocity: f32, depth: f32) -> f32 {
    let velocity = velocity.clamp(0.0, 1.0);
    let depth = depth.clamp(0.0, 1.0);
    (1.0 - depth) + velocity * depth
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

    fn configure(&mut self, config: &ResonatorConfig, base_frequency: f32) {
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

        self.fast_env += 0.15 * (magnitude - self.fast_env);
        self.slow_env += 0.002 * (magnitude - self.slow_env);

        let transient_bias =
            ((self.fast_env - self.slow_env) / (self.fast_env + 1.0e-6)).clamp(0.0, 1.0);
        highpassed * (0.08 + transient_bias * 0.92)
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
        frequency_hz: tuned_frequency(base_frequency, config.semitone_offset, config.cent_offset),
        loop_filter_cutoff: config.loop_filter_cutoff,
        loop_gain: config.loop_gain,
        loop_nonlinearity: config.loop_nonlinearity,
        position_of_strike: config.position_of_strike,
    }
}

fn tuned_frequency(base_frequency: f32, semitone_offset: i8, cent_offset: f32) -> f32 {
    base_frequency * semitones_to_ratio(semitone_offset as f32 + cent_offset / 100.0)
}

fn svf_mode(mode: FilterMode) -> SvfMode {
    match mode {
        FilterMode::LowPass => SvfMode::Lowpass,
        FilterMode::BandPass => SvfMode::Bandpass,
        FilterMode::HighPass => SvfMode::Highpass,
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
        let mut patch = test_patch(ResonatorRouting::Series);
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
