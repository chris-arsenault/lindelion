use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math::{finite_clamp, semitones_to_ratio},
    params::{StructuralChangePolicy, StructuralParam},
};
use lindelion_plugin_shell::SmoothedAtomicParam;

use crate::{
    ModalConfig, ModulationConfig, ModulationDestination, PARALLEL_MIX_A_PARAMETER_ID,
    PARALLEL_MIX_B_PARAMETER_ID, ResonatorConfig, ResonatorRouting, WaveguideConfig,
    smoothed_runtime_parameter,
};

use super::{
    modulation_state::{ModulationSources, modulation_sum_from},
    structural_ramp_samples,
};
use crate::dsp::{
    constants::{
        LOWEST_RESONATOR_FREQUENCY_HZ, MODAL_DAMPING_MOD_OCTAVES, RESONATOR_POSITION_MOD_DEPTH,
        SERIES_CONDITIONER, STRIKE_POSITION, WAVEGUIDE_DAMPING_MOD_DEPTH, WAVEGUIDE_LOOP_GAIN,
    },
    modal::{ModalBank, ModalBankParams},
    waveguide::{WaveguideParams, WaveguideResonator},
};

#[derive(Debug)]
pub(super) struct ResonatorStack {
    resonator_a: ResonatorEngine,
    resonator_b: ResonatorEngine,
    pub(super) routing: StructuralParam<ResonatorRouting>,
    pub(super) base_resonator_a_config: ResonatorConfig,
    pub(super) base_resonator_b_config: ResonatorConfig,
    pub(super) resonator_a_config: ResonatorConfig,
    pub(super) resonator_b_config: ResonatorConfig,
    pub(super) parallel_mix_a: SmoothedAtomicParam,
    pub(super) parallel_mix_b: SmoothedAtomicParam,
    pub(super) series_conditioner: SeriesConditioner,
}

