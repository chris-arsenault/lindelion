use lindelion_dsp_utils::{
    math::{finite_clamp, snap_to_zero},
    params::{StructuralChangePolicy, StructuralParam},
};
use lindelion_plugin_shell::SmoothedAtomicParam;

use crate::{
    ContactConfig, DriverConfig, ModalConfig, ModulationConfig, ModulationDestination,
    PARALLEL_MIX_A_PARAMETER_ID, PARALLEL_MIX_B_PARAMETER_ID, ResonatorConfig, ResonatorRouting,
    WaveguideConfig, normalize_routing_for_resonator_models, smoothed_runtime_parameter,
};

use super::oversampler::Oversampler2x;
use super::{
    modulation_state::{ModulationSources, modulation_sum_from},
    structural_ramp_samples,
};
use crate::dsp::{
    constants::{LOWEST_RESONATOR_FREQUENCY_HZ, WAVEGUIDE_LOOP_GAIN},
    modal::{ModalBank, ModalBankParams},
    waveguide::{MeshResonator, WaveguideParams, WaveguideResonator},
};

mod conditioners;
mod contact;
mod driver;
mod makeup;
mod mapping;
use conditioners::BodyColorExciter;
pub(super) use conditioners::SeriesConditioner;
use contact::ContactStage;
use driver::Driver;
use mapping::{
    loop_gain_from_configs, mesh_params_from_config, modal_params_from_config,
    modulated_resonator_config, waveguide_params_from_config,
};

#[derive(Debug)]
pub(crate) struct ResonatorStack {
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
    driver_config: DriverConfig,
    contact_config: ContactConfig,
    /// M11 P9 per-resonator-made-up mix from the last `process_sample` (audio path).
    staged_output: f32,
}

