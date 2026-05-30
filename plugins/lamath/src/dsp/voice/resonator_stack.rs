use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math::{finite_clamp, finite_or, snap_to_zero},
    params::{StructuralChangePolicy, StructuralParam},
};
use lindelion_plugin_shell::SmoothedAtomicParam;

use crate::{
    ModalConfig, ModulationConfig, ModulationDestination, PARALLEL_MIX_A_PARAMETER_ID,
    PARALLEL_MIX_B_PARAMETER_ID, ResonatorConfig, ResonatorRouting, WaveguideConfig,
    normalize_routing_for_resonator_models, smoothed_runtime_parameter,
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
    waveguide::{MeshResonator, WaveguideParams, WaveguideResonator},
};

mod mapping;
use mapping::{mesh_params_from_config, modal_params_from_config, waveguide_params_from_config};

const BODY_COLOR_WINDOW_MS: f32 = 35.0;
const BODY_COLOR_RETRIGGER_MS: f32 = 80.0;
const BODY_COLOR_TRIGGER_THRESHOLD: f32 = 1.0e-5;
const BODY_COLOR_TRIGGER_RATIO: f32 = 1.5;
const BODY_COLOR_EXCITATION_GAIN: f32 = 0.006;

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
    body_color_exciter: BodyColorExciter,
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
            body_color_exciter: BodyColorExciter::new(sample_rate),
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
        let routing = self.sanitized_model_routing(routing);
        self.routing.reset(routing);
        self.parallel_mix_a.reset_plain(parallel_mix_a(routing));
        self.parallel_mix_b.reset_plain(parallel_mix_b(routing));
    }

    pub(super) fn set_routing(&mut self, routing: ResonatorRouting) {
        let routing = self.sanitized_model_routing(routing);
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

    #[cfg(test)]
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
            self.reset_body_color_exciter(sample_rate);
        }
        routing_sample.gain
    }

    pub(super) fn process_sample(&mut self, excitation: f32) -> f32 {
        let excitation = snap_to_zero(excitation);
        let mix_a = self.parallel_mix_a.next_sample();
        let mix_b = self.parallel_mix_b.next_sample();
        snap_to_zero(match self.routing.current() {
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
            ResonatorRouting::BodyColor { .. } => {
                let a = self.resonator_a.process_sample(excitation);
                let colored_excitation = self.body_color_exciter.process_sample(excitation, a);
                self.resonator_b.process_sample(colored_excitation)
            }
        })
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
        self.reset_body_color_exciter(sample_rate);
    }

    pub(super) fn reset_series_conditioner(&mut self, sample_rate: f32) {
        self.series_conditioner.reset(sample_rate);
    }

    fn sanitized_model_routing(&self, routing: ResonatorRouting) -> ResonatorRouting {
        sanitize_routing(normalize_routing_for_resonator_models(
            routing,
            self.base_resonator_a_config,
            self.base_resonator_b_config,
        ))
    }

    fn reset_body_color_exciter(&mut self, sample_rate: f32) {
        self.body_color_exciter.reset(sample_rate);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorKind {
    Silent,
    Modal,
    Waveguide,
    Mesh,
}

#[derive(Debug)]
struct ResonatorEngine {
    kind: ResonatorKind,
    modal: ModalBank,
    waveguide: WaveguideResonator,
    waveguide_params: WaveguideParams,
    mesh: MeshResonator,
}

impl ResonatorEngine {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            kind: ResonatorKind::Silent,
            modal: ModalBank::with_capacity(sample_rate, 256, ModalBankParams::default()),
            waveguide: WaveguideResonator::new(sample_rate, LOWEST_RESONATOR_FREQUENCY_HZ),
            waveguide_params: WaveguideParams::default(),
            mesh: MeshResonator::new(sample_rate),
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
            ResonatorConfig::Mesh(config) => {
                self.kind = ResonatorKind::Mesh;
                self.mesh
                    .configure(mesh_params_from_config(config, base_frequency));
                self.mesh.reset();
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
            (ResonatorKind::Mesh, ResonatorConfig::Mesh(config)) => {
                self.mesh
                    .configure(mesh_params_from_config(config, base_frequency));
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
            (ResonatorKind::Mesh, ResonatorConfig::Mesh(config)) => {
                self.mesh
                    .configure(mesh_params_from_config(config, base_frequency));
            }
            _ => self.configure(config, base_frequency, true),
        }
    }

    pub(super) fn clear(&mut self) {
        self.kind = ResonatorKind::Silent;
        self.modal.reset();
        self.waveguide.reset();
        self.mesh.reset();
    }

    pub(super) fn process_sample(&mut self, input: f32) -> f32 {
        match self.kind {
            ResonatorKind::Silent => 0.0,
            ResonatorKind::Modal => self.modal.process_sample(input),
            ResonatorKind::Waveguide => self.waveguide.process_sample(input, self.waveguide_params),
            ResonatorKind::Mesh => self.mesh.process_sample(input),
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
        let highpassed = snap_to_zero(self.highpass.process(input));
        let magnitude = highpassed.abs();

        self.fast_env =
            snap_to_zero(SERIES_CONDITIONER.next_fast_env(snap_to_zero(self.fast_env), magnitude));
        self.slow_env =
            snap_to_zero(SERIES_CONDITIONER.next_slow_env(snap_to_zero(self.slow_env), magnitude));

        let transient_bias = SERIES_CONDITIONER.transient_bias(self.fast_env, self.slow_env);
        snap_to_zero(highpassed * SERIES_CONDITIONER.output_gain(transient_bias))
    }
}

#[derive(Debug, Clone, Copy)]
struct BodyColorExciter {
    window_env: f32,
    trigger_peak: f32,
    window_decay: f32,
    trigger_decay: f32,
}

impl BodyColorExciter {
    fn new(sample_rate: f32) -> Self {
        let mut exciter = Self {
            window_env: 0.0,
            trigger_peak: 0.0,
            window_decay: 0.0,
            trigger_decay: 0.0,
        };
        exciter.reset(sample_rate);
        exciter
    }

    fn reset(&mut self, sample_rate: f32) {
        self.window_env = 0.0;
        self.trigger_peak = 0.0;
        self.window_decay = decay_to_floor(sample_rate, BODY_COLOR_WINDOW_MS);
        self.trigger_decay = decay_to_floor(sample_rate, BODY_COLOR_RETRIGGER_MS);
    }

    fn process_sample(&mut self, excitation: f32, color_sample: f32) -> f32 {
        let excitation = snap_to_zero(excitation);
        let color_sample = snap_to_zero(color_sample);
        self.window_env = finite_clamp(self.window_env, 0.0, 1.0, 0.0);
        self.trigger_peak = snap_to_zero(self.trigger_peak).max(0.0);
        let magnitude = excitation.abs();
        let trigger_threshold =
            BODY_COLOR_TRIGGER_THRESHOLD.max(self.trigger_peak * BODY_COLOR_TRIGGER_RATIO);
        if magnitude > trigger_threshold {
            self.window_env = 1.0;
        }
        self.trigger_peak = self.trigger_peak.max(magnitude) * self.trigger_decay;

        let colored = color_sample * self.window_env * BODY_COLOR_EXCITATION_GAIN;
        self.window_env *= self.window_decay;
        if self.window_env < BODY_COLOR_TRIGGER_THRESHOLD {
            self.window_env = 0.0;
        }

        snap_to_zero(colored)
    }
}

fn decay_to_floor(sample_rate: f32, duration_ms: f32) -> f32 {
    let sample_rate = finite_or(
        sample_rate,
        super::super::constants::DSP_FALLBACK_SAMPLE_RATE,
    );
    let duration_ms = finite_or(duration_ms, 0.0).max(0.0);
    let samples = (sample_rate * duration_ms * 0.001).max(1.0);
    0.001_f32.powf(1.0 / samples)
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
        ResonatorRouting::BodyColor { mix_a, mix_b } => ResonatorRouting::BodyColor {
            mix_a: finite_clamp(mix_a, 0.0, 1.0, 0.5),
            mix_b: finite_clamp(mix_b, 0.0, 1.0, 0.5),
        },
    }
}

pub(super) fn routing_plain(routing: ResonatorRouting) -> u8 {
    match routing {
        ResonatorRouting::Parallel { .. } => 0,
        ResonatorRouting::Series { .. } => 1,
        ResonatorRouting::BodyColor { .. } => 2,
    }
}

fn parallel_mix_a(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_a, .. } => mix_a,
        ResonatorRouting::Series { mix_a, .. } => mix_a,
        ResonatorRouting::BodyColor { mix_a, .. } => mix_a,
    }
}

