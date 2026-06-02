//! Runtime-scope idiophone configure for the shared body (ADR-0031, M2).
//!
//! The shared body reuses the per-voice [`ResonatorStack`] but is driven from the
//! orchestration layer, not a voice. This relocates the stack's voice-scope
//! `configure_modulated` path into a runtime-scope entry that configures each slot from
//! its **base** config, restricted to the idiophone families (Modal/Mesh) — the family
//! restriction decision 2 of ADR-0031 calls for.

use super::{ResonatorEngine, ResonatorStack};
use crate::{ResonatorConfig, ResonatorRouting};

impl ResonatorStack {
    /// Configure each slot from its base config at `base_frequency`, restricted to the
    /// idiophone families: a `Modal`/`Mesh` base config configures that engine (passing
    /// `reset_state` straight to [`ResonatorEngine::configure`], so M3 can retune
    /// ring-preserving with `reset_state = false`); a non-idiophone (`Waveguide`) base
    /// config clears that engine to `Silent`. Then applies `routing`. Mirrors the voice's
    /// `configure_modulated` path with modulation absent.
    pub(crate) fn configure_idiophone_body(
        &mut self,
        base_frequency: f32,
        reset_state: bool,
        routing: ResonatorRouting,
    ) {
        Self::configure_idiophone_engine(
            &mut self.resonator_a,
            self.base_resonator_a_config,
            base_frequency,
            reset_state,
        );
        Self::configure_idiophone_engine(
            &mut self.resonator_b,
            self.base_resonator_b_config,
            base_frequency,
            reset_state,
        );
        self.resonator_a_config = self.base_resonator_a_config;
        self.resonator_b_config = self.base_resonator_b_config;
        self.reset_routing(routing);
    }

    fn configure_idiophone_engine(
        engine: &mut ResonatorEngine,
        base_config: ResonatorConfig,
        base_frequency: f32,
        reset_state: bool,
    ) {
        if base_config.is_idiophone() {
            engine.configure(&base_config, base_frequency, reset_state);
        } else {
            engine.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModalConfig, WaveguideConfig};
    use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs};

    /// ADR-0031 M2 step 1: the runtime-scope idiophone configure moves a `Modal` base
    /// config out of `Silent`, so an impulse rings. Without it the stack stays silent.
    #[test]
    fn configure_idiophone_body_rings_a_modal_base_config() {
        let mut stack = ResonatorStack::new(48_000.0);
        stack.set_base_configs(
            ResonatorConfig::Modal(ModalConfig::default()),
            ResonatorConfig::Modal(ModalConfig::default()),
        );
        stack.configure_idiophone_body(
            220.0,
            true,
            ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
        );

        let mut output = Vec::with_capacity(2_048);
        output.push(stack.process_sample(1.0, 0.0, 0.0, 0.0));
        for _ in 0..2_047 {
            output.push(stack.process_sample(0.0, 0.0, 0.0, 0.0));
        }

        assert_all_finite(&output);
        assert!(
            peak_abs(&output) > 0.0,
            "configured idiophone body must ring from an impulse",
        );
    }

    /// ADR-0031 M2 step 1: a non-idiophone (`Waveguide`) base config is cleared to
    /// `Silent` by the idiophone configure, so its slot contributes nothing.
    #[test]
    fn configure_idiophone_body_silences_a_waveguide_base_config() {
        let mut stack = ResonatorStack::new(48_000.0);
        stack.set_base_configs(
            ResonatorConfig::Waveguide(WaveguideConfig::default()),
            ResonatorConfig::Waveguide(WaveguideConfig::default()),
        );
        stack.configure_idiophone_body(
            220.0,
            true,
            ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
        );

        let mut output = Vec::with_capacity(512);
        output.push(stack.process_sample(1.0, 0.0, 0.0, 0.0));
        for _ in 0..511 {
            output.push(stack.process_sample(0.0, 0.0, 0.0, 0.0));
        }

        assert_eq!(
            peak_abs(&output),
            0.0,
            "a waveguide base config must stay silent under the idiophone configure",
        );
    }
}
