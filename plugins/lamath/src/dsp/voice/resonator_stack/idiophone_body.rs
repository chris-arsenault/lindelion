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
    /// `reset_state` straight to [`ResonatorEngine::configure`]); a non-idiophone
    /// (`Waveguide`) base config clears that engine to `Silent`. Then applies `routing`.
    /// Mirrors the voice's `configure_modulated` path with modulation absent. The shared
    /// body calls this on the first strike; later strikes use the ring-preserving
    /// [`retune_idiophone_body`](Self::retune_idiophone_body).
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

    /// Ring-preserving retune of the live idiophone body to `base_frequency` (ADR-0031,
    /// M3, decision 3). For each idiophone slot, retunes that engine via the
    /// state-preserving [`ResonatorEngine::retune`] path (no buffer clear), so the prior
    /// strike's decaying ring keeps propagating while the tuning shifts to the latest
    /// strike. A non-idiophone slot was cleared to `Silent` by
    /// [`configure_idiophone_body`](Self::configure_idiophone_body) and is left
    /// untouched — the generic stack retune would wake it through its reset fallback.
    pub(crate) fn retune_idiophone_body(&mut self, base_frequency: f32) {
        Self::retune_idiophone_engine(
            &mut self.resonator_a,
            self.base_resonator_a_config,
            base_frequency,
        );
        Self::retune_idiophone_engine(
            &mut self.resonator_b,
            self.base_resonator_b_config,
            base_frequency,
        );
    }

    fn retune_idiophone_engine(
        engine: &mut ResonatorEngine,
        base_config: ResonatorConfig,
        base_frequency: f32,
    ) {
        if base_config.is_idiophone() {
            engine.retune(&base_config, base_frequency);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModalConfig, WaveguideConfig};
    use lindelion_dsp_utils::analysis::{assert_all_finite, dft_magnitude_at, peak_abs};

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

    /// ADR-0031 M3 step 1: the ring-preserving retune keeps the prior strike's decaying
    /// energy (no buffer clear) while shifting the body's tuning to the new pitch. With
    /// no new excitation after the retune, any output is the preserved ring, and its
    /// spectral energy now sits at the new fundamental.
    #[test]
    fn retune_idiophone_body_preserves_state_and_tracks_pitch() {
        const SR: f32 = 48_000.0;
        let freq_a = 220.0;
        let freq_b = 370.0;
        let mut stack = ResonatorStack::new(SR);
        stack.set_base_configs(
            ResonatorConfig::Modal(ModalConfig::default()),
            ResonatorConfig::Modal(ModalConfig::default()),
        );
        stack.configure_idiophone_body(
            freq_a,
            true,
            ResonatorRouting::Parallel {
                mix_a: 1.0,
                mix_b: 0.0,
            },
        );

        // Strike once and ring a short tail at A (energy still substantial).
        stack.process_sample(1.0, 0.0, 0.0, 0.0);
        for _ in 0..511 {
            stack.process_sample(0.0, 0.0, 0.0, 0.0);
        }

        // Ring-preserving retune to B, then render WITHOUT new excitation.
        stack.retune_idiophone_body(freq_b);
        let mut output = Vec::with_capacity(4_096);
        for _ in 0..4_096 {
            output.push(stack.process_sample(0.0, 0.0, 0.0, 0.0));
        }

        assert_all_finite(&output);
        assert!(
            peak_abs(&output) > 0.0,
            "retune must preserve the decaying ring (no reset)",
        );
        let mag_a = dft_magnitude_at(&output, SR, freq_a);
        let mag_b = dft_magnitude_at(&output, SR, freq_b);
        assert!(
            mag_b > mag_a,
            "the retuned ring must track the new pitch: mag_b {mag_b} <= mag_a {mag_a}",
        );
    }

    /// ADR-0031 M3 step 1: the retune is idiophone-restricted — a slot that
    /// `configure_idiophone_body` cleared to `Silent` (a waveguide base config) must stay
    /// silent, not be woken through `ResonatorEngine::retune`'s reset fallback.
    #[test]
    fn retune_idiophone_body_leaves_waveguide_slot_silent() {
        const SR: f32 = 48_000.0;
        let mut stack = ResonatorStack::new(SR);
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

        stack.retune_idiophone_body(370.0);

        let mut output = Vec::with_capacity(512);
        output.push(stack.process_sample(1.0, 0.0, 0.0, 0.0));
        for _ in 0..511 {
            output.push(stack.process_sample(0.0, 0.0, 0.0, 0.0));
        }

        assert_eq!(
            peak_abs(&output),
            0.0,
            "retune must not wake a silenced waveguide slot",
        );
    }
}
