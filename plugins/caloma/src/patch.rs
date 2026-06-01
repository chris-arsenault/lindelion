//! The Calóma patch: the signal-order value plus every slot's enable flag and typed parameters.
//! This is a *normal VST patch* — it round-trips through `lindelion-plugin-shell`'s patch I/O
//! (see `patch_io`). The order value lives inside the patch (ADR-0020); selecting an order loads
//! that order's default tuning (`default_patch_for` / `load_order_defaults`).
//!
//! Each named slot field corresponds 1:1 to a [`SlotId`](crate::slot::SlotId) catalog variant.
//! `#[serde(default)]` lets a patch record only the values it changes; everything else falls back
//! to the effects' defaults.

use serde::{Deserialize, Serialize};

use crate::order::SignalOrder;
use crate::slot::SlotId;
use crate::slot_params::{
    AirExciterParams, BassEnhancerParams, CompressorParams, ConsonantTransientParams,
    DeEsserParams, DereverberationParams, DynamicEqParams, FftNoiseRemovalParams, FiveBandEqParams,
    GainParams, HighPassParams, LimiterParams, NoiseGateParams, RoomToneParams, SaturationParams,
    SlotConfig, SpectralContrastParams, SpeechDenoiserParams, UpwardExpanderParams,
    VitalizerParams, VoiceGateParams,
};
use crate::topology::order_topology;

/// A full Calóma patch: the selected order plus per-slot enable + typed params for every catalog
/// slot. One field per [`SlotId`](crate::slot::SlotId).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CalomaPatch {
    pub order: SignalOrder,
    /// Input trim applied at the chain head (dB; default 0 = unity).
    pub input_level_db: f32,
    /// Output level applied after the chain (dB; default 0 = unity).
    pub output_level_db: f32,
    pub high_pass: SlotConfig<HighPassParams>,
    pub noise_gate: SlotConfig<NoiseGateParams>,
    pub speech_denoiser: SlotConfig<SpeechDenoiserParams>,
    pub dereverberation: SlotConfig<DereverberationParams>,
    pub fft_noise_removal: SlotConfig<FftNoiseRemovalParams>,
    pub de_esser: SlotConfig<DeEsserParams>,
    pub five_band_eq: SlotConfig<FiveBandEqParams>,
    pub dynamic_eq: SlotConfig<DynamicEqParams>,
    pub compressor: SlotConfig<CompressorParams>,
    pub upward_expander: SlotConfig<UpwardExpanderParams>,
    pub bass_enhancer: SlotConfig<BassEnhancerParams>,
    pub air_exciter: SlotConfig<AirExciterParams>,
    pub spectral_contrast: SlotConfig<SpectralContrastParams>,
    pub consonant_transient: SlotConfig<ConsonantTransientParams>,
    pub vitalizer: SlotConfig<VitalizerParams>,
    pub limiter: SlotConfig<LimiterParams>,
    pub voice_gate: SlotConfig<VoiceGateParams>,
    pub saturation: SlotConfig<SaturationParams>,
    pub room_tone: SlotConfig<RoomToneParams>,
    pub gain: SlotConfig<GainParams>,
}

impl CalomaPatch {
    /// Replace this patch with `order`'s default tuning — the program-change-like behaviour of the
    /// signal-order parameter (ADR-0020): selecting an order loads its defaults.
    pub fn load_order_defaults(&mut self, order: SignalOrder) {
        *self = default_patch_for(order);
    }

    /// Whether the slot is enabled in this patch.
    pub fn slot_enabled(&self, id: SlotId) -> bool {
        match id {
            SlotId::HighPass => self.high_pass.enabled,
            SlotId::NoiseGate => self.noise_gate.enabled,
            SlotId::SpeechDenoiser => self.speech_denoiser.enabled,
            SlotId::Dereverberation => self.dereverberation.enabled,
            SlotId::FftNoiseRemoval => self.fft_noise_removal.enabled,
            SlotId::DeEsser => self.de_esser.enabled,
            SlotId::FiveBandEq => self.five_band_eq.enabled,
            SlotId::DynamicEq => self.dynamic_eq.enabled,
            SlotId::Compressor => self.compressor.enabled,
            SlotId::UpwardExpander => self.upward_expander.enabled,
            SlotId::BassEnhancer => self.bass_enhancer.enabled,
            SlotId::AirExciter => self.air_exciter.enabled,
            SlotId::SpectralContrast => self.spectral_contrast.enabled,
            SlotId::ConsonantTransient => self.consonant_transient.enabled,
            SlotId::Vitalizer => self.vitalizer.enabled,
            SlotId::Limiter => self.limiter.enabled,
            SlotId::VoiceGate => self.voice_gate.enabled,
            SlotId::Saturation => self.saturation.enabled,
            SlotId::RoomTone => self.room_tone.enabled,
            SlotId::Gain => self.gain.enabled,
        }
    }