impl ResonatorStack {
    pub(crate) fn new(sample_rate: f32) -> Self {
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
            driver_config: DriverConfig::default(),
            contact_config: ContactConfig::default(),
            staged_output: 0.0,
        }
    }

    /// M11 P9 staged output: the most recent `process_sample` mix with per-resonator
    /// output makeup applied (see [`makeup`]). The voice uses this for the audio path
    /// while tapping the energy bus from the raw (un-made-up) return value.
    pub(crate) fn staged_output(&self) -> f32 {
        self.staged_output
    }

    pub(crate) fn set_base_configs(
        &mut self,
        resonator_a: ResonatorConfig,
        resonator_b: ResonatorConfig,
    ) {
        self.base_resonator_a_config = resonator_a;
        self.base_resonator_b_config = resonator_b;
    }

    /// Select the physical driver (M8) for both waveguide engines. Only rebuilds when
    /// the driver config changes, so steady playback never re-allocates or clicks;
    /// the driver applies only on the waveguide path (Modal/Mesh ignore it).
    pub(super) fn set_driver(&mut self, config: DriverConfig) {
        if config != self.driver_config {
            self.driver_config = config;
            self.resonator_a.set_driver(config);
            self.resonator_b.set_driver(config);
        }
    }

    /// Select the coupling/contact stage (M9) for both waveguide engines. Only
    /// rebuilds when the contact config changes, so steady playback never
    /// re-allocates; the stage applies only on the waveguide path (Modal/Mesh
    /// ignore it). At the default config the stage is a transparent pass-through.
    pub(super) fn set_contact(&mut self, config: ContactConfig) {
        if config != self.contact_config {
            self.contact_config = config;
            self.resonator_a.set_contact(config);
            self.resonator_b.set_contact(config);
        }
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

    pub(crate) fn process_sample(
        &mut self,
        excitation: f32,
        energy: f32,
        effort: f32,
        drive_gate: f32,
    ) -> f32 {
        let excitation = snap_to_zero(excitation);
        let mix_a = self.parallel_mix_a.next_sample();
        let mix_b = self.parallel_mix_b.next_sample();
        let makeup_a = makeup::resonator_output_makeup(self.resonator_a_config, self.driver_config);
        let makeup_b = makeup::resonator_output_makeup(self.resonator_b_config, self.driver_config);
        // (raw, staged): the raw A/B mix feeds the energy tap (P8's physical bus); the
        // staged mix applies each slot's output makeup before the mix (P9 audio path).
        let (raw, staged) = match self.routing.current() {
            ResonatorRouting::Parallel { .. } => {
                let a = self
                    .resonator_a
                    .process_sample(excitation, energy, effort, drive_gate);
                let b = self
                    .resonator_b
                    .process_sample(excitation, energy, effort, drive_gate);
                (
                    a * mix_a + b * mix_b,
                    a * makeup_a * mix_a + b * makeup_b * mix_b,
                )
            }
            ResonatorRouting::Series { .. } => {
                let a = self
                    .resonator_a
                    .process_sample(excitation, energy, effort, drive_gate);
                let conditioned = self.series_conditioner.process_sample(a);
                let out = self
                    .resonator_b
                    .process_sample(conditioned, energy, effort, drive_gate);
                (out, out * makeup_b)
            }
            ResonatorRouting::BodyColor { .. } => {
                let a = self
                    .resonator_a
                    .process_sample(excitation, energy, effort, drive_gate);
                let colored_excitation = self.body_color_exciter.process_sample(excitation, a);
                let out =
                    self.resonator_b
                        .process_sample(colored_excitation, energy, effort, drive_gate);
                (out, out * makeup_b)
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

    pub(super) fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        self.resonator_a.set_waveguide_loop_gain(loop_gain);
        self.resonator_b.set_waveguide_loop_gain(loop_gain);
    }

    pub(super) fn current_loop_gain(&self) -> f32 {
        loop_gain_from_configs(self.resonator_a_config, self.resonator_b_config)
    }

    pub(crate) fn clear(&mut self, sample_rate: f32) {
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
    // The waveguide/mesh cores run at 2x through this oversampler (ADR-0016), the
    // shared substrate for the dynamic-response nonlinear stages. Modal stays at
    // the host rate and is never oversampled (untouched reference model).
    oversampler: Oversampler2x,
    // Force-dependent physical driver (M8): runs inside the 2x loop on the waveguide
    // path, driven by player effort and coupled two-way to the resonator through the
    // waveguide's input-end returning wave. Default pass-through, so a patch with no
    // physical driver is unaffected.
    driver: Driver,
    // Coupling/contact stage (M9): runs inside the 2x loop between the driver and
    // the waveguide injection. It writes the effort-widened strike-position spread
    // onto the params and applies the contact-time onset shaping. Default config is
    // a transparent pass-through.
    contact: ContactStage,
    // The 2x oversample rate the driver's inner-loop DSP is built at (the driver
    // runs inside the oversampled loop, so it must use that rate, not the host rate).
    oversample_rate: f32,
}

impl ResonatorEngine {
    pub(super) fn new(sample_rate: f32) -> Self {
        // Build the oversampled (waveguide/mesh) cores at twice the host rate so
        // their pitch and decay stay correct while their inner loop runs at 2x.
        let oversample_rate = 2.0 * sample_rate;
        Self {
            kind: ResonatorKind::Silent,
            modal: ModalBank::with_capacity(sample_rate, 256, ModalBankParams::default()),
            waveguide: WaveguideResonator::new(oversample_rate, LOWEST_RESONATOR_FREQUENCY_HZ),
            waveguide_params: WaveguideParams::default(),
            mesh: MeshResonator::new(oversample_rate),
            oversampler: Oversampler2x::new(),
            driver: Driver::default(),
            contact: ContactStage::from_config(ContactConfig::default(), oversample_rate),
            oversample_rate,
        }
    }

    /// Select the physical driver from the patch (M8). Rebuilt allocation-free from
    /// the config at the 2x oversample rate; the pass-through default leaves the
    /// excitation untouched.
    pub(super) fn set_driver(&mut self, config: DriverConfig) {
        self.driver = Driver::from_config(config, self.oversample_rate);
    }

    /// Select the coupling/contact stage from the patch (M9). Rebuilt allocation-free
    /// at the 2x oversample rate; the default config is a transparent pass-through.
    pub(super) fn set_contact(&mut self, config: ContactConfig) {
        self.contact = ContactStage::from_config(config, self.oversample_rate);
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
                self.oversampler.reset();
                self.driver.reset();
                self.contact.reset();
            }
            ResonatorConfig::Mesh(config) => {
                self.kind = ResonatorKind::Mesh;
                self.mesh
                    .configure(mesh_params_from_config(config, base_frequency));
                self.mesh.reset();
                self.oversampler.reset();
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
        self.oversampler.reset();
        self.driver.reset();
        self.contact.reset();
    }

    pub(super) fn process_sample(
        &mut self,
        input: f32,
        energy: f32,
        effort: f32,
        drive_gate: f32,
    ) -> f32 {
        match self.kind {
            ResonatorKind::Silent => 0.0,
            // Modal is the untouched reference: host rate, no oversampling, no driver.
            ResonatorKind::Modal => self.modal.process_sample(input),
            // Waveguide inner loop runs at 2x through the shared oversampler, with the
            // M8 physical driver inside the loop (ADR-0016/0017): each sub-sample the
            // driver shapes the excitation from player effort and the resonator's
            // coupled-back feedback, then the waveguide processes the driven sample.
            ResonatorKind::Waveguide => {
                // Measured energy drives the waveguide nonlinearities (String
                // tension M4, Tube steepening M5), set once per host sample
                // (constant across the 2x sub-samples).
                self.waveguide.set_energy_drive(energy);
                let waveguide = &mut self.waveguide;
                let driver = &mut self.driver;
                let contact = &mut self.contact;
                let mut params = self.waveguide_params;
                // The effort-widened strike-position spread (M9) is constant across
                // the host sample's sub-samples (effort is per host sample), so write
                // it onto the params once; the contact-time shaping runs per sub-sample.
                params.excitation_spread = contact.effective_spread(effort);
                self.oversampler.process(input, |sample| {
                    // Read the resonator's input-end returning wave (mouth/bridge)
                    // from the previous sub-sample as the driver's coupled feedback,
                    // run the driver, then the contact stage (contact-time onset
                    // shaping), then the waveguide with the spread set on the params.
                    let feedback = waveguide.driven_feedback(params);
                    let driven = driver.process(sample, effort, feedback, drive_gate);
                    let shaped = contact.shape(driven);
                    waveguide.process_sample(shaped, params)
                })
            }
            ResonatorKind::Mesh => {
                // Measured energy drives the mesh geometric (von Kármán) coupling
                // (M6), set once per host sample (constant across the 2x sub-samples).
                self.mesh.set_geometric_drive(energy);
                let mesh = &mut self.mesh;
                self.oversampler
                    .process(input, |sample| mesh.process_sample(sample))
            }
        }
    }

    pub(super) fn set_waveguide_loop_gain(&mut self, loop_gain: f32) {
        if self.kind == ResonatorKind::Waveguide {
            self.waveguide_params.loop_gain = WAVEGUIDE_LOOP_GAIN.clamp(loop_gain);
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

#[cfg(test)]
mod tests;
