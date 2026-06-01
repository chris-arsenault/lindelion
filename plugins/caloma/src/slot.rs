//! The catalog of effect slots available to the chain: one stable id per ported `speech/` effect
//! crate. ConvolutionReverb is intentionally absent — it was dropped from the product (D3,
//! ADR-0020). Which slots a given signal order uses, and in what sequence, is chain topology
//! defined at M2/M3; this enum is only the catalog. Each id is a stable kebab string (matching the
//! crate name) so it serialises identically forever — patches depend on this stability.

use serde::{Deserialize, Serialize};

/// One effect slot in the catalog. The serialised form is the crate-name kebab id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SlotId {
    #[serde(rename = "high-pass")]
    HighPass,
    #[serde(rename = "noise-gate")]
    NoiseGate,
    #[serde(rename = "speech-denoiser")]
    SpeechDenoiser,
    #[serde(rename = "dereverberation")]
    Dereverberation,
    #[serde(rename = "fft-noise-removal")]
    FftNoiseRemoval,
    #[serde(rename = "de-esser")]
    DeEsser,
    #[serde(rename = "five-band-eq")]
    FiveBandEq,
    #[serde(rename = "dynamic-eq")]
    DynamicEq,
    #[serde(rename = "compressor")]
    Compressor,
    #[serde(rename = "upward-expander")]
    UpwardExpander,
    #[serde(rename = "bass-enhancer")]
    BassEnhancer,
    #[serde(rename = "air-exciter")]
    AirExciter,
    #[serde(rename = "spectral-contrast")]
    SpectralContrast,
    #[serde(rename = "consonant-transient")]
    ConsonantTransient,
    #[serde(rename = "vitalizer")]
    Vitalizer,
    #[serde(rename = "limiter")]
    Limiter,
    #[serde(rename = "voice-gate")]
    VoiceGate,
    #[serde(rename = "saturation")]
    Saturation,
    #[serde(rename = "room-tone")]
    RoomTone,
    #[serde(rename = "gain")]
    Gain,
}

impl SlotId {
    /// A human-readable label for the editor's per-effect rows.
    pub fn label(self) -> &'static str {
        match self {
            Self::HighPass => "High-Pass",
            Self::NoiseGate => "Noise Gate",
            Self::SpeechDenoiser => "Speech Denoiser",
            Self::Dereverberation => "Dereverberation",
            Self::FftNoiseRemoval => "FFT Noise Removal",
            Self::DeEsser => "De-Esser",
            Self::FiveBandEq => "5-Band EQ",
            Self::DynamicEq => "Dynamic EQ",
            Self::Compressor => "Compressor",
            Self::UpwardExpander => "Upward Expander",
            Self::BassEnhancer => "Bass Enhancer",
            Self::AirExciter => "Air Exciter",
            Self::SpectralContrast => "Spectral Contrast",
            Self::ConsonantTransient => "Consonant Transient",
            Self::Vitalizer => "Vitalizer",
            Self::Limiter => "Limiter",
            Self::VoiceGate => "Voice Gate",
            Self::Saturation => "Saturation",
            Self::RoomTone => "Room Tone",
            Self::Gain => "Gain",
        }
    }

    /// Every catalog slot.
    pub const ALL: [SlotId; 20] = [
        Self::HighPass,
        Self::NoiseGate,
        Self::SpeechDenoiser,
        Self::Dereverberation,
        Self::FftNoiseRemoval,
        Self::DeEsser,
        Self::FiveBandEq,
        Self::DynamicEq,
        Self::Compressor,
        Self::UpwardExpander,
        Self::BassEnhancer,
        Self::AirExciter,
        Self::SpectralContrast,
        Self::ConsonantTransient,
        Self::Vitalizer,
        Self::Limiter,
        Self::VoiceGate,
        Self::Saturation,
        Self::RoomTone,
        Self::Gain,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kebab(slot: SlotId) -> String {
        match toml::Value::try_from(slot).unwrap() {
            toml::Value::String(id) => id,
            other => panic!("slot id must serialise to a string, got {other:?}"),
        }
    }

    #[test]
    fn all_lists_every_ported_effect() {
        // The 20 ported `speech/` effect crates (everything under `speech/` except the
        // `signals` analysis crate). ConvolutionReverb is not a slot (D3).
        assert_eq!(SlotId::ALL.len(), 20);
    }

    #[test]
    fn convolution_reverb_is_not_in_the_catalog() {
        let ids: Vec<String> = SlotId::ALL.into_iter().map(kebab).collect();
        assert!(!ids.iter().any(|id| id == "convolution-reverb"));
    }

    #[test]
    fn every_id_round_trips_through_its_kebab_string() {
        for slot in SlotId::ALL {
            let value = toml::Value::try_from(slot).unwrap();
            let parsed: SlotId = value.try_into().unwrap();
            assert_eq!(parsed, slot);
        }
    }

    #[test]
    fn all_ids_are_unique() {
        let mut ids: Vec<String> = SlotId::ALL.into_iter().map(kebab).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate slot ids in SlotId::ALL");
    }
}