    /// The slot's dry/wet intensity (0..1) in this patch.
    pub fn slot_intensity(&self, id: SlotId) -> f32 {
        match id {
            SlotId::HighPass => self.high_pass.intensity,
            SlotId::NoiseGate => self.noise_gate.intensity,
            SlotId::SpeechDenoiser => self.speech_denoiser.intensity,
            SlotId::Dereverberation => self.dereverberation.intensity,
            SlotId::FftNoiseRemoval => self.fft_noise_removal.intensity,
            SlotId::DeEsser => self.de_esser.intensity,
            SlotId::FiveBandEq => self.five_band_eq.intensity,
            SlotId::DynamicEq => self.dynamic_eq.intensity,
            SlotId::Compressor => self.compressor.intensity,
            SlotId::UpwardExpander => self.upward_expander.intensity,
            SlotId::BassEnhancer => self.bass_enhancer.intensity,
            SlotId::AirExciter => self.air_exciter.intensity,
            SlotId::SpectralContrast => self.spectral_contrast.intensity,
            SlotId::ConsonantTransient => self.consonant_transient.intensity,
            SlotId::Vitalizer => self.vitalizer.intensity,
            SlotId::Limiter => self.limiter.intensity,
            SlotId::VoiceGate => self.voice_gate.intensity,
            SlotId::Saturation => self.saturation.intensity,
            SlotId::RoomTone => self.room_tone.intensity,
            SlotId::Gain => self.gain.intensity,
        }
    }

    /// Set whether the slot is enabled in this patch (mirror of `slot_enabled`).
    pub fn set_slot_enabled(&mut self, id: SlotId, enabled: bool) {
        match id {
            SlotId::HighPass => self.high_pass.enabled = enabled,
            SlotId::NoiseGate => self.noise_gate.enabled = enabled,
            SlotId::SpeechDenoiser => self.speech_denoiser.enabled = enabled,
            SlotId::Dereverberation => self.dereverberation.enabled = enabled,
            SlotId::FftNoiseRemoval => self.fft_noise_removal.enabled = enabled,
            SlotId::DeEsser => self.de_esser.enabled = enabled,
            SlotId::FiveBandEq => self.five_band_eq.enabled = enabled,
            SlotId::DynamicEq => self.dynamic_eq.enabled = enabled,
            SlotId::Compressor => self.compressor.enabled = enabled,
            SlotId::UpwardExpander => self.upward_expander.enabled = enabled,
            SlotId::BassEnhancer => self.bass_enhancer.enabled = enabled,
            SlotId::AirExciter => self.air_exciter.enabled = enabled,
            SlotId::SpectralContrast => self.spectral_contrast.enabled = enabled,
            SlotId::ConsonantTransient => self.consonant_transient.enabled = enabled,
            SlotId::Vitalizer => self.vitalizer.enabled = enabled,
            SlotId::Limiter => self.limiter.enabled = enabled,
            SlotId::VoiceGate => self.voice_gate.enabled = enabled,
            SlotId::Saturation => self.saturation.enabled = enabled,
            SlotId::RoomTone => self.room_tone.enabled = enabled,
            SlotId::Gain => self.gain.enabled = enabled,
        }
    }

    /// Set the slot's dry/wet intensity in this patch (mirror of `slot_intensity`).
    pub fn set_slot_intensity(&mut self, id: SlotId, intensity: f32) {
        match id {
            SlotId::HighPass => self.high_pass.intensity = intensity,
            SlotId::NoiseGate => self.noise_gate.intensity = intensity,
            SlotId::SpeechDenoiser => self.speech_denoiser.intensity = intensity,
            SlotId::Dereverberation => self.dereverberation.intensity = intensity,
            SlotId::FftNoiseRemoval => self.fft_noise_removal.intensity = intensity,
            SlotId::DeEsser => self.de_esser.intensity = intensity,
            SlotId::FiveBandEq => self.five_band_eq.intensity = intensity,
            SlotId::DynamicEq => self.dynamic_eq.intensity = intensity,
            SlotId::Compressor => self.compressor.intensity = intensity,
            SlotId::UpwardExpander => self.upward_expander.intensity = intensity,
            SlotId::BassEnhancer => self.bass_enhancer.intensity = intensity,
            SlotId::AirExciter => self.air_exciter.intensity = intensity,
            SlotId::SpectralContrast => self.spectral_contrast.intensity = intensity,
            SlotId::ConsonantTransient => self.consonant_transient.intensity = intensity,
            SlotId::Vitalizer => self.vitalizer.intensity = intensity,
            SlotId::Limiter => self.limiter.intensity = intensity,
            SlotId::VoiceGate => self.voice_gate.intensity = intensity,
            SlotId::Saturation => self.saturation.intensity = intensity,
            SlotId::RoomTone => self.room_tone.intensity = intensity,
            SlotId::Gain => self.gain.intensity = intensity,
        }
    }
}