impl ResonatorStack {
    pub(super) fn new(sample_rate: f32) -> Self {
        let routing = ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        };
        let resonator_a_config = ResonatorConfig::Modal(ModalConfig::default());
        let resonator_b_config = ResonatorConfig::Waveguide(WaveguideConfig::default());
        Self {
            resonator_a: ResonatorEngine::new(sample_rate),
            resonator_b: ResonatorEngine::new(sample_rate),
            routing: StructuralParam::with_ramp_samples(
                routing,
                StructuralChangePolicy::LiveMuteRamp,
                structural_ramp_samples(sample_rate),
            ),
            base_resonator_a_config: resonator_a_config,
            base_resonator_b_config: resonator_b_config,
            resonator_a_config,
            resonator_b_config,
            parallel_mix_a: parallel_mix_a_param(sample_rate, parallel_mix_a(routing)),
            parallel_mix_b: parallel_mix_b_param(sample_rate, parallel_mix_b(routing)),
            series_conditioner: SeriesConditioner::new(sample_rate),
        }
    }

    pub(super) fn set_base_configs(
        &mut self,
        resonator_a: ResonatorConfig,
        resonator_b: ResonatorConfig,
    ) {
        self.base_resonator_a_config = resonator_a;
        self.base_resonator_b_config = resonator_b;
    }

    pub(super) fn configure_modulated(
        &mut self,
        modulation: ModulationConfig,
        sources: ModulationSources,
        base_frequency: f32,
        reset_state: bool,
        force: bool,
    ) {
        let resonator_a_config = modulated_resonator_config(
            self.base_resonator_a_config,
            modulation_sum_from(
                modulation,
                ModulationDestination::ResonatorADamping,
                sources,
            ),
            modulation_sum_from(
                modulation,
                ModulationDestination::ResonatorAPosition,
                sources,
            ),
        );
        let resonator_b_config = modulated_resonator_config(
            self.base_resonator_b_config,
            modulation_sum_from(
                modulation,
                ModulationDestination::ResonatorBDamping,
                sources,
            ),
            modulation_sum_from(
                modulation,
                ModulationDestination::ResonatorBPosition,
                sources,
            ),
        );

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

    pub(super) fn reset_routing(&mut self, routing: ResonatorRouting) {
        let routing = sanitize_routing(routing);
        self.routing.reset(routing);
        self.parallel_mix_a.reset_plain(parallel_mix_a(routing));
        self.parallel_mix_b.reset_plain(parallel_mix_b(routing));
    }

    pub(super) fn set_routing(&mut self, routing: ResonatorRouting) {
        let routing = sanitize_routing(routing);
        if routing_plain(self.routing.current()) != routing_plain(routing) {
            self.routing.set_target(routing);
        } else {
            self.routing.apply_immediate(routing);
        }
        self.parallel_mix_a
            .set_plain_target(parallel_mix_a(routing));
        self.parallel_mix_b
            .set_plain_target(parallel_mix_b(routing));
    }

    pub(super) fn set_base_waveguide_loop_gain(&mut self, loop_gain: f32) {
        let loop_gain = WAVEGUIDE_LOOP_GAIN.clamp(loop_gain);
        if let ResonatorConfig::Waveguide(mut config) = self.base_resonator_a_config {
            config.loop_gain = loop_gain;
            self.base_resonator_a_config = ResonatorConfig::Waveguide(config);
        }
        if let ResonatorConfig::Waveguide(mut config) = self.base_resonator_b_config {
            config.loop_gain = loop_gain;
            self.base_resonator_b_config = ResonatorConfig::Waveguide(config);
        }
    }

    pub(super) fn apply_structural_transitions(&mut self, sample_rate: f32) -> f32 {
        let routing_sample = self.routing.next_sample();
        if routing_sample.change.is_some() {
            self.reset_series_conditioner(sample_rate);
        }
        routing_sample.gain
    }

    pub(super) fn process_sample(&mut self, excitation: f32) -> f32 {
        let mix_a = self.parallel_mix_a.next_sample();
        let mix_b = self.parallel_mix_b.next_sample();
        match self.routing.current() {
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
        }
    }

    pub(super) fn retune(&mut self, base_frequency: f32) {
        self.resonator_a
            .retune(&self.resonator_a_config, base_frequency);
        self.resonator_b
            .retune(&self.resonator_b_config, base_frequency);
    }

    pub(super) fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        self.resonator_a.set_waveguide_loop_gain(loop_gain);
        self.resonator_b.set_waveguide_loop_gain(loop_gain);
    }

    pub(super) fn current_loop_gain(&self) -> f32 {
        loop_gain_from_configs(self.resonator_a_config, self.resonator_b_config)
    }

    pub(super) fn clear(&mut self, sample_rate: f32) {
        self.base_resonator_a_config = ResonatorConfig::Modal(ModalConfig::default());
        self.base_resonator_b_config = ResonatorConfig::Waveguide(WaveguideConfig::default());
        self.resonator_a_config = self.base_resonator_a_config;
        self.resonator_b_config = self.base_resonator_b_config;
        self.resonator_a.clear();
        self.resonator_b.clear();
        let routing = self.routing.current();
        self.parallel_mix_a.reset_plain(parallel_mix_a(routing));
        self.parallel_mix_b.reset_plain(parallel_mix_b(routing));
        self.reset_series_conditioner(sample_rate);
    }

    pub(super) fn reset_series_conditioner(&mut self, sample_rate: f32) {
        self.series_conditioner.reset(sample_rate);
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
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            kind: ResonatorKind::Silent,
            modal: ModalBank::with_capacity(sample_rate, 256, ModalBankParams::default()),
            waveguide: WaveguideResonator::new(sample_rate, LOWEST_RESONATOR_FREQUENCY_HZ),
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

    pub(super) fn retune(&mut self, config: &ResonatorConfig, base_frequency: f32) {
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

    pub(super) fn clear(&mut self) {
        self.kind = ResonatorKind::Silent;
        self.modal.reset();
        self.waveguide.reset();
    }

    pub(super) fn process_sample(&mut self, input: f32) -> f32 {
        match self.kind {
            ResonatorKind::Silent => 0.0,
            ResonatorKind::Modal => self.modal.process_sample(input),
            ResonatorKind::Waveguide => self.waveguide.process_sample(input, self.waveguide_params),
        }
    }

    pub(super) fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        if self.kind == ResonatorKind::Waveguide {
            self.waveguide_params.loop_gain = WAVEGUIDE_LOOP_GAIN.clamp(loop_gain);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SeriesConditioner {
    highpass: Biquad,
    fast_env: f32,
    slow_env: f32,
}

impl SeriesConditioner {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            highpass: Biquad::new(BiquadCoefficients::highpass(
                sample_rate,
                SERIES_CONDITIONER.highpass_cutoff_hz,
                SERIES_CONDITIONER.highpass_q,
            )),
            fast_env: 0.0,
            slow_env: 0.0,
        }
    }

    pub(super) fn reset(&mut self, sample_rate: f32) {
        self.highpass.set_coefficients(BiquadCoefficients::highpass(
            sample_rate,
            SERIES_CONDITIONER.highpass_cutoff_hz,
            SERIES_CONDITIONER.highpass_q,
        ));
        self.highpass.reset();
        self.fast_env = 0.0;
        self.slow_env = 0.0;
    }

    pub(super) fn process_sample(&mut self, input: f32) -> f32 {
        let highpassed = self.highpass.process(input);
        let magnitude = highpassed.abs();

        self.fast_env = SERIES_CONDITIONER.next_fast_env(self.fast_env, magnitude);
        self.slow_env = SERIES_CONDITIONER.next_slow_env(self.slow_env, magnitude);

        let transient_bias = SERIES_CONDITIONER.transient_bias(self.fast_env, self.slow_env);
        highpassed * SERIES_CONDITIONER.output_gain(transient_bias)
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

pub(super) fn routing_plain(routing: ResonatorRouting) -> u8 {
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

fn parallel_mix_a_param(sample_rate: f32, mix: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(PARALLEL_MIX_A_PARAMETER_ID, sample_rate, mix)
}

fn parallel_mix_b_param(sample_rate: f32, mix: f32) -> SmoothedAtomicParam {
    runtime_smoothed_param(PARALLEL_MIX_B_PARAMETER_ID, sample_rate, mix)
}

fn runtime_smoothed_param(id: u32, sample_rate: f32, initial_plain: f32) -> SmoothedAtomicParam {
    smoothed_runtime_parameter(id, sample_rate, initial_plain)
        .expect("live routing parameter should have smoothing metadata")
}

fn loop_gain_from_configs(resonator_a: ResonatorConfig, resonator_b: ResonatorConfig) -> f32 {
    match (resonator_a, resonator_b) {
        (ResonatorConfig::Waveguide(config), _) => WAVEGUIDE_LOOP_GAIN.clamp(config.loop_gain),
        (_, ResonatorConfig::Waveguide(config)) => WAVEGUIDE_LOOP_GAIN.clamp(config.loop_gain),
        _ => WAVEGUIDE_LOOP_GAIN.default,
    }
}

fn modulated_resonator_config(
    config: ResonatorConfig,
    damping_mod: f32,
    position_mod: f32,
) -> ResonatorConfig {
    match config {
        ResonatorConfig::Modal(mut config) => {
            config.decay_global = (config.decay_global
                * 2.0_f32.powf(damping_mod * MODAL_DAMPING_MOD_OCTAVES))
            .clamp(0.01, 10.0);
            config.position_of_strike = STRIKE_POSITION
                .clamp(config.position_of_strike + position_mod * RESONATOR_POSITION_MOD_DEPTH);
            ResonatorConfig::Modal(config)
        }
        ResonatorConfig::Waveguide(mut config) => {
            config.loop_gain = WAVEGUIDE_LOOP_GAIN
                .clamp(config.loop_gain + damping_mod * WAVEGUIDE_DAMPING_MOD_DEPTH);
            config.position_of_strike = STRIKE_POSITION
                .clamp(config.position_of_strike + position_mod * RESONATOR_POSITION_MOD_DEPTH);
            ResonatorConfig::Waveguide(config)
        }
    }
}
