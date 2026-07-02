//! Articulation styles: each slot maps onto the wind voice's note-start
//! physics — the tongue-release overpressure, the seed transient, onset
//! breath turbulence, and the phrase engine's legato seam — so picking an
//! articulation changes how the note *starts*, not just a sub-millisecond
//! seed click.
//!
//! Slot 0 (Tongue) is the identity style: every field is the neutral value,
//! so the shipped default note start stays bit-identical to the auditioned
//! voice. The other styles are derived from clarinet playing technique:
//! tonguing strength sets the overpressure arm and seed weight, breath
//! attacks trade tongue for turbulence, and slurred entries re-lock the mode
//! with the smallest boost that still speaks.

use super::ARTICULATION_SLOT_COUNT;

/// Decay time for the onset breath-turbulence boost. Long enough to read as
/// a breathy attack, short enough not to wash the sustain.
pub(super) const ONSET_NOISE_TAU_SECONDS: f32 = 0.08;

#[derive(Debug, Clone, Copy)]
pub(super) struct ArticulationStyle {
    /// Tongue-release overpressure arm in the vented register, where the
    /// boost covers the exponential bloom (1.0 = the auditioned tongued
    /// attack, the choke-cliff cap still applies).
    pub(super) vented_attack: f32,
    /// Overpressure arm below the break, where the bore speaks immediately
    /// and the boost is a loudness/brightness accent rather than bloom
    /// cover. 0 keeps the auditioned neutral low-register start.
    pub(super) low_attack: f32,
    /// Attack-overpressure decay-time multiplier (1.0 = the auditioned
    /// 70 ms): long emphasizes the attack, short clips it.
    pub(super) attack_tau_scale: f32,
    /// Seed-transient gain multiplier on the injected tonguing click.
    pub(super) seed_gain: f32,
    /// Onset breath-turbulence boost added to the reed's noise scale and
    /// decaying with [`ONSET_NOISE_TAU_SECONDS`].
    pub(super) onset_noise: f32,
    /// Drive-gate ramp-time multiplier (1.0 = the auditioned 6 ms): how fast
    /// the breath arrives from silence. Tongued starts are crisp; a breath
    /// attack swells the air in over tens of milliseconds; slurred entries
    /// ease in. This is the dominant audible difference between entries —
    /// the overpressure arm rides a compressive pressure-to-level map and
    /// cannot carry the distinction alone.
    pub(super) gate_ramp_scale: f32,
    /// Transient effort push above the played velocity, riding (and decaying
    /// with) the attack-overpressure envelope: a tongued accent's air push,
    /// which reads as a louder *and brighter* attack through the existing
    /// effort-to-steepening coupling. 0 keeps the played velocity exactly.
    pub(super) effort_accent: f32,
    /// Enter through the phrase engine's legato seam even from silence:
    /// the note keeps a developed breath/vibrato instead of restarting the
    /// lifecycle with a fresh tongued attack.
    pub(super) slurred: bool,
}

const NEUTRAL: ArticulationStyle = ArticulationStyle {
    vented_attack: 1.0,
    low_attack: 0.0,
    attack_tau_scale: 1.0,
    seed_gain: 1.0,
    onset_noise: 0.0,
    gate_ramp_scale: 1.0,
    effort_accent: 0.0,
    slurred: false,
};

