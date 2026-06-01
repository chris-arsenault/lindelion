//! Coupling/contact stage between the driver and the resonator (M9).
//!
//! This is the gesture interface that turns a strike into a pick (tight, point
//! contact) or a strum (spread, slower contact). It sits **between the driver and
//! the waveguide injection**, inside the 2x oversampled inner loop (ADR-0016), and
//! owns two dimensions of the `ContactConfig`:
//!
//! - **Strike-position spread** (spatial): the base `spread` control widened further
//!   by playing effort (the "control + effort widens" voicing). It is written onto
//!   `WaveguideParams::excitation_spread` and consumed at injection, where it spreads
//!   the excitation across the string and averages out the strike-position comb.
//! - **Contact time** (temporal): a one-pole low-pass on the driven excitation whose
//!   cutoff drops as `contact_time` rises, spreading the momentum transfer in time so
//!   the onset mellows. It only shapes the forward excitation (a feed-forward stage,
//!   no new feedback path), so it stays passive and bounded.
//!
//! At the `ContactConfig` defaults (spread `0`, contact time `0`) the stage is a
//! **pass-through**: `effective_spread` is `0` (the cached narrow taps) and `shape`
//! returns its input unchanged, so a default patch renders exactly as before M9.

use lindelion_dsp_utils::{filters::OnePoleLowpass, math};

use crate::ContactConfig;

/// How much full playing effort multiplies the base spread control: at effort `1`
/// the strum is `1 + CONTACT_EFFORT_WIDEN` times as wide as the dialled-in spread,
/// so a harder gesture strums wider. At a `0` spread control the product stays `0`,
/// so effort never widens a patch that asked for a tight pick (identity preserved).
const CONTACT_EFFORT_WIDEN: f32 = 1.0;
/// Contact low-pass cutoff at the shortest contact (`contact_time` just above 0):
/// high enough to pass the full audible band, so a short contact is near-transparent.
/// At exactly `contact_time == 0` the stage is bypassed for a bit-exact identity.
const CONTACT_MAX_CUTOFF_HZ: f32 = 18_000.0;
/// Contact low-pass cutoff at the longest contact (`contact_time == 1`): a mellow,
/// dark onset from a slow, broad contact.
const CONTACT_MIN_CUTOFF_HZ: f32 = 800.0;

#[derive(Debug)]
pub(super) struct ContactStage {
    /// Base strike-position spread control `0..1`.
    spread: f32,
    /// Contact-time control `0..1`.
    contact_time: f32,
    /// Temporal contact low-pass, run at the 2x oversample rate.
    lowpass: OnePoleLowpass,
    /// The 2x oversample rate the low-pass is set at (the stage runs inside the loop).
    sample_rate: f32,
}

impl ContactStage {
    pub(super) fn from_config(config: ContactConfig, sample_rate: f32) -> Self {
        Self {
            spread: math::finite_clamp(config.spread, 0.0, 1.0, 0.0),
            contact_time: math::finite_clamp(config.contact_time, 0.0, 1.0, 0.0),
            lowpass: OnePoleLowpass::default(),
            sample_rate,
        }
    }

    /// The effort-widened strike-position spread to write onto the waveguide params.
    /// Constant across a host sample's sub-samples (effort is per host sample), so the
    /// engine computes it once per host sample.
    pub(super) fn effective_spread(&self, effort: f32) -> f32 {
        let effort = math::finite_clamp(effort, 0.0, 1.0, 0.0);
        math::finite_clamp(
            self.spread * (1.0 + effort * CONTACT_EFFORT_WIDEN),
            0.0,
            1.0,
            0.0,
        )
    }

    /// Temporal contact shaping of one (oversampled) driven excitation sample. At
    /// `contact_time == 0` the stage is a transparent pass-through (the pre-M9 path);
    /// higher spreads the momentum transfer in time, mellowing the onset.
    pub(super) fn shape(&mut self, sample: f32) -> f32 {
        if self.contact_time <= 0.0 {
            return sample;
        }
        let cutoff = CONTACT_MAX_CUTOFF_HZ
            + (CONTACT_MIN_CUTOFF_HZ - CONTACT_MAX_CUTOFF_HZ) * self.contact_time;
        self.lowpass.set_cutoff(cutoff, self.sample_rate);
        self.lowpass.process(sample)
    }

    pub(super) fn reset(&mut self) {
        self.lowpass.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contact_stage_is_transparent() {
        // The ContactConfig defaults (spread 0, contact time 0) make the stage a
        // pass-through: no spread for any effort, and `shape` returns its input
        // unchanged — so a default patch renders exactly as before M9.
        let mut stage = ContactStage::from_config(ContactConfig::default(), 96_000.0);
        for &effort in &[0.0_f32, 0.5, 1.0] {
            assert_eq!(stage.effective_spread(effort), 0.0);
        }
        for &sample in &[-1.5_f32, -0.2, 0.0, 0.3, 1.2] {
            assert_eq!(stage.shape(sample), sample);
        }
    }

    #[test]
    fn effort_widens_a_dialled_in_spread_but_never_a_tight_pick() {
        // A tight pick (spread 0) stays a point contact at any effort.
        let pick = ContactStage::from_config(
            ContactConfig {
                spread: 0.0,
                contact_time: 0.0,
            },
            96_000.0,
        );
        assert_eq!(pick.effective_spread(1.0), 0.0);

        // A dialled-in spread widens with effort (and stays bounded at 1).
        let strum = ContactStage::from_config(
            ContactConfig {
                spread: 0.4,
                contact_time: 0.0,
            },
            96_000.0,
        );
        assert!((strum.effective_spread(0.0) - 0.4).abs() < 1.0e-6);
        assert!(strum.effective_spread(1.0) > strum.effective_spread(0.0));
        assert!(strum.effective_spread(5.0) <= 1.0);
    }
}
