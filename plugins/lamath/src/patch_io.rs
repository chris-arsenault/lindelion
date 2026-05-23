use std::{
    fs,
    path::{Path, PathBuf},
};

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat, TomlPatchMigration};
use lindelion_sample_library::LibraryPaths;

use crate::ResonatorSynthPatch;

pub const FORMAT_VERSION: u32 = 1;
pub type PatchIoError = TomlPatchError;

const PATCH_FORMAT: TomlPatchFormat<ResonatorSynthPatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_toml_string(patch: &ResonatorSynthPatch) -> Result<String, PatchIoError> {
    PATCH_FORMAT.to_toml_string(patch)
}

pub fn from_toml_str(input: &str) -> Result<ResonatorSynthPatch, PatchIoError> {
    PATCH_FORMAT.from_toml_str_with_migration(input, ResonatorPatchMigration)
}

pub fn save_patch(path: impl AsRef<Path>, patch: &ResonatorSynthPatch) -> Result<(), PatchIoError> {
    PATCH_FORMAT.save_patch(path, patch)
}

pub fn load_patch(path: impl AsRef<Path>) -> Result<ResonatorSynthPatch, PatchIoError> {
    PATCH_FORMAT.load_patch_with_migration(path, ResonatorPatchMigration)
}

pub fn to_plugin_state(patch: &ResonatorSynthPatch) -> Result<PluginState, PatchIoError> {
    PATCH_FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<ResonatorSynthPatch, PatchIoError> {
    PATCH_FORMAT.from_plugin_state_with_migration(state, ResonatorPatchMigration)
}

pub fn save_library_patch(
    paths: &LibraryPaths,
    patch: &ResonatorSynthPatch,
) -> Result<PathBuf, PatchIoError> {
    fs::create_dir_all(&paths.patches)?;
    let path = paths
        .patches
        .join(format!("{}.toml", sanitize_patch_name(&patch.name)));
    save_patch(&path, patch)?;
    Ok(path)
}

pub fn load_library_patch(path: impl AsRef<Path>) -> Result<ResonatorSynthPatch, PatchIoError> {
    load_patch(path)
}

fn sanitize_patch_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();

    if sanitized.is_empty() {
        "Untitled".to_string()
    } else {
        sanitized
    }
}

fn migrate_legacy_series_routing(input: &str) -> Option<String> {
    let legacy = "routing = \"Series\"";
    input
        .contains(legacy)
        .then(|| input.replacen(legacy, "[routing.Series]\nmix_a = 0.5\nmix_b = 0.5", 1))
}

#[derive(Debug, Clone, Copy)]
struct ResonatorPatchMigration;

impl TomlPatchMigration<ResonatorSynthPatch> for ResonatorPatchMigration {
    fn migrate_legacy(&self, input: &str, _error: &toml::de::Error) -> Option<String> {
        migrate_legacy_series_routing(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilterMode, OutputConfig, ResonatorConfig, ResonatorRouting, WaveguideStyle};

    #[test]
    fn patch_toml_roundtrip() {
        let patch = ResonatorSynthPatch {
            name: "Patch TOML".to_string(),
            output: OutputConfig {
                filter_mode: FilterMode::HighPass,
                filter_cutoff: 1_200.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        };

        let encoded = to_toml_string(&patch).unwrap();
        let decoded = from_toml_str(&encoded).unwrap();

        assert_eq!(decoded.name, patch.name);
        assert_eq!(decoded.output.filter_mode, FilterMode::HighPass);
        assert_eq!(decoded.output.filter_cutoff, 1_200.0);
    }

    #[test]
    fn legacy_unit_series_routing_toml_loads_with_default_mix() {
        let input = legacy_toml_string(&ResonatorSynthPatch::default());
        let legacy = input.replace(
            "[routing.Parallel]\nmix_a = 1.0\nmix_b = 0.0",
            "routing = \"Series\"",
        );

        let decoded = from_toml_str(&legacy).unwrap();

        assert!(matches!(
            decoded.routing,
            ResonatorRouting::Series { mix_a, mix_b }
                if (mix_a - 0.5).abs() < 0.001 && (mix_b - 0.5).abs() < 0.001
        ));
    }

    #[test]
    fn legacy_waveguide_patch_loads_with_default_style_fields() {
        let input = legacy_toml_string(&ResonatorSynthPatch::default());
        let legacy = input
            .replace("style = \"String\"\n", "")
            .replace("boundary_reflection = 0.75\n", "");

        let decoded = from_toml_str(&legacy).unwrap();

        let ResonatorConfig::Waveguide(config) = decoded.resonator_b else {
            panic!("default resonator B should remain waveguide");
        };
        assert_eq!(config.style, WaveguideStyle::String);
        assert!((config.boundary_reflection - 0.75).abs() < 0.001);
    }

    #[test]
    fn library_patch_save_load_preserves_sample_references() {
        let root = std::env::temp_dir().join(format!(
            "lindelion-patch-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = lindelion_sample_library::LibraryPaths::from_root(root);
        let reference = lindelion_sample_library::SampleReference::new(
            "0123456789abcdef0123456789abcdef",
            "Samples/incoming/excitation.wav",
        );
        let mut patch = ResonatorSynthPatch {
            name: "Library Patch".to_string(),
            ..ResonatorSynthPatch::default()
        };
        patch.excitation_slots[0].sample = Some(reference.clone());

        let path = save_library_patch(&paths, &patch).unwrap();
        let restored = load_library_patch(&path).unwrap();

        assert!(path.ends_with("Library Patch.toml"));
        assert_eq!(restored.name, "Library Patch");
        assert_eq!(restored.excitation_slots[0].sample, Some(reference));
    }

    #[test]
    fn plugin_state_roundtrips_through_shared_patch_format() {
        let patch = ResonatorSynthPatch {
            name: "State Patch".to_string(),
            ..ResonatorSynthPatch::default()
        };

        let state = to_plugin_state(&patch).unwrap();
        let restored = from_plugin_state(state).unwrap();

        assert_eq!(restored.name, "State Patch");
    }

    #[test]
    fn malformed_patch_returns_typed_error() {
        let error = from_toml_str("not valid =").unwrap_err();

        assert!(matches!(error, PatchIoError::Decode(_)));
    }

    #[test]
    fn forward_version_fails_cleanly() {
        let input = "format_version = 99\n[patch]\nname = \"Future\"\n";

        let error = from_toml_str(input).unwrap_err();

        assert!(matches!(
            error,
            PatchIoError::UnsupportedVersion {
                found: 99,
                supported: FORMAT_VERSION
            }
        ));
    }

    fn legacy_toml_string(patch: &ResonatorSynthPatch) -> String {
        toml::to_string_pretty(patch).unwrap()
    }
}