/// One style per articulation slot, in [`super::ARTICULATION_NAMES`] order:
/// Tongue, Sforzando, Legato, Staccato, Marcato, Breath, Accent, Slur.
const ARTICULATION_STYLES: [ArticulationStyle; ARTICULATION_SLOT_COUNT] = [
    // Tongue: the auditioned default attack, untouched.
    NEUTRAL,
    // Sforzando: overblown attack, heavy tongue, a touch of breath blast,
    // emphasis held well past the bloom, with a strong transient air push.
    ArticulationStyle {
        low_attack: 0.8,
        attack_tau_scale: 1.7,
        seed_gain: 1.6,
        onset_noise: 0.25,
        effort_accent: 0.35,
        ..NEUTRAL
    },
    // Legato: soft tongue — the gentlest boost that still re-locks the
    // vented mode, a light seed, an eased entry, the legato seam even from
    // silence.
    ArticulationStyle {
        vented_attack: 0.45,
        attack_tau_scale: 0.8,
        seed_gain: 0.35,
        gate_ramp_scale: 1.5,
        slurred: true,
        ..NEUTRAL
    },
    // Staccato: crisp tongue, the emphasis clipped short, a fast gate.
    ArticulationStyle {
        low_attack: 0.6,
        attack_tau_scale: 0.55,
        seed_gain: 1.35,
        gate_ramp_scale: 0.7,
        effort_accent: 0.2,
        ..NEUTRAL
    },
    // Marcato: weighted accent, broader than tongue but short of sforzando.
    ArticulationStyle {
        low_attack: 0.7,
        attack_tau_scale: 1.35,
        seed_gain: 1.45,
        onset_noise: 0.1,
        effort_accent: 0.25,
        ..NEUTRAL
    },
    // Breath: no tongue — the note starts from air, turbulence carrying the
    // attack instead of the seed, the breath swelling in instead of arriving.
    ArticulationStyle {
        vented_attack: 0.55,
        low_attack: 0.2,
        attack_tau_scale: 1.2,
        seed_gain: 0.5,
        onset_noise: 0.6,
        gate_ramp_scale: 5.0,
        ..NEUTRAL
    },
    // Accent: sharp emphasis that decays at the normal rate.
    ArticulationStyle {
        low_attack: 0.75,
        seed_gain: 1.5,
        onset_noise: 0.15,
        gate_ramp_scale: 0.8,
        effort_accent: 0.3,
        ..NEUTRAL
    },
    // Slur: near-invisible entry — minimal re-lock boost, almost no seed,
    // the gentlest arrival.
    ArticulationStyle {
        vented_attack: 0.3,
        attack_tau_scale: 0.8,
        seed_gain: 0.2,
        gate_ramp_scale: 2.0,
        slurred: true,
        ..NEUTRAL
    },
];

pub(super) fn articulation_style(slot: usize) -> ArticulationStyle {
    ARTICULATION_STYLES[slot.min(ARTICULATION_SLOT_COUNT - 1)]
}

/// The finger-lift return to a still-held note (monophonic note stack): no
/// tongue at all — no seed, no onset air — just the smallest vented re-lock
/// boost through the legato seam, gentler than the played Slur articulation.
pub(super) const HELD_RETURN: ArticulationStyle = ArticulationStyle {
    vented_attack: 0.3,
    low_attack: 0.0,
    attack_tau_scale: 0.8,
    seed_gain: 0.0,
    onset_noise: 0.0,
    gate_ramp_scale: 1.5,
    effort_accent: 0.0,
    slurred: true,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tongue_is_the_identity_style() {
        let tongue = articulation_style(0);
        assert_eq!(tongue.vented_attack, 1.0);
        assert_eq!(tongue.low_attack, 0.0);
        assert_eq!(tongue.attack_tau_scale, 1.0);
        assert_eq!(tongue.seed_gain, 1.0);
        assert_eq!(tongue.onset_noise, 0.0);
        assert_eq!(tongue.gate_ramp_scale, 1.0);
        assert_eq!(tongue.effort_accent, 0.0);
        assert!(!tongue.slurred);
    }

    #[test]
    fn styles_stay_inside_the_safe_physical_envelope() {
        for slot in 0..ARTICULATION_SLOT_COUNT {
            let style = articulation_style(slot);
            assert!((0.0..=1.0).contains(&style.vented_attack));
            assert!((0.0..=1.0).contains(&style.low_attack));
            assert!((0.25..=2.5).contains(&style.attack_tau_scale));
            assert!((0.0..=2.0).contains(&style.seed_gain));
            assert!((0.0..=1.0).contains(&style.onset_noise));
            assert!((0.25..=16.0).contains(&style.gate_ramp_scale));
            assert!((0.0..=0.5).contains(&style.effort_accent));
        }
    }
}
