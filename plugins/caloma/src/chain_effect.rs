//! The chain's effect adapter. `Effect::process` is product-neutral and carries no analysis
//! snapshot (ADR-0013), but the chain stores its slots as trait objects and must inject the shared
//! `SignalSnapshot` into the SwiftF0-consuming slots. [`ChainEffect`] is the caloma-side supertrait
//! that adds `set_snapshot` (a no-op by default; the four consumers override it). The trait impls
//! for all 20 slot effects and the `SlotId -> Box<dyn ChainEffect>` factory are generated from one
//! table by [`chain_slots!`], so the per-effect surface has a single source of truth.

use lindelion_effect::Effect;
use lindelion_speech_signals::SignalSnapshot;

use crate::patch::CalomaPatch;

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

/// Push a slot's typed patch params into its built effect through the `Effect::set_parameter`
/// boundary — the bridge between the persisted `slot_params` structs and the live DSP. The runtime
/// calls this whenever the patch changes (and once after every chain build), so the committed
/// per-order defaults and a loaded patch's knob values actually reach the effects; without this
/// application every effect runs at its crate defaults forever. Allocation-free: every effect's
/// `set_parameter` only stores fields / recomputes coefficients. Exhaustive over `SlotId`, so a
/// new slot cannot ship without a param mapping.
pub fn apply_slot_params(effect: &mut dyn ChainEffect, id: SlotId, patch: &CalomaPatch) {
    match id {
        SlotId::HighPass => {
            let p = patch.high_pass.params;
            effect.set_parameter(lindelion_speech_high_pass::PARAM_CUTOFF_HZ, p.cutoff);
            effect.set_parameter(lindelion_speech_high_pass::PARAM_SLOPE_DB_OCT, p.slope);
        }
        SlotId::NoiseGate => apply_noise_gate(effect, patch),
        SlotId::SpeechDenoiser => {
            let p = patch.speech_denoiser.params;
            effect.set_parameter(lindelion_speech_denoiser::PARAM_MIX, p.mix);
            effect.set_parameter(
                lindelion_speech_denoiser::PARAM_ATTEN_LIMIT_DB,
                p.atten_limit,
            );
        }
        SlotId::Dereverberation => effect.set_parameter(
            lindelion_speech_dereverberation::PARAM_AMOUNT_PCT,
            patch.dereverberation.params.amount,
        ),
        SlotId::FftNoiseRemoval => effect.set_parameter(
            lindelion_speech_fft_noise_removal::PARAM_AMOUNT_PCT,
            patch.fft_noise_removal.params.amount,
        ),
        SlotId::DeEsser => apply_de_esser(effect, patch),
        SlotId::FiveBandEq => apply_five_band_eq(effect, patch),
        SlotId::DynamicEq => apply_dynamic_eq(effect, patch),
        SlotId::Compressor => apply_compressor(effect, patch),
        SlotId::UpwardExpander => apply_upward_expander(effect, patch),
        SlotId::BassEnhancer => effect.set_parameter(
            lindelion_speech_bass_enhancer::PARAM_AMOUNT_PCT,
            patch.bass_enhancer.params.amount,
        ),
        SlotId::AirExciter => effect.set_parameter(
            lindelion_speech_air_exciter::PARAM_AMOUNT_PCT,
            patch.air_exciter.params.amount,
        ),
        SlotId::SpectralContrast => effect.set_parameter(
            lindelion_speech_spectral_contrast::PARAM_AMOUNT_PCT,
            patch.spectral_contrast.params.amount,
        ),
        SlotId::ConsonantTransient => effect.set_parameter(
            lindelion_speech_consonant_transient::PARAM_AMOUNT_PCT,
            patch.consonant_transient.params.amount,
        ),
        SlotId::Vitalizer => {
            use lindelion_speech_vitalizer as fx;
            let p = patch.vitalizer.params;
            effect.set_parameter(fx::PARAM_BASS_DB, p.bass);
            effect.set_parameter(fx::PARAM_TREBLE_DB, p.treble);
            effect.set_parameter(fx::PARAM_DRIVE_PCT, p.drive);
        }
        SlotId::Limiter => {
            let p = patch.limiter.params;
            effect.set_parameter(lindelion_speech_limiter::PARAM_CEILING_DB, p.ceiling);
            effect.set_parameter(lindelion_speech_limiter::PARAM_RELEASE_MS, p.release);
        }
        SlotId::VoiceGate => apply_voice_gate(effect, patch),
        SlotId::Saturation => {
            let p = patch.saturation.params;
            effect.set_parameter(lindelion_speech_saturation::PARAM_WARMTH_PCT, p.warmth);
            effect.set_parameter(lindelion_speech_saturation::PARAM_BLEND_PCT, p.blend);
        }
        SlotId::RoomTone => effect.set_parameter(
            lindelion_speech_room_tone::PARAM_LEVEL_DB,
            patch.room_tone.params.level,
        ),
        SlotId::Gain => {
            let p = patch.gain.params;
            effect.set_parameter(lindelion_speech_gain::PARAM_GAIN_DB, p.gain);
            effect.set_parameter(lindelion_speech_gain::PARAM_PHASE_INVERT, p.phase);
        }
    }
}

