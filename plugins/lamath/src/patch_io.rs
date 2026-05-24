use std::{
    fs,
    path::{Path, PathBuf},
};

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};
use lindelion_sample_library::LibraryPaths;

use crate::ResonatorSynthPatch;

pub const FORMAT_VERSION: u32 = 1;
pub type PatchIoError = TomlPatchError;

const PATCH_FORMAT: TomlPatchFormat<ResonatorSynthPatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_toml_string(patch: &ResonatorSynthPatch) -> Result<String, PatchIoError> {
    PATCH_FORMAT.to_toml_string(patch)
}

pub fn from_toml_str(input: &str) -> Result<ResonatorSynthPatch, PatchIoError> {
    PATCH_FORMAT.from_toml_str(input)
}

pub fn save_patch(path: impl AsRef<Path>, patch: &ResonatorSynthPatch) -> Result<(), PatchIoError> {
    PATCH_FORMAT.save_patch(path, patch)
}

pub fn load_patch(path: impl AsRef<Path>) -> Result<ResonatorSynthPatch, PatchIoError> {
    PATCH_FORMAT.load_patch(path)
}

pub fn to_plugin_state(patch: &ResonatorSynthPatch) -> Result<PluginState, PatchIoError> {
    PATCH_FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<ResonatorSynthPatch, PatchIoError> {
    PATCH_FORMAT.from_plugin_state(state)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioInputMode, FilterMode, LiveExcitationMode, OutputConfig};

    #[test]
    fn patch_toml_roundtrips_v2_surface() {
        let patch = v2_roundtrip_patch();

        let encoded = to_toml_string(&patch).unwrap();
        let decoded = from_toml_str(&encoded).unwrap();

        assert!(encoded.contains("format_version = 1"));
        assert!(encoded.contains("[patch.audio_input]"));
        assert!(encoded.contains("[patch.audio_expression.mapping]"));
        assert!(encoded.contains("[patch.note_detection]"));
        assert!(encoded.contains("[patch.live_excitation]"));
        assert_eq!(decoded.name, patch.name);
        assert_eq!(decoded.output.filter_mode, FilterMode::HighPass);
        assert_eq!(decoded.output.filter_cutoff, 1_200.0);
        assert_v2_surface_matches(&decoded);
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
        let patch = v2_roundtrip_patch();

        let state = to_plugin_state(&patch).unwrap();
        let restored = from_plugin_state(state).unwrap();

        assert_eq!(
            to_plugin_state(&patch).unwrap().format_version,
            FORMAT_VERSION
        );
        assert_eq!(restored.name, "Patch TOML");
        assert_v2_surface_matches(&restored);
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

    fn v2_roundtrip_patch() -> ResonatorSynthPatch {
        let mut patch = ResonatorSynthPatch {
            name: "Patch TOML".to_string(),
            output: OutputConfig {
                filter_mode: FilterMode::HighPass,
                filter_cutoff: 1_200.0,
                ..OutputConfig::default()
            },
            ..ResonatorSynthPatch::default()
        };
        patch.audio_input.mode = AudioInputMode::MidiPlusAudioCreatesNotes;
        patch.audio_expression.enabled = true;
        patch.audio_expression.mapping.pitch_bend_range_semitones = 12.0;
        patch.audio_expression.mapping.pressure_floor_rms = 0.04;
        patch.audio_expression.mapping.pressure_ceiling_rms = 0.5;
        patch.audio_expression.mapping.brightness_floor_hz = 900.0;
        patch.audio_expression.mapping.brightness_ceiling_hz = 9_000.0;
        patch.note_detection.onset_sensitivity = 0.75;
        patch.note_detection.note_release_floor_rms = 0.03;
        patch.note_detection.minimum_note_length_ms = 90.0;
        patch.note_detection.pitch_confidence = 0.8;
        patch.note_detection.velocity_amount = 0.6;
        patch.live_excitation.mode = LiveExcitationMode::ContinuousAndNoteLatched;
        patch.live_excitation.gain_db = -6.0;
        patch.live_excitation.latch_window_ms = 180.0;
        patch.live_excitation.latch_pre_roll_ms = 30.0;
        patch.live_excitation.latch_fade_ms = 10.0;
        patch
    }

    fn assert_v2_surface_matches(patch: &ResonatorSynthPatch) {
        assert_eq!(
            patch.audio_input.mode,
            AudioInputMode::MidiPlusAudioCreatesNotes
        );
        assert!(patch.audio_expression.enabled);
        assert!((patch.audio_expression.mapping.pitch_bend_range_semitones - 12.0).abs() < 0.001);
        assert!((patch.audio_expression.mapping.pressure_floor_rms - 0.04).abs() < 0.001);
        assert!((patch.audio_expression.mapping.pressure_ceiling_rms - 0.5).abs() < 0.001);
        assert!((patch.audio_expression.mapping.brightness_floor_hz - 900.0).abs() < 0.001);
        assert!((patch.audio_expression.mapping.brightness_ceiling_hz - 9_000.0).abs() < 0.001);
        assert!((patch.note_detection.onset_sensitivity - 0.75).abs() < 0.001);
        assert!((patch.note_detection.note_release_floor_rms - 0.03).abs() < 0.001);
        assert!((patch.note_detection.minimum_note_length_ms - 90.0).abs() < 0.001);
        assert!((patch.note_detection.pitch_confidence - 0.8).abs() < 0.001);
        assert!((patch.note_detection.velocity_amount - 0.6).abs() < 0.001);
        assert_eq!(
            patch.live_excitation.mode,
            LiveExcitationMode::ContinuousAndNoteLatched
        );
        assert!((patch.live_excitation.gain_db + 6.0).abs() < 0.001);
        assert!((patch.live_excitation.latch_window_ms - 180.0).abs() < 0.001);
        assert!((patch.live_excitation.latch_pre_roll_ms - 30.0).abs() < 0.001);
        assert!((patch.live_excitation.latch_fade_ms - 10.0).abs() < 0.001);
    }
}
