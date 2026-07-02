//! Verifies every per-effect param struct's `Default` matches the corresponding `speech/` effect's
//! `EffectParam.default` values, and that serde round-trips (full and partial). Pure, in-memory.
//!
//! One `Default` test per effect: a transcription typo in `slot_params.rs` trips exactly the
//! effect it belongs to. Each explicit struct restates the effect crate's documented defaults.

use caloma::slot_params::*;

#[test]
fn high_pass_defaults() {
    assert_eq!(
        HighPassParams::default(),
        HighPassParams {
            cutoff: 100.0,
            slope: 24.0
        }
    );
}

#[test]
fn noise_gate_defaults() {
    assert_eq!(
        NoiseGateParams::default(),
        NoiseGateParams {
            threshold: -40.0,
            hysteresis: 4.0,
            attack: 1.0,
            hold: 50.0,
            release: 100.0
        }
    );
}

#[test]
fn speech_denoiser_defaults() {
    assert_eq!(
        SpeechDenoiserParams::default(),
        SpeechDenoiserParams {
            mix: 100.0,
            atten_limit: 100.0
        }
    );
}

#[test]
fn dereverberation_defaults() {
    assert_eq!(
        DereverberationParams::default(),
        DereverberationParams { amount: 60.0 }
    );
}

#[test]
fn fft_noise_removal_defaults() {
    assert_eq!(
        FftNoiseRemovalParams::default(),
        FftNoiseRemovalParams { amount: 60.0 }
    );
}

#[test]
fn de_esser_defaults() {
    assert_eq!(
        DeEsserParams::default(),
        DeEsserParams {
            center_freq: 6_000.0,
            bandwidth: 2_000.0,
            threshold: -12.0, // level-relative: band envelope over program level, dB
            reduction: 6.0,
            max_range: 10.0
        }
    );
}

#[test]
fn five_band_eq_defaults() {
    assert_eq!(
        FiveBandEqParams::default(),
        FiveBandEqParams {
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
            high_shelf_freq: 10_000.0
        }
    );
}

#[test]
fn dynamic_eq_defaults() {
    assert_eq!(
        DynamicEqParams::default(),
        DynamicEqParams {
            low_boost: 2.0,
            high_boost: 2.0,
            scale: 1.0,
            smoothing: 80.0
        }
    );
}

#[test]
fn compressor_defaults() {
    assert_eq!(
        CompressorParams::default(),
        CompressorParams {
            threshold: -20.0,
            ratio: 4.0,
            attack: 10.0,
            release: 100.0,
            knee: 6.0,
            makeup: 0.0
        }
    );
}

#[test]
fn upward_expander_defaults() {
    assert_eq!(
        UpwardExpanderParams::default(),
        UpwardExpanderParams {
            amount: 20.0,
            threshold: -35.0,
            low_split: 200.0,
            high_split: 3_500.0,
            attack: 8.0,
            release: 120.0,
            gate_strength: 0.8
        }
    );
}

#[test]
fn bass_enhancer_defaults() {
    assert_eq!(
        BassEnhancerParams::default(),
        BassEnhancerParams { amount: 50.0 }
    );
}

#[test]
fn air_exciter_defaults() {
    assert_eq!(
        AirExciterParams::default(),
        AirExciterParams { amount: 40.0 }
    );
}

#[test]
fn spectral_contrast_defaults() {
    assert_eq!(
        SpectralContrastParams::default(),
        SpectralContrastParams { amount: 50.0 }
    );
}

#[test]
fn consonant_transient_defaults() {
    assert_eq!(
        ConsonantTransientParams::default(),
        ConsonantTransientParams { amount: 40.0 }
    );
}

#[test]
fn vitalizer_defaults() {
    assert_eq!(
        VitalizerParams::default(),
        VitalizerParams {
            bass: 4.0,
            treble: 3.0,
            drive: 30.0
        }
    );
}

#[test]
fn limiter_defaults() {
    assert_eq!(
        LimiterParams::default(),
        LimiterParams {
            ceiling: -1.0,
            release: 50.0
        }
    );
}

#[test]
fn voice_gate_defaults() {
    assert_eq!(
        VoiceGateParams::default(),
        VoiceGateParams {
            threshold: 0.5,
            attack: 5.0,
            hold: 200.0,
            release: 150.0,
            reduction: 30.0
        }
    );
}

#[test]
fn saturation_defaults() {
    assert_eq!(
        SaturationParams::default(),
        SaturationParams {
            warmth: 50.0,
            blend: 100.0
        }
    );
}

#[test]
fn room_tone_defaults() {
    assert_eq!(RoomToneParams::default(), RoomToneParams { level: -40.0 });
}

#[test]
fn gain_defaults() {
    assert_eq!(
        GainParams::default(),
        GainParams {
            gain: 0.0,
            phase: 0.0
        }
    );
}

/// A struct serialised then parsed round-trips (representative multi-field struct).
#[test]
fn params_struct_serde_round_trips() {
    let params = CompressorParams {
        threshold: -18.0,
        ratio: 3.0,
        attack: 12.0,
        release: 90.0,
        knee: 4.0,
        makeup: 2.0,
    };
    let toml = toml::to_string(&params).unwrap();
    let parsed: CompressorParams = toml::from_str(&toml).unwrap();
    assert_eq!(parsed, params);
}

/// An empty TOML table fills every field from `Default` via `#[serde(default)]`.
#[test]
fn empty_table_parses_to_default() {
    assert_eq!(
        toml::from_str::<CompressorParams>("").unwrap(),
        CompressorParams::default()
    );
    assert_eq!(
        toml::from_str::<HighPassParams>("").unwrap(),
        HighPassParams::default()
    );
}

/// A partial TOML table keeps the named field and defaults the rest.
#[test]
fn partial_table_defaults_unspecified_fields() {
    let parsed: CompressorParams = toml::from_str("ratio = 8.0").unwrap();
    assert_eq!(
        parsed,
        CompressorParams {
            ratio: 8.0,
            ..CompressorParams::default()
        }
    );
}

/// `SlotConfig` round-trips and defaults its parts.
#[test]
fn slot_config_round_trips_and_defaults() {
    let config = SlotConfig {
        enabled: true,
        intensity: 0.6,
        params: LimiterParams {
            ceiling: -0.5,
            release: 75.0,
        },
    };
    let toml = toml::to_string(&config).unwrap();
    let parsed: SlotConfig<LimiterParams> = toml::from_str(&toml).unwrap();
    assert_eq!(parsed, config);

    let defaulted: SlotConfig<LimiterParams> = toml::from_str("").unwrap();
    assert_eq!(defaulted, SlotConfig::default());
    assert!(!defaulted.enabled);
    assert_eq!(defaulted.intensity, 1.0);
    assert_eq!(defaulted.params, LimiterParams::default());
}
