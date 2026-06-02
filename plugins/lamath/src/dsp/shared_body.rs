//! Shared-body idiophone mode (M1–M2, ADR-0031).
//!
//! A single, runtime-owned **persistent** resonant body that idiophone note-ons
//! re-strike instead of allocating a per-note voice. Owned by the runtime at the
//! orchestration layer like [`SympatheticChamber`](super::SympatheticChamber) — the
//! voice engine knows nothing about it.
//!
//! M1 stood up the skeleton: one persistent [`ResonatorStack`](crate::dsp::ResonatorStack)
//! mirroring the patch's resonator configs as its base configs, summed into the mix
//! behind the `shared_body` toggle but not yet fed. M2 adds the **push strike path**:
//! a [`strike`](SharedBody::strike) configures the body to its idiophone config on the
//! first strike and arms a preallocated **injector pool** (each a [`VoiceExcitation`]
//! reused from the per-voice path) that plays the selected excitation into the live body
//! at the strike position baked into the resonator config. Overlapping strikes add fresh
//! injectors without re-configuring, so the existing ring keeps propagating.
//!
//! Per-strike ring-preserving *retune* (M3), key-switch damp (M4), the body energy
//! follower / gain staging (M5), and amp-envelope bypass (M6) land in later milestones.
//! Until M5 the body is driven with zero energy/effort/drive-gate — Modal ignores them
//! and Mesh simply runs without geometric drive, so a strike still rings.

use crate::dsp::ResonatorStack;
use crate::{ResonatorRouting, ResonatorSynthPatch};

use super::excitation::{SelectedExcitations, VoiceExcitation};

/// Sample rate used when the host reports a non-finite or non-positive rate, matching
/// the sympathetic chamber's fallback.
const FALLBACK_SAMPLE_RATE: f32 = 48_000.0;

/// Number of simultaneous strike injectors. Each overlapping strike claims one; when
/// they are all live a new strike steals the oldest. Working value for M2 — the final
/// pool size is an M8 voicing call (ADR-0031).
const INJECTOR_POOL_SIZE: usize = 16;

/// One armed strike: a reused [`VoiceExcitation`] playing the selected excitation into
/// the body, scaled by the strike's force gain. `gain == 0.0` marks a free slot.
#[derive(Debug, Clone, Copy)]
struct Injector<'a> {
    excitation: VoiceExcitation<'a>,
    gain: f32,
}

impl Default for Injector<'_> {
    fn default() -> Self {
        Self {
            excitation: VoiceExcitation::default(),
            gain: 0.0,
        }
    }
}

/// A single strike enqueued onto the body: force (velocity→gain), pitch (as the body's
/// base frequency plus the excitation playback ratio), and the selected excitation
/// layers. The strike position is baked into the mirrored resonator config, not carried
/// here (ADR-0031).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BodyStrike<'a> {
    pub(crate) selected: SelectedExcitations<'a>,
    pub(crate) force_gain: f32,
    pub(crate) base_frequency: f32,
    pub(crate) pitch_ratio: f32,
}

#[derive(Debug)]
pub(crate) struct SharedBody<'a> {
    stack: ResonatorStack,
    sample_rate: f32,
    enabled: bool,
    /// The body mirrors the patch routing so its idiophone configure matches the patch.
    routing: ResonatorRouting,
    /// `true` once a strike has configured the stack out of `Silent`. M2 configures on
    /// the first strike only; M3 replaces this with a per-strike ring-preserving retune.
    struck: bool,
    injectors: [Injector<'a>; INJECTOR_POOL_SIZE],
    /// Round-robin steal cursor used only when the whole pool is live.
    cursor: usize,
}

impl<'a> SharedBody<'a> {
    pub(crate) fn new(sample_rate: f32, patch: &ResonatorSynthPatch) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            FALLBACK_SAMPLE_RATE
        };
        let mut stack = ResonatorStack::new(sample_rate);
        // Mirror the patch's resonator configs as the body's base configs. This only
        // *stores* them; the engines stay in their `Silent` kind until a strike
        // configures them, so the freshly-built body is silent. The configure restricts
        // to the idiophone families (Modal/Mesh) at strike time.
        stack.set_base_configs(patch.resonator_a, patch.resonator_b);
        Self {
            stack,
            sample_rate,
            enabled: false,
            routing: patch.routing,
            struck: false,
            injectors: [Injector::default(); INJECTOR_POOL_SIZE],
            cursor: 0,
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Strike the body: on the first strike configure the stack to its idiophone config
    /// at the strike's base frequency (M2 configures once — M3 retunes ring-preserving
    /// per strike), then arm a free injector to play the selected excitation scaled by
    /// the strike force. Overlapping strikes add injectors without re-configuring, so the
    /// existing ring is never choked.
    pub(crate) fn strike(&mut self, strike: BodyStrike<'a>) {
        if !self.struck {
            self.stack
                .configure_idiophone_body(strike.base_frequency, true, self.routing);
            self.struck = true;
        }
        let index = self.free_injector_index();
        let injector = &mut self.injectors[index];
        injector
            .excitation
            .trigger(strike.selected, self.sample_rate, strike.pitch_ratio);
        injector.gain = strike.force_gain;
    }

    /// Pick a free injector (never armed, or finished playing); steal the oldest
    /// round-robin slot only when the whole pool is still live.
    fn free_injector_index(&mut self) -> usize {
        if let Some(index) = self
            .injectors
            .iter()
            .position(|injector| injector.gain == 0.0 || injector.excitation.is_finished())
        {
            return index;
        }
        let index = self.cursor;
        self.cursor = (self.cursor + 1) % INJECTOR_POOL_SIZE;
        index
    }

    /// Add the body's struck output into the mix. The body is a generator — it reads
    /// nothing from `left`/`right`, only adds the summed injector excitation driven
    /// through the persistent stack. Defeated (no-op) when disabled. Stereo placement is
    /// an M5 concern; M2 adds the mono body sample equally to both channels.
    pub(crate) fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled {
            return;
        }
        let len = left.len().min(right.len());
        for index in 0..len {
            let mut excitation = 0.0;
            for injector in &mut self.injectors {
                if injector.gain != 0.0 {
                    excitation += injector.excitation.next_sample() * injector.gain;
                }
            }
            // Zero energy/effort/drive-gate until the M5 body energy follower lands; the
            // unconfigured (unstruck) stack returns 0.0. `staged_output` is the voice's
            // P9 audio tap.
            self.stack.process_sample(excitation, 0.0, 0.0, 0.0);
            let sample = self.stack.staged_output();
            left[index] += sample;
            right[index] += sample;
        }
    }

    /// Re-mirror the patch's resonator configs and routing after a patch change. Only
    /// updates the stored base configs/routing; the body stays silent until struck.
    pub(crate) fn set_patch(&mut self, patch: &ResonatorSynthPatch) {
        self.stack
            .set_base_configs(patch.resonator_a, patch.resonator_b);
        self.routing = patch.routing;
    }

    /// Silence the body's ring and disarm every injector (panic / patch change / reset),
    /// mirroring the chamber.
    pub(crate) fn silence(&mut self) {
        self.stack.clear(self.sample_rate);
        self.injectors = [Injector::default(); INJECTOR_POOL_SIZE];
        self.cursor = 0;
        self.struck = false;
    }
}

#[cfg(test)]
mod tests;
