//! Typed per-effect parameter sets, one struct per `SlotId`. Each field is an `f32` mirroring one
//! `EffectParam` of the corresponding `speech/` effect (every effect param is an `f32` at the
//! `set_parameter(index, f32)` boundary), and each struct's `Default` equals that effect's
//! `EffectParam.default`. Keeping these typed (rather than a `BTreeMap<String, f32>`) lets the
//! compiler catch a missing or renamed param when the chain is wired (M2), an order is composed
//! (M3), or defaults are tuned (M5). The defaults here are the *effects'* defaults; the per-order
//! tuned defaults are chosen at M5.
//!
//! `#[serde(default)]` on every struct lets a partial TOML table fall back to these defaults, so a
//! patch only needs to record the values it changes.

use serde::{Deserialize, Serialize};

/// A slot's tuning: whether it is enabled in the chain, its dry/wet intensity, and its typed
/// parameters. `intensity` (0..1) is the editor's per-effect "strength" control — the runtime blends
/// the slot's dry input with its wet output by it (default 1.0 = fully wet / full effect).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotConfig<P> {
    pub enabled: bool,
    pub intensity: f32,
    pub params: P,
}

impl<P: Default> Default for SlotConfig<P> {
    fn default() -> Self {
        Self {
            enabled: false,
            intensity: 1.0,
            params: P::default(),
        }
    }
}

/// `high-pass` — cascaded Butterworth high-pass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighPassParams {
    pub cutoff: f32,
    pub slope: f32,
}

impl Default for HighPassParams {
    fn default() -> Self {
        Self {
            cutoff: 100.0,
            slope: 24.0,
        }
    }
}

/// `noise-gate` — level-driven downward gate with hysteresis and hold.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoiseGateParams {
    pub threshold: f32,
    pub hysteresis: f32,
    pub attack: f32,
    pub hold: f32,
    pub release: f32,
}

impl Default for NoiseGateParams {
    fn default() -> Self {
        Self {
            threshold: -40.0,
            hysteresis: 4.0,
            attack: 1.0,
            hold: 50.0,
            release: 100.0,
        }
    }
}

/// `speech-denoiser` — streaming DeepFilterNet 3 denoiser.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechDenoiserParams {
    pub mix: f32,
    pub atten_limit: f32,
}

impl Default for SpeechDenoiserParams {
    fn default() -> Self {
        Self {
            mix: 100.0,
            atten_limit: 100.0,
        }
    }
}

/// `dereverberation` — spectral suppression of late reverb.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DereverberationParams {
    pub amount: f32,
}

impl Default for DereverberationParams {
    fn default() -> Self {
        Self { amount: 60.0 }
    }
}

/// `fft-noise-removal` — spectral-subtraction noise removal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FftNoiseRemovalParams {
    pub amount: f32,
}

impl Default for FftNoiseRemovalParams {
    fn default() -> Self {
        Self { amount: 60.0 }
    }
}

/// `de-esser` — dynamic narrowband sibilance attenuation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeEsserParams {
    pub center_freq: f32,
    pub bandwidth: f32,
    pub threshold: f32,
    pub reduction: f32,
    pub max_range: f32,
}

impl Default for DeEsserParams {
    fn default() -> Self {
        Self {
            center_freq: 6_000.0,
            bandwidth: 2_000.0,
            threshold: -30.0,
            reduction: 6.0,
            max_range: 10.0,
        }
    }
}

/// `five-band-eq` — HPF + low shelf + two peaking mids + high shelf.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FiveBandEqParams {
    pub hpf_freq: f32,
    pub low_shelf_gain: f32,
    pub low_shelf_freq: f32,
    pub low_mid_gain: f32,
    pub low_mid_freq: f32,
    pub low_mid_q: f32,
    pub high_mid_gain: f32,
    pub high_mid_freq: f32,
    pub high_mid_q: f32,
    pub high_shelf_gain: f32,
    pub high_shelf_freq: f32,
}

impl Default for FiveBandEqParams {
    fn default() -> Self {
        Self {
            hpf_freq: 80.0,
            low_shelf_gain: 3.0,
            low_shelf_freq: 120.0,
            low_mid_gain: -3.0,
            low_mid_freq: 300.0,
            low_mid_q: 1.0,
            high_mid_gain: 3.0,
            high_mid_freq: 3_000.0,
            high_mid_q: 1.0,
            high_shelf_gain: 2.0,
            high_shelf_freq: 10_000.0,
        }
    }
}

