//! The chain's effect adapter. `Effect::process` is product-neutral and carries no analysis
//! snapshot (ADR-0013), but the chain stores its slots as trait objects and must inject the shared
//! `SignalSnapshot` into the SwiftF0-consuming slots. [`ChainEffect`] is the caloma-side supertrait
//! that adds `set_snapshot` (a no-op by default; the four consumers override it). The trait impls
//! for all 20 slot effects and the `SlotId -> Box<dyn ChainEffect>` factory are generated from one
//! table by [`chain_slots!`], so the per-effect surface has a single source of truth.

use lindelion_effect::Effect;
use lindelion_speech_signals::SignalSnapshot;

use lindelion_speech_air_exciter::AirExciter;
use lindelion_speech_bass_enhancer::BassEnhancer;
use lindelion_speech_compressor::Compressor;
use lindelion_speech_consonant_transient::ConsonantTransient;
use lindelion_speech_de_esser::DeEsser;
use lindelion_speech_denoiser::SpeechDenoiser;
use lindelion_speech_dereverberation::Dereverberation;
use lindelion_speech_dynamic_eq::DynamicEq;
use lindelion_speech_fft_noise_removal::FftNoiseRemoval;
use lindelion_speech_five_band_eq::FiveBandEq;
use lindelion_speech_gain::Gain;
use lindelion_speech_high_pass::HighPass;
use lindelion_speech_limiter::Limiter;
use lindelion_speech_noise_gate::NoiseGate;
use lindelion_speech_room_tone::RoomTone;
use lindelion_speech_saturation::Saturation;
use lindelion_speech_spectral_contrast::SpectralContrast;
use lindelion_speech_upward_expander::UpwardExpander;
use lindelion_speech_vitalizer::Vitalizer;
use lindelion_speech_voice_gate::VoiceGate;

use crate::slot::SlotId;

/// An [`Effect`] usable as a chain slot: it can also receive the shared analysis snapshot. The
/// default `set_snapshot` is a no-op; only the SwiftF0-consuming effects override it.
pub trait ChainEffect: Effect {
    fn set_snapshot(&mut self, _snapshot: &SignalSnapshot) {}
}

/// Single source of truth mapping each [`SlotId`] to its effect type. Generates the `ChainEffect`
/// impl per effect (no-op `set_snapshot`, or — when marked `(consumer)` — delegating to the
/// effect's inherent `set_snapshot`) and the exhaustive `build_chain_slot` factory.
macro_rules! chain_slots {
    ( $( $slot:ident => $ty:ident $( ($marker:ident) )? ; )+ ) => {
        $( chain_slots!(@impl $ty $( $marker )? ); )+

        /// Construct a freshly-defaulted effect for a slot, boxed as a [`ChainEffect`].
        pub fn build_chain_slot(id: SlotId) -> Box<dyn ChainEffect> {
            match id {
                $( SlotId::$slot => Box::new($ty::new()), )+
            }
        }
    };
    (@impl $ty:ident) => {
        impl ChainEffect for $ty {}
    };
    (@impl $ty:ident consumer) => {
        impl ChainEffect for $ty {
            fn set_snapshot(&mut self, snapshot: &SignalSnapshot) {
                $ty::set_snapshot(self, snapshot);
            }
        }
    };
}

chain_slots! {
    HighPass => HighPass;
    NoiseGate => NoiseGate;
    SpeechDenoiser => SpeechDenoiser;
    Dereverberation => Dereverberation;
    FftNoiseRemoval => FftNoiseRemoval;
    DeEsser => DeEsser;
    FiveBandEq => FiveBandEq;
    DynamicEq => DynamicEq (consumer);
    Compressor => Compressor;
    UpwardExpander => UpwardExpander (consumer);
    BassEnhancer => BassEnhancer (consumer);
    AirExciter => AirExciter;
    SpectralContrast => SpectralContrast;
    ConsonantTransient => ConsonantTransient (consumer);
    Vitalizer => Vitalizer;
    Limiter => Limiter;
    VoiceGate => VoiceGate;
    Saturation => Saturation;
    RoomTone => RoomTone;
    Gain => Gain;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * freq * i as f32 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn consumer_slot_forwards_set_snapshot_through_dyn() {
        let process_with = |voicing: f32| {
            let mut slot = build_chain_slot(SlotId::BassEnhancer);
            slot.prepare(48_000.0, 1_024);
            slot.set_snapshot(&SignalSnapshot {
                voicing_score: voicing,
                ..SignalSnapshot::default()
            });
            let mut buffer = tone(100.0, 4_096);
            slot.process(&mut buffer);
            buffer
        };
        // Bass gain rises with the injected voicing_score, so the outputs must differ — proving the
        // boxed `dyn ChainEffect` forwarded `set_snapshot` to the consumer.
        assert_ne!(process_with(1.0), process_with(0.0));
    }

    #[test]
    fn non_consumer_slot_set_snapshot_is_noop_and_processes_finite() {
        let mut slot = build_chain_slot(SlotId::HighPass);
        slot.prepare(48_000.0, 1_024);
        // No-op for a non-consumer; must not change behavior or panic.
        slot.set_snapshot(&SignalSnapshot {
            voicing_score: 1.0,
            ..SignalSnapshot::default()
        });
        let mut buffer = tone(200.0, 1_024);
        slot.process(&mut buffer);
        assert!(buffer.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn every_non_nn_slot_builds_with_a_name() {
        for id in SlotId::ALL {
            // SpeechDenoiser / VoiceGate load NN models — exercised by the make-test-models e2e.
            if matches!(id, SlotId::SpeechDenoiser | SlotId::VoiceGate) {
                continue;
            }
            let slot = build_chain_slot(id);
            assert!(!slot.name().is_empty(), "slot {id:?} has no name");
        }
    }
}
