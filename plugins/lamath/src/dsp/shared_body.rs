//! Shared-body idiophone mode (M1, ADR-0031).
//!
//! A single, runtime-owned **persistent** resonant body that idiophone note-ons will
//! re-strike instead of allocating a per-note voice. Owned by the runtime at the
//! orchestration layer like [`SympatheticChamber`](super::SympatheticChamber) — the
//! voice engine knows nothing about it.
//!
//! M1 stands up only the skeleton: the body holds one persistent
//! [`ResonatorStack`](crate::dsp::ResonatorStack) (the same stack a voice owns, relocated
//! to runtime scope), mirrors the patch's resonator configs as its base configs, and is
//! summed into the mix behind the `shared_body` toggle — but it is **not yet fed**. With
//! the engines left in their `Silent` kind (no strike has configured them), the body
//! emits exactly `0.0`, so summing it is inert. The push strike path + injector pool
//! (M2), per-strike ring-preserving retune (M3), key-switch damp (M4), body energy
//! follower / gain staging (M5), and amp-envelope bypass (M6) land in later milestones.

use crate::ResonatorSynthPatch;
use crate::dsp::ResonatorStack;

/// Sample rate used when the host reports a non-finite or non-positive rate, matching
/// the sympathetic chamber's fallback.
const FALLBACK_SAMPLE_RATE: f32 = 48_000.0;

#[derive(Debug)]
pub(crate) struct SharedBody {
    stack: ResonatorStack,
    sample_rate: f32,
    enabled: bool,
}

impl SharedBody {
    pub(crate) fn new(sample_rate: f32, patch: &ResonatorSynthPatch) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            FALLBACK_SAMPLE_RATE
        };
        let mut stack = ResonatorStack::new(sample_rate);
        // Mirror the patch's resonator configs as the body's base configs. This only
        // *stores* them; the engines stay in their `Silent` kind until a strike
        // configures them (M2/M3), so the freshly-built body is silent. Restricting the
        // body to the idiophone families (Modal/Mesh) is a strike-time concern and lands
        // with the push strike path in M2.
        stack.set_base_configs(patch.resonator_a, patch.resonator_b);
        Self {
            stack,
            sample_rate,
            enabled: false,
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Add the body's struck output into the mix. The body is a generator — it reads
    /// nothing from `left`/`right`, only adds. Defeated (no-op) when disabled. Stereo
    /// placement is an M5 concern; M1 adds the mono body sample equally to both channels.
    pub(crate) fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled {
            return;
        }
        let len = left.len().min(right.len());
        for index in 0..len {
            // No strike yet (M1): zero excitation/energy/effort/drive-gate. The
            // unconfigured stack returns 0.0; `staged_output` is the voice's P9 audio tap.
            self.stack.process_sample(0.0, 0.0, 0.0, 0.0);
            let sample = self.stack.staged_output();
            left[index] += sample;
            right[index] += sample;
        }
    }

    /// Re-mirror the patch's resonator configs after a patch change. Only updates the
    /// stored base configs; the body stays silent until struck (M2).
    pub(crate) fn set_patch(&mut self, patch: &ResonatorSynthPatch) {
        self.stack
            .set_base_configs(patch.resonator_a, patch.resonator_b);
    }

    /// Silence the body's ring (panic / patch change / reset), mirroring the chamber.
    pub(crate) fn silence(&mut self) {
        self.stack.clear(self.sample_rate);
    }
}

#[cfg(test)]
mod tests;