/// `dynamic-eq` — voiced/fricative-driven low/high shelf shaping.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DynamicEqParams {
    pub low_boost: f32,
    pub high_boost: f32,
    pub scale: f32,
    pub smoothing: f32,
}

impl Default for DynamicEqParams {
    fn default() -> Self {
        Self {
            low_boost: 2.0,
            high_boost: 2.0,
            scale: 1.0,
            smoothing: 80.0,
        }
    }
}

/// `compressor` — feed-forward dynamics with soft knee.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompressorParams {
    pub threshold: f32,
    pub ratio: f32,
    pub attack: f32,
    pub release: f32,
    pub knee: f32,
    pub makeup: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold: -20.0,
            ratio: 4.0,
            attack: 10.0,
            release: 100.0,
            knee: 6.0,
            makeup: 0.0,
        }
    }
}

/// `upward-expander` — multiband upward expansion of quiet detail.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpwardExpanderParams {
    pub amount: f32,
    pub threshold: f32,
    pub low_split: f32,
    pub high_split: f32,
    pub attack: f32,
    pub release: f32,
    pub gate_strength: f32,
}

impl Default for UpwardExpanderParams {
    fn default() -> Self {
        Self {
            amount: 20.0,
            threshold: -35.0,
            low_split: 200.0,
            high_split: 3_500.0,
            attack: 8.0,
            release: 120.0,
            gate_strength: 0.8,
        }
    }
}

/// `bass-enhancer` — psychoacoustic bass via low-band harmonics.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BassEnhancerParams {
    pub amount: f32,
}

impl Default for BassEnhancerParams {
    fn default() -> Self {
        Self { amount: 50.0 }
    }
}

/// `air-exciter` — de-ess-aware high-frequency harmonic generation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AirExciterParams {
    pub amount: f32,
}

impl Default for AirExciterParams {
    fn default() -> Self {
        Self { amount: 40.0 }
    }
}

/// `spectral-contrast` — raise spectral peak/valley contrast.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpectralContrastParams {
    pub amount: f32,
}

impl Default for SpectralContrastParams {
    fn default() -> Self {
        Self { amount: 50.0 }
    }
}

/// `consonant-transient` — onset-flux-keyed consonant attack emphasis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsonantTransientParams {
    pub amount: f32,
}

impl Default for ConsonantTransientParams {
    fn default() -> Self {
        Self { amount: 40.0 }
    }
}

/// `vitalizer` — bass/treble psychoacoustic shaping with tube saturation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VitalizerParams {
    pub bass: f32,
    pub treble: f32,
    pub drive: f32,
}

impl Default for VitalizerParams {
    fn default() -> Self {
        Self {
            bass: 4.0,
            treble: 3.0,
            drive: 30.0,
        }
    }
}

/// `limiter` — lookahead brickwall limiter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimiterParams {
    pub ceiling: f32,
    pub release: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            ceiling: -1.0,
            release: 50.0,
        }
    }
}

/// `voice-gate` — Silero-VAD-driven gate for spoken word.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceGateParams {
    pub threshold: f32,
    pub attack: f32,
    pub hold: f32,
    pub release: f32,
    pub reduction: f32,
}

impl Default for VoiceGateParams {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            attack: 5.0,
            hold: 200.0,
            release: 150.0,
            reduction: 30.0,
        }
    }
}

/// `saturation` — asymmetric tanh warmth with dry/wet blend.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaturationParams {
    pub warmth: f32,
    pub blend: f32,
}

impl Default for SaturationParams {
    fn default() -> Self {
        Self {
            warmth: 50.0,
            blend: 100.0,
        }
    }
}

/// `room-tone` — synthetic room-tone bed with speech ducking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomToneParams {
    pub level: f32,
}

impl Default for RoomToneParams {
    fn default() -> Self {
        Self { level: -40.0 }
    }
}

/// `gain` — smoothed dB gain with optional phase invert.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GainParams {
    pub gain: f32,
    pub phase: f32,
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            gain: 0.0,
            phase: 0.0,
        }
    }
}