fn parallel_mix_b(routing: ResonatorRouting) -> f32 {
    match routing {
        ResonatorRouting::Parallel { mix_b, .. } => mix_b,
        ResonatorRouting::Series { mix_b, .. } => mix_b,
        ResonatorRouting::BodyColor { mix_b, .. } => mix_b,
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
        ResonatorConfig::Mesh(mut config) => {
            // Positive damping modulation lengthens the decay, so it lowers the
            // mesh's boundary loss.
            config.damping =
                (config.damping - damping_mod * WAVEGUIDE_DAMPING_MOD_DEPTH).clamp(0.0, 1.0);
            config.position_of_strike = STRIKE_POSITION
                .clamp(config.position_of_strike + position_mod * RESONATOR_POSITION_MOD_DEPTH);
            ResonatorConfig::Mesh(config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_conditioner_recovers_from_non_finite_state_and_input() {
        let mut conditioner = SeriesConditioner::new(48_000.0);
        conditioner.fast_env = f32::NAN;
        conditioner.slow_env = f32::INFINITY;

        assert_eq!(conditioner.process_sample(f32::NAN), 0.0);
        assert!(conditioner.process_sample(0.25).is_finite());
    }

    #[test]
    fn body_color_exciter_recovers_from_non_finite_state_and_input() {
        let mut exciter = BodyColorExciter::new(48_000.0);
        exciter.window_env = f32::NAN;
        exciter.trigger_peak = f32::INFINITY;

        assert_eq!(exciter.process_sample(f32::NAN, f32::NAN), 0.0);
        assert!(exciter.process_sample(0.5, 0.25).is_finite());
    }
}
