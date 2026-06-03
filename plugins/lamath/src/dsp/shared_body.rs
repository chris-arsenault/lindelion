//! Shared-body idiophone mode (M1–M5, ADR-0031).
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
//! injectors, so the existing ring keeps propagating. M3 makes the body melodically
//! playable: every strike after the first **retunes** the live body to the new pitch via
//! the state-preserving path (no buffer clear), so the prior ring keeps decaying while
//! the tuning tracks the latest strike.
//!
//! M4 adds the key-switch damp (a choke ramp to silence). M5 gives the body its own
//! measured-energy follower on the **raw** resonator output, driving the Mesh geometric
//! nonlinearity (Modal ignores energy); staging stays via the per-family-made-up
//! `staged_output`, and effort/drive-gate stay zero (idiophone has no bow/reed). The
//! amp-envelope bypass + static post-body coloration (M6) lands later.

use crate::dsp::{OutputStage, ResonatorStack, SurroundingStage};
use crate::{ResonatorRouting, ResonatorSynthPatch, SurroundingConfig};
use lindelion_dsp_utils::idiophone::EnergyFollower;

use super::excitation::{SelectedExcitations, VoiceExcitation};

/// Sample rate used when the host reports a non-finite or non-positive rate, matching
/// the sympathetic chamber's fallback.
const FALLBACK_SAMPLE_RATE: f32 = 48_000.0;

/// Number of simultaneous strike injectors. Each overlapping strike claims one; when
/// they are all live a new strike steals the oldest. Working value for M2 — the final
/// pool size is an M8 voicing call (ADR-0031).
const INJECTOR_POOL_SIZE: usize = 16;

