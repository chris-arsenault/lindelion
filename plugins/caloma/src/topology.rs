//! The ordered slot sequence for each signal order — the chain topology the runtime processes.
//!
//! The three orders follow established speech / broadcast signal-path practice (not the original
//! plan sketch). The common spine is:
//!
//!   input gain → repair (deepest-first) → tonal (EQ) → dynamics / enhancement → de-ess → limiter
//!
//! - **Repair, deepest-first** ("mud flows downstream"): high-pass rumble removal, then broadband
//!   neural denoise (DFN3), then dereverb on the denoised signal, then the level gate last — on the
//!   clean, dry signal so no reverb tail holds it open and the noise floor is low enough for an easy
//!   threshold. (iZotope "Order of Audio Repair Operations"; broadcast HPF→gate convention.)
//! - **EQ before compression** (broadcast + vocal-chain consensus).
//! - **Enhancement is additive → after compression** for Clarity; **before compression** for
//!   Broadcast, where the compressor levels the enhanced signal for consistent broadcast loudness.
//! - **De-ess placement depends on the order:** late (after the air-exciter that adds sibilance) in
//!   Clarity; before the compressor in Broadcast.
//! - **Limiter last**; exciter before the limiter.
//! - **Gain staging:** one input-trim `Gain` at the head (a slot can appear only once — the patch
//!   has one field per `SlotId`); output level is the Compressor makeup + Limiter ceiling. All gain
//!   *values* are left to M5's empirical tuning.
//!
//! Dropped from the default orders: `FftNoiseRemoval` (the DFN3 neural denoiser covers broadband
//! noise), `Saturation` (the Vitalizer's tube stage covers warmth), `RoomTone` (niche).

use crate::order::SignalOrder;
use crate::slot::SlotId;

/// Clarity (default) — full clarity chain; enhancement *after* compression (additive, preserved).
const CLARITY: &[SlotId] = &[
    SlotId::Gain, // input trim (value tuned at M5)
    SlotId::HighPass,
    SlotId::SpeechDenoiser,
    SlotId::Dereverberation,
    SlotId::NoiseGate,
    SlotId::FiveBandEq,
    SlotId::DynamicEq,
    SlotId::Compressor,
    SlotId::Vitalizer,
    SlotId::BassEnhancer,
    SlotId::AirExciter,
    SlotId::SpectralContrast,
    SlotId::ConsonantTransient,
    SlotId::UpwardExpander,
    SlotId::DeEsser,
    SlotId::Limiter,
];

/// Broadcast — enhancement *before* compression so the compressor levels the enhanced signal for
/// consistent loudness; de-ess before the compressor.
const BROADCAST: &[SlotId] = &[
    SlotId::Gain, // input trim
    SlotId::HighPass,
    SlotId::SpeechDenoiser,
    SlotId::Dereverberation,
    SlotId::NoiseGate,
    SlotId::FiveBandEq,
    SlotId::DynamicEq,
    SlotId::BassEnhancer,
    SlotId::AirExciter,
    SlotId::SpectralContrast,
    SlotId::ConsonantTransient,
    SlotId::Vitalizer,
    SlotId::DeEsser,
    SlotId::Compressor,
    SlotId::UpwardExpander,
    SlotId::Limiter,
];

/// Light — minimal, transparency-first. Voice-gate (VAD, noise-robust) early; no dereverb; the
/// enhancement slots are present (after the compressor, the additive position) but disabled by the
/// default patch.
const LIGHT: &[SlotId] = &[
    SlotId::Gain, // input trim
    SlotId::HighPass,
    SlotId::VoiceGate,
    SlotId::SpeechDenoiser,
    SlotId::DeEsser,
    SlotId::FiveBandEq,
    SlotId::Compressor,
    SlotId::BassEnhancer, // present, disabled by default (see default_off_slots)
    SlotId::AirExciter,   // present, disabled by default
    SlotId::SpectralContrast, // present, disabled by default
    SlotId::ConsonantTransient, // present, disabled by default
    SlotId::Vitalizer,    // present, disabled by default
    SlotId::Limiter,
];

/// The ordered slots the chain processes for `order`.
pub fn order_topology(order: SignalOrder) -> &'static [SlotId] {
    match order {
        SignalOrder::Clarity => CLARITY,
        SignalOrder::Broadcast => BROADCAST,
        SignalOrder::Light => LIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clarity_sequence_is_the_research_grounded_order() {
        assert_eq!(
            order_topology(SignalOrder::Clarity),
            &[
                SlotId::Gain,
                SlotId::HighPass,
                SlotId::SpeechDenoiser,
                SlotId::Dereverberation,
                SlotId::NoiseGate,
                SlotId::FiveBandEq,
                SlotId::DynamicEq,
                SlotId::Compressor,
                SlotId::Vitalizer,
                SlotId::BassEnhancer,
                SlotId::AirExciter,
                SlotId::SpectralContrast,
                SlotId::ConsonantTransient,
                SlotId::UpwardExpander,
                SlotId::DeEsser,
                SlotId::Limiter,
            ]
        );
    }

    #[test]
    fn broadcast_sequence_is_the_research_grounded_order() {
        assert_eq!(
            order_topology(SignalOrder::Broadcast),
            &[
                SlotId::Gain,
                SlotId::HighPass,
                SlotId::SpeechDenoiser,
                SlotId::Dereverberation,
                SlotId::NoiseGate,
                SlotId::FiveBandEq,
                SlotId::DynamicEq,
                SlotId::BassEnhancer,
                SlotId::AirExciter,
                SlotId::SpectralContrast,
                SlotId::ConsonantTransient,
                SlotId::Vitalizer,
                SlotId::DeEsser,
                SlotId::Compressor,
                SlotId::UpwardExpander,
                SlotId::Limiter,
            ]
        );
    }

    #[test]
    fn light_sequence_is_the_research_grounded_order() {
        assert_eq!(
            order_topology(SignalOrder::Light),
            &[
                SlotId::Gain,
                SlotId::HighPass,
                SlotId::VoiceGate,
                SlotId::SpeechDenoiser,
                SlotId::DeEsser,
                SlotId::FiveBandEq,
                SlotId::Compressor,
                SlotId::BassEnhancer,
                SlotId::AirExciter,
                SlotId::SpectralContrast,
                SlotId::ConsonantTransient,
                SlotId::Vitalizer,
                SlotId::Limiter,
            ]
        );
    }

    #[test]
    fn the_three_topologies_differ() {
        let clarity = order_topology(SignalOrder::Clarity);
        let broadcast = order_topology(SignalOrder::Broadcast);
        let light = order_topology(SignalOrder::Light);
        assert_ne!(clarity, broadcast);
        assert_ne!(clarity, light);
        assert_ne!(broadcast, light);
    }

    #[test]
    fn no_order_repeats_a_slot() {
        // A slot maps to one patch field, so it may appear at most once per order.
        for order in SignalOrder::ALL {
            let ids = order_topology(order);
            let mut sorted: Vec<SlotId> = ids.to_vec();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), ids.len(), "{order:?} repeats a slot");
        }
    }
}
