use lindelion_dsp_utils::{
    math::{finite_clamp, snap_to_zero},
    params::{StructuralChangePolicy, StructuralParam},
};
use lindelion_plugin_shell::SmoothedAtomicParam;

use crate::{
    ModalConfig, PARALLEL_MIX_A_PARAMETER_ID, PARALLEL_MIX_B_PARAMETER_ID, ResonatorRouting,
    smoothed_runtime_parameter,
};

use super::structural_ramp_samples;
use crate::dsp::modal::{ModalBank, ModalBankParams};

mod conditioners;
mod mapping;
use conditioners::BodyColorExciter;
pub(super) use conditioners::SeriesConditioner;
use mapping::modal_params_from_config;

const MODAL_OUTPUT_MAKEUP: f32 = 0.6;
const MODAL_SERIES_OUTPUT_TRIM: f32 = 0.3;

#[derive(Debug)]
pub(crate) struct ResonatorStack {
    resonator_a: ResonatorEngine,
    resonator_b: ResonatorEngine,
    pub(super) routing: StructuralParam<ResonatorRouting>,
    pub(super) base_resonator_a_config: ModalConfig,
    pub(super) base_resonator_b_config: ModalConfig,
    pub(super) resonator_a_config: ModalConfig,
    pub(super) resonator_b_config: ModalConfig,
    pub(super) parallel_mix_a: SmoothedAtomicParam,
    pub(super) parallel_mix_b: SmoothedAtomicParam,
    pub(super) series_conditioner: SeriesConditioner,
    body_color_exciter: BodyColorExciter,
    staged_output: f32,
}