/// Slots that are present in an order's topology but disabled by its default patch (Light's
/// enhancement slots; none for Clarity/Broadcast). Must be a subset of `order_topology(order)`.
fn default_off_slots(order: SignalOrder) -> &'static [SlotId] {
    match order {
        SignalOrder::Clarity | SignalOrder::Broadcast => &[],
        SignalOrder::Light => &[
            SlotId::BassEnhancer,
            SlotId::AirExciter,
            SlotId::SpectralContrast,
            SlotId::ConsonantTransient,
            SlotId::Vitalizer,
        ],
    }
}

/// The default patch an order loads (ADR-0020): its topology slots enabled (except the order's
/// default-off set), with params at the effect defaults — placeholders until M5's empirical tuning.
/// The structural baseline for an order: its topology slots enabled (minus the default-off set) at
/// the **effect defaults**. This is the seed the M5 tuner starts from and the fallback if a committed
/// default patch ever fails to parse (it should not — they are checked in).
fn order_default_baseline(order: SignalOrder) -> CalomaPatch {
    let mut patch = CalomaPatch {
        order,
        ..Default::default()
    };
    let off = default_off_slots(order);
    for &id in order_topology(order) {
        patch.set_slot_enabled(id, !off.contains(&id));
    }
    patch
}

/// The committed per-order tuned default patch (M5 writes these via `make tune-defaults`; until then
/// they equal `order_default_baseline`). Checked in as TOML so the tuning result is diff-reviewable.
const CLARITY_DEFAULTS_TOML: &str = include_str!("defaults/clarity.toml");
const BROADCAST_DEFAULTS_TOML: &str = include_str!("defaults/broadcast.toml");
const LIGHT_DEFAULTS_TOML: &str = include_str!("defaults/light.toml");

fn committed_default_toml(order: SignalOrder) -> &'static str {
    match order {
        SignalOrder::Clarity => CLARITY_DEFAULTS_TOML,
        SignalOrder::Broadcast => BROADCAST_DEFAULTS_TOML,
        SignalOrder::Light => LIGHT_DEFAULTS_TOML,
    }
}

