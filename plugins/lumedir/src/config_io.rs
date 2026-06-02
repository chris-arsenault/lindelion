//! Coaching-config persistence: encode/decode [`LumedirConfig`] through `lindelion-plugin-shell`'s
//! versioned TOML patch format — the same normal-VST-patch mechanism the other plugins use.

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};

use crate::config::LumedirConfig;

pub type ConfigIoError = TomlPatchError;

/// On-disk/state format version for Lúmedir's coaching config.
pub const FORMAT_VERSION: u32 = 1;

const CONFIG_FORMAT: TomlPatchFormat<LumedirConfig> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_toml_string(config: &LumedirConfig) -> Result<String, ConfigIoError> {
    CONFIG_FORMAT.to_toml_string(config)
}

pub fn from_toml_str(input: &str) -> Result<LumedirConfig, ConfigIoError> {
    CONFIG_FORMAT.from_toml_str(input)
}

pub fn to_plugin_state(config: &LumedirConfig) -> Result<PluginState, ConfigIoError> {
    CONFIG_FORMAT.to_plugin_state(config)
}

pub fn from_plugin_state(state: PluginState) -> Result<LumedirConfig, ConfigIoError> {
    CONFIG_FORMAT.from_plugin_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TargetBands;

    fn non_default_config() -> LumedirConfig {
        LumedirConfig {
            syllables_per_word: 1.7,
            bands: TargetBands {
                rate_syl_per_s_min: 2.5,
                rate_syl_per_s_max: 4.5,
                wpm_min: 110.0,
                wpm_max: 170.0,
                dynamism_semitones_min: 2.5,
                pause_fraction_min: 0.05,
                pause_fraction_max: 0.35,
                clarity_min: 0.55,
            },
        }
    }

    #[test]
    fn config_round_trips_through_plugin_state() {
        let config = non_default_config();
        let state = to_plugin_state(&config).expect("encode");
        let decoded = from_plugin_state(state).expect("decode");
        assert_eq!(decoded, config);
    }

    #[test]
    fn serialized_config_carries_the_format_version_envelope() {
        let toml = to_toml_string(&LumedirConfig::default()).expect("encode");
        assert!(
            toml.contains("format_version"),
            "serialized config must carry the versioned envelope, got:\n{toml}"
        );
    }
}