/// Damp/choke ramp length (ms): a key-switch damp ramps the body's output gain to zero
/// over this window, then clears the ring (ADR-0031, M4, decision 4). Working value — the
/// final choke feel is an M8 voicing call.
const CHOKE_RAMP_MS: f32 = 60.0;

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
    /// `true` once a strike has configured the stack out of `Silent`. The first strike
    /// configures; every later strike retunes the live body ring-preserving (M3).
    struck: bool,
    injectors: [Injector<'a>; INJECTOR_POOL_SIZE],
    /// Round-robin steal cursor used only when the whole pool is live.
    cursor: usize,
    /// Output gain ramp for the key-switch damp (M4): `1.0` open, ramping to `0.0` while
    /// `choking`, at which point the ring is cleared. A strike re-opens it.
    choke_gain: f32,
    choking: bool,
    /// Per-sample decrement of `choke_gain` while choking (1.0 / ramp samples).
    choke_step: f32,
    /// Body-scoped measured-energy bus (M5, ADR-0029): an `EnergyFollower` on the **raw**
    /// resonator output drives the body's nonlinearity (Mesh geometric coupling). Like the
    /// voice, the followed RMS of one sample feeds the next sample's `energy` arg.
    energy_follower: EnergyFollower,
    body_energy: f32,
    /// Static post-body coloration (M6): the voice's output filter + saturation, applied
    /// with the per-note amp envelope stepped aside (`amp = 1.0`) — the body's decay is the
    /// envelope. A continuous stage (not retriggered per strike), configured from
    /// `patch.output`.
    output: OutputStage,
    /// Per-strike attack noise (M6): the voice's `SurroundingStage` mechanical-noise burst,
    /// armed on each strike. Radiation brightening is defeated (noise-only) — M6 specifies
    /// only the attack burst. Scaled by the strike force, supplied as `body_effort`.
    surrounding: SurroundingStage,
    body_effort: f32,
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
        let choke_step = 1.0 / (CHOKE_RAMP_MS * 0.001 * sample_rate).max(1.0);
        let mut output = OutputStage::new(sample_rate);
        output.reset(patch.output);
        let mut surrounding = SurroundingStage::new(sample_rate);
        surrounding.set_config(noise_only_surrounding(patch));
        Self {
            stack,
            sample_rate,
            enabled: false,
            routing: patch.routing,
            struck: false,
            injectors: [Injector::default(); INJECTOR_POOL_SIZE],
            cursor: 0,
            choke_gain: 1.0,
            choking: false,
            choke_step,
            energy_follower: EnergyFollower::new(sample_rate),
            body_energy: 0.0,
            output,
            surrounding,
            body_effort: 0.0,
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Strike the body: the first strike configures the stack to its idiophone config at
    /// the strike's base frequency; every later strike **retunes** the live body to the
    /// new pitch via the state-preserving path (no buffer clear), so the prior strike's
    /// decaying ring keeps propagating while the tuning tracks the latest strike (ADR-0031
    /// decision 3). Then a free injector is armed to play the selected excitation scaled
    /// by the strike force. Overlapping strikes add injectors without choking the ring.
    pub(crate) fn strike(&mut self, strike: BodyStrike<'a>) {
        // A strike re-opens the output gate, cancelling any in-progress damp ramp.
        self.choking = false;
        self.choke_gain = 1.0;
        if !self.struck {
            self.stack
                .configure_idiophone_body(strike.base_frequency, true, self.routing);
            self.struck = true;
        } else {
            self.stack.retune_idiophone_body(strike.base_frequency);
        }
        let index = self.free_injector_index();
        let injector = &mut self.injectors[index];
        injector
            .excitation
            .trigger(strike.selected, self.sample_rate, strike.pitch_ratio);
        injector.gain = strike.force_gain;
        // Arm the per-strike mechanical-noise attack (M6), scaled by the strike force.
        self.body_effort = strike.force_gain.clamp(0.0, 1.0);
        self.surrounding.trigger();
    }

    /// Key-switch damp (M4, decision 4): begin ramping the body's output gain toward
    /// silence. The ring keeps decaying audibly through the ramp and is cleared once the
    /// gain reaches zero, so a damped body is dead and the next strike starts fresh.
    /// Idempotent while already choking; a strike re-opens the gate.
    pub(crate) fn damp(&mut self) {
        self.choking = true;
    }

    /// Advance the choke ramp one sample, returning the gain to apply this sample. On
    /// reaching zero it clears the ring so the body is truly silent and re-strikes fresh.
    fn next_choke_gain(&mut self) -> f32 {
        if !self.choking {
            return self.choke_gain;
        }
        let gain = self.choke_gain;
        self.choke_gain -= self.choke_step;
        if self.choke_gain <= 0.0 {
            self.silence();
        }
        gain
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
            // Body-scoped energy bus (M5, ADR-0029): feed the previous sample's followed
            // energy into the nonlinearity (Mesh geometric drive; Modal ignores it), then
            // observe the **raw** resonator output to update it for the next sample —
            // staging stays decoupled from dynamics. Effort/drive-gate stay zero (idiophone
            // has no bow/reed). `staged_output` is the per-family-made-up audio tap; the
            // choke ramp (M4) attenuates it toward silence on a key-switch damp.
            let raw = self
                .stack
                .process_sample(excitation, self.body_energy, 0.0, 0.0);
            self.body_energy = self.energy_follower.observe(raw);
            let staged = self.stack.staged_output();
            // Per-strike attack noise (M6): the mechanical-noise burst, scaled by the strike
            // force; radiation brightening is defeated (noise-only) by the body's config.
            let surrounded = self
                .surrounding
                .process(staged, self.body_effort, self.body_energy);
            // Static post-body coloration (M6): the patch's output filter + saturation,
            // with the per-note amp envelope stepped aside (`amp = 1.0`) — the body's decay
            // is the envelope. The choke ramp (M4) attenuates the colored output to silence.
            let structural = self.output.apply_structural_transitions();
            let colored =
                self.output
                    .process_sample(surrounded, self.sample_rate, 0.0, 1.0, structural);
            let sample = colored * self.next_choke_gain();
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
        self.output.set_config(patch.output);
        self.surrounding.set_config(noise_only_surrounding(patch));
    }

    /// Silence the body's ring and disarm every injector (panic / patch change / reset),
    /// mirroring the chamber.
    pub(crate) fn silence(&mut self) {
        self.stack.clear(self.sample_rate);
        self.injectors = [Injector::default(); INJECTOR_POOL_SIZE];
        self.cursor = 0;
        self.struck = false;
        self.choking = false;
        self.choke_gain = 1.0;
        self.energy_follower.reset();
        self.body_energy = 0.0;
        self.output.clear();
        self.surrounding.reset();
        self.body_effort = 0.0;
    }
}

/// The body's surrounding config: the patch's mechanical-noise depth, with radiation
/// brightening defeated. M6 specifies only the per-strike attack burst, not the M10
/// energy-scaled radiation high-shelf.
fn noise_only_surrounding(patch: &ResonatorSynthPatch) -> SurroundingConfig {
    let mut config = patch.surrounding;
    config.radiation_brightness = 0.0;
    config
}

#[cfg(test)]
mod tests;