/// The default patch an order loads (ADR-0020): its committed tuned default (M5). Falls back to the
/// structural baseline only if the committed TOML fails to parse.
pub fn default_patch_for(order: SignalOrder) -> CalomaPatch {
    match crate::patch_io::from_toml_str(committed_default_toml(order)) {
        Ok(mut patch) => {
            patch.order = order; // the committed file is per-order; keep the order authoritative
            patch
        }
        Err(_) => order_default_baseline(order),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch_io;

    fn non_trivial_patch() -> CalomaPatch {
        let mut patch = CalomaPatch {
            order: SignalOrder::Broadcast,
            ..Default::default()
        };
        // One slot explicitly disabled, another carrying a non-default parameter value.
        patch.noise_gate.enabled = false;
        patch.compressor.enabled = true;
        patch.compressor.params.ratio = 8.0;
        patch
    }

    #[test]
    fn patch_round_trips_through_plugin_state() {
        let patch = non_trivial_patch();
        let state = patch_io::to_plugin_state(&patch).unwrap();
        let decoded = patch_io::from_plugin_state(state).unwrap();
        assert_eq!(decoded, patch);
    }

    #[test]
    fn patch_round_trips_through_toml() {
        let patch = non_trivial_patch();
        let toml = patch_io::to_toml_string(&patch).unwrap();
        let decoded = patch_io::from_toml_str(&toml).unwrap();
        assert_eq!(decoded, patch);
    }

    #[test]
    fn serialized_patch_carries_the_format_version_envelope() {
        let patch = non_trivial_patch();
        let toml = patch_io::to_toml_string(&patch).unwrap();
        assert!(
            toml.contains("format_version"),
            "serialized patch must carry the versioned envelope, got:\n{toml}"
        );
    }

    #[test]
    fn default_patch_for_loads_the_committed_file() {
        // The durable invariant: the loader returns the checked-in TOML (not the in-code
        // construction), with the order kept authoritative. The committed files start equal to
        // `order_default_baseline` (M5 Step 5, behavior-preserving) and diverge once
        // `make tune-defaults` writes the tuned parameter values.
        for order in SignalOrder::ALL {
            let mut from_file = patch_io::from_toml_str(committed_default_toml(order)).unwrap();
            from_file.order = order;
            assert_eq!(
                default_patch_for(order),
                from_file,
                "{order:?}: loader vs file"
            );
        }
    }

    #[test]
    fn committed_defaults_enable_exactly_the_orders_composition() {
        // Tuning moves parameter values, never the enable set: the committed default's enabled slots
        // must always match the structural baseline's.
        for order in SignalOrder::ALL {
            let committed = default_patch_for(order);
            let baseline = order_default_baseline(order);
            for id in crate::slot::SlotId::ALL {
                assert_eq!(
                    committed.slot_enabled(id),
                    baseline.slot_enabled(id),
                    "{order:?} {id:?}: enable set must match the baseline composition"
                );
            }
        }
    }

    #[test]
    fn default_patch_for_carries_the_selected_order() {
        for order in SignalOrder::ALL {
            assert_eq!(default_patch_for(order).order, order);
        }
    }

    #[test]
    fn load_order_defaults_replaces_the_patch_with_that_orders_defaults() {
        let mut patch = non_trivial_patch();
        patch.load_order_defaults(SignalOrder::Light);
        assert_eq!(patch, default_patch_for(SignalOrder::Light));
    }

    #[test]
    fn default_patch_enables_exactly_the_orders_composition() {
        for order in SignalOrder::ALL {
            let patch = default_patch_for(order);
            for id in SlotId::ALL {
                let in_topology = order_topology(order).contains(&id);
                let default_off = default_off_slots(order).contains(&id);
                let expected = in_topology && !default_off;
                assert_eq!(
                    patch.slot_enabled(id),
                    expected,
                    "{order:?} slot {id:?}: in_topology={in_topology} default_off={default_off}"
                );
            }
        }
    }

    #[test]
    fn light_enhancement_slots_are_present_but_disabled() {
        let patch = default_patch_for(SignalOrder::Light);
        let topology = order_topology(SignalOrder::Light);
        for &id in default_off_slots(SignalOrder::Light) {
            assert!(topology.contains(&id), "Light topology must contain {id:?}");
            assert!(!patch.slot_enabled(id), "Light default must disable {id:?}");
        }
        // The active core stays enabled.
        assert!(patch.slot_enabled(SlotId::VoiceGate));
        assert!(patch.slot_enabled(SlotId::SpeechDenoiser));
        assert!(patch.slot_enabled(SlotId::Compressor));
    }

    #[test]
    fn default_off_slots_are_a_subset_of_the_topology() {
        for order in SignalOrder::ALL {
            let topology = order_topology(order);
            for &id in default_off_slots(order) {
                assert!(
                    topology.contains(&id),
                    "{order:?} default-off {id:?} is not in its topology"
                );
            }
        }
    }

    #[test]
    fn each_default_patch_round_trips_through_state() {
        for order in SignalOrder::ALL {
            let patch = default_patch_for(order);
            let state = patch_io::to_plugin_state(&patch).unwrap();
            let restored = patch_io::from_plugin_state(state).unwrap();
            assert_eq!(restored, patch, "{order:?} default patch must round-trip");
        }
    }

    #[test]
    fn tweaked_default_patch_round_trips_through_state() {
        let mut patch = default_patch_for(SignalOrder::Clarity);
        // A user tweak: flip one slot's enable and change one slot param.
        let was = patch.slot_enabled(SlotId::Compressor);
        patch.set_slot_enabled(SlotId::Compressor, !was);
        patch.bass_enhancer.params.amount = 12.0;
        let state = patch_io::to_plugin_state(&patch).unwrap();
        let restored = patch_io::from_plugin_state(state).unwrap();
        assert_eq!(restored, patch);
    }

    #[test]
    fn intensity_and_levels_round_trip_through_state() {
        let mut patch = default_patch_for(SignalOrder::Clarity);
        patch.input_level_db = -3.0;
        patch.output_level_db = 2.5;
        patch.compressor.intensity = 0.4;
        patch.air_exciter.intensity = 0.75;
        let state = patch_io::to_plugin_state(&patch).unwrap();
        let restored = patch_io::from_plugin_state(state).unwrap();
        assert_eq!(restored, patch);
    }
}