fn apply_noise_gate(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_noise_gate as fx;
    let p = patch.noise_gate.params;
    effect.set_parameter(fx::PARAM_THRESHOLD_DB, p.threshold);
    effect.set_parameter(fx::PARAM_HYSTERESIS_DB, p.hysteresis);
    effect.set_parameter(fx::PARAM_ATTACK_MS, p.attack);
    effect.set_parameter(fx::PARAM_HOLD_MS, p.hold);
    effect.set_parameter(fx::PARAM_RELEASE_MS, p.release);
}

fn apply_de_esser(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_de_esser as fx;
    let p = patch.de_esser.params;
    effect.set_parameter(fx::PARAM_CENTER_HZ, p.center_freq);
    effect.set_parameter(fx::PARAM_BANDWIDTH_HZ, p.bandwidth);
    effect.set_parameter(fx::PARAM_THRESHOLD_DB, p.threshold);
    effect.set_parameter(fx::PARAM_REDUCTION_DB, p.reduction);
    effect.set_parameter(fx::PARAM_MAX_RANGE_DB, p.max_range);
}

fn apply_five_band_eq(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_five_band_eq as fx;
    let p = patch.five_band_eq.params;
    effect.set_parameter(fx::PARAM_HPF_FREQ, p.hpf_freq);
    effect.set_parameter(fx::PARAM_LOW_SHELF_GAIN, p.low_shelf_gain);
    effect.set_parameter(fx::PARAM_LOW_SHELF_FREQ, p.low_shelf_freq);
    effect.set_parameter(fx::PARAM_LOW_MID_GAIN, p.low_mid_gain);
    effect.set_parameter(fx::PARAM_LOW_MID_FREQ, p.low_mid_freq);
    effect.set_parameter(fx::PARAM_LOW_MID_Q, p.low_mid_q);
    effect.set_parameter(fx::PARAM_HIGH_MID_GAIN, p.high_mid_gain);
    effect.set_parameter(fx::PARAM_HIGH_MID_FREQ, p.high_mid_freq);
    effect.set_parameter(fx::PARAM_HIGH_MID_Q, p.high_mid_q);
    effect.set_parameter(fx::PARAM_HIGH_SHELF_GAIN, p.high_shelf_gain);
    effect.set_parameter(fx::PARAM_HIGH_SHELF_FREQ, p.high_shelf_freq);
}

fn apply_dynamic_eq(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_dynamic_eq as fx;
    let p = patch.dynamic_eq.params;
    effect.set_parameter(fx::PARAM_LOW_BOOST_DB, p.low_boost);
    effect.set_parameter(fx::PARAM_HIGH_BOOST_DB, p.high_boost);
    effect.set_parameter(fx::PARAM_SCALE, p.scale);
    effect.set_parameter(fx::PARAM_SMOOTHING_MS, p.smoothing);
}

fn apply_compressor(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_compressor as fx;
    let p = patch.compressor.params;
    effect.set_parameter(fx::PARAM_THRESHOLD_DB, p.threshold);
    effect.set_parameter(fx::PARAM_RATIO, p.ratio);
    effect.set_parameter(fx::PARAM_ATTACK_MS, p.attack);
    effect.set_parameter(fx::PARAM_RELEASE_MS, p.release);
    effect.set_parameter(fx::PARAM_KNEE_DB, p.knee);
    effect.set_parameter(fx::PARAM_MAKEUP_DB, p.makeup);
}

fn apply_upward_expander(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_upward_expander as fx;
    let p = patch.upward_expander.params;
    effect.set_parameter(fx::PARAM_AMOUNT_PCT, p.amount);
    effect.set_parameter(fx::PARAM_THRESHOLD_DB, p.threshold);
    effect.set_parameter(fx::PARAM_LOW_SPLIT_HZ, p.low_split);
    effect.set_parameter(fx::PARAM_HIGH_SPLIT_HZ, p.high_split);
    effect.set_parameter(fx::PARAM_ATTACK_MS, p.attack);
    effect.set_parameter(fx::PARAM_RELEASE_MS, p.release);
    effect.set_parameter(fx::PARAM_GATE_STRENGTH, p.gate_strength);
}

fn apply_voice_gate(effect: &mut dyn ChainEffect, patch: &CalomaPatch) {
    use lindelion_speech_voice_gate as fx;
    let p = patch.voice_gate.params;
    effect.set_parameter(fx::PARAM_THRESHOLD, p.threshold);
    effect.set_parameter(fx::PARAM_ATTACK_MS, p.attack);
    effect.set_parameter(fx::PARAM_HOLD_MS, p.hold);
    effect.set_parameter(fx::PARAM_RELEASE_MS, p.release);
    effect.set_parameter(fx::PARAM_REDUCTION_DB, p.reduction);
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