impl ResonatorStack {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let routing = ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        };
        let resonator_a_config = ModalConfig::default();
        let resonator_b_config = ModalConfig::default();
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
            staged_output: 0.0,
        }
    }

    pub(crate) fn staged_output(&self) -> f32 {
        self.staged_output
    }

    pub(crate) fn set_base_configs(&mut self, resonator_a: ModalConfig, resonator_b: ModalConfig) {
        self.base_resonator_a_config = resonator_a;
        self.base_resonator_b_config = resonator_b;
    }

    pub(super) fn configure(&mut self, base_frequency: f32, reset_state: bool, force: bool) {
        if force || self.base_resonator_a_config != self.resonator_a_config {
            self.resonator_a
                .configure(&self.base_resonator_a_config, base_frequency, reset_state);
            self.resonator_a_config = self.base_resonator_a_config;
        } else {
            self.resonator_a
                .retune(&self.resonator_a_config, base_frequency);
        }

        if force || self.base_resonator_b_config != self.resonator_b_config {
            self.resonator_b
                .configure(&self.base_resonator_b_config, base_frequency, reset_state);
            self.resonator_b_config = self.base_resonator_b_config;
        } else {
            self.resonator_b
                .retune(&self.resonator_b_config, base_frequency);
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

    pub(super) fn apply_structural_transitions(&mut self, sample_rate: f32) -> f32 {
        let routing_sample = self.routing.next_sample();
        if routing_sample.change.is_some() {
            self.reset_series_conditioner(sample_rate);
            self.reset_body_color_exciter(sample_rate);
        }
        routing_sample.gain
    }

    pub(crate) fn process_sample(&mut self, excitation: f32) -> f32 {
        let excitation = snap_to_zero(excitation);
        let mix_a = self.parallel_mix_a.next_sample();
        let mix_b = self.parallel_mix_b.next_sample();
        let (raw, staged) = match self.routing.current() {
            ResonatorRouting::Parallel { .. } => {
                let a = self.resonator_a.process_sample(excitation);
                let b = self.resonator_b.process_sample(excitation);
                (
                    a * mix_a + b * mix_b,
                    (a * mix_a + b * mix_b) * MODAL_OUTPUT_MAKEUP,
                )
            }
            ResonatorRouting::Series { .. } => {
                let a = self.resonator_a.process_sample(excitation);
                let conditioned = self.series_conditioner.process_sample(a);
                let out = self.resonator_b.process_sample(conditioned);
                (out, out * MODAL_OUTPUT_MAKEUP * MODAL_SERIES_OUTPUT_TRIM)
            }
            ResonatorRouting::BodyColor { .. } => {
                let a = self.resonator_a.process_sample(excitation);
                let colored_excitation = self.body_color_exciter.process_sample(excitation, a);
                let out = self.resonator_b.process_sample(colored_excitation);
                (out, out * MODAL_OUTPUT_MAKEUP)
            }
        };
        self.staged_output = snap_to_zero(staged);
        snap_to_zero(raw)
    }

    pub(super) fn retune(&mut self, base_frequency: f32) {
        self.resonator_a
            .retune(&self.resonator_a_config, base_frequency);
        self.resonator_b
            .retune(&self.resonator_b_config, base_frequency);
    }

    pub(crate) fn clear(&mut self, sample_rate: f32) {
        self.base_resonator_a_config = ModalConfig::default();
        self.base_resonator_b_config = ModalConfig::default();
        self.resonator_a_config = self.base_resonator_a_config;
        self.resonator_b_config = self.base_resonator_b_config;
        self.resonator_a.clear();
        self.resonator_b.clear();
        let routing = self.routing.current();
        self.parallel_mix_a.reset_plain(parallel_mix_a(routing));
        self.parallel_mix_b.reset_plain(parallel_mix_b(routing));
        self.reset_series_conditioner(sample_rate);
        self.reset_body_color_exciter(sample_rate);
        self.staged_output = 0.0;
    }

    pub(super) fn reset_series_conditioner(&mut self, sample_rate: f32) {
        self.series_conditioner.reset(sample_rate);
    }

    fn reset_body_color_exciter(&mut self, sample_rate: f32) {
        self.body_color_exciter.reset(sample_rate);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResonatorKind {
    Silent,
    Modal,
}

#[derive(Debug)]
struct ResonatorEngine {
    kind: ResonatorKind,
    modal: ModalBank,
}

impl ResonatorEngine {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            kind: ResonatorKind::Silent,
            modal: ModalBank::with_capacity(sample_rate, 256, ModalBankParams::default()),
        }
    }

    fn configure(&mut self, config: &ModalConfig, base_frequency: f32, reset_state: bool) {
        let params = modal_params_from_config(config, base_frequency);
        if !reset_state
            && self.kind == ResonatorKind::Modal
            && self.modal.modes().len() == params.mode_count
        {
            self.modal.retune(params);
            return;
        }

        self.kind = ResonatorKind::Modal;
        self.modal.configure(params);
        self.modal.reset();
    }

    pub(super) fn retune(&mut self, config: &ModalConfig, base_frequency: f32) {
        if self.kind == ResonatorKind::Modal {
            self.modal
                .retune(modal_params_from_config(config, base_frequency));
        } else {
            self.configure(config, base_frequency, true);
        }
    }

    pub(super) fn clear(&mut self) {
        self.kind = ResonatorKind::Silent;
        self.modal.reset();
    }

    pub(super) fn process_sample(&mut self, input: f32) -> f32 {
        match self.kind {
            ResonatorKind::Silent => 0.0,
            ResonatorKind::Modal => self.modal.process_sample(input),
        }
    }
}

fn sanitize_routing(routing: ResonatorRouting) -> ResonatorRouting {
    match routing {
        ResonatorRouting::Parallel { mix_a, mix_b } => ResonatorRouting::Parallel {
            mix_a: finite_clamp(mix_a, 0.0, 1.0, 0.5),
            mix_b: finite_clamp(mix_b, 0.0, 1.0, 0.5),
        },
        ResonatorRouting::Series { mix_a, mix_b } => ResonatorRouting::Series {
            mix_a: finite_clamp(mix_a, 0.0, 1.0, 1.0),
            mix_b: finite_clamp(mix_b, 0.0, 1.0, 1.0),
        },
        ResonatorRouting::BodyColor { mix_a, mix_b } => ResonatorRouting::BodyColor {
            mix_a: finite_clamp(mix_a, 0.0, 1.0, 1.0),
            mix_b: finite_clamp(mix_b, 0.0, 1.0, 1.0),
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

#[cfg(test)]
mod tests;
