use std::{
    fs,
    path::{Path, PathBuf},
};

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};
use lindelion_sample_library::LibraryPaths;

use crate::patch::GlirdirPatch;

pub const FORMAT_VERSION: u32 = 1;
pub type PatchIoError = TomlPatchError;

const PATCH_FORMAT: TomlPatchFormat<GlirdirPatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_toml_string(patch: &GlirdirPatch) -> Result<String, PatchIoError> {
    PATCH_FORMAT.to_toml_string(patch)
}

pub fn from_toml_str(input: &str) -> Result<GlirdirPatch, PatchIoError> {
    PATCH_FORMAT.from_toml_str(input)
}

pub fn save_patch(path: impl AsRef<Path>, patch: &GlirdirPatch) -> Result<(), PatchIoError> {
    PATCH_FORMAT.save_patch(path, patch)
}

pub fn load_patch(path: impl AsRef<Path>) -> Result<GlirdirPatch, PatchIoError> {
    PATCH_FORMAT.load_patch(path)
}

pub fn to_plugin_state(patch: &GlirdirPatch) -> Result<PluginState, PatchIoError> {
    PATCH_FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<GlirdirPatch, PatchIoError> {
    PATCH_FORMAT.from_plugin_state(state)
}

pub fn save_library_patch(
    paths: &LibraryPaths,
    patch: &GlirdirPatch,
) -> Result<PathBuf, PatchIoError> {
    fs::create_dir_all(&paths.patches)?;
    let path = paths
        .patches
        .join(format!("{}.toml", sanitize_patch_name(&patch.name)));
    save_patch(&path, patch)?;
    Ok(path)
}

pub fn load_library_patch(path: impl AsRef<Path>) -> Result<GlirdirPatch, PatchIoError> {
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
    use crate::{CaptureBars, ScratchpadAudio};

    #[test]
    fn patch_toml_roundtrips_through_shared_format() {
        let patch = GlirdirPatch {
            name: "Glirdir State".to_string(),
            scratchpad: Some(ScratchpadAudio::new(48_000, vec![0.0, 0.25, -0.25])),
            ..GlirdirPatch::default()
        };

        let encoded = to_toml_string(&patch).unwrap();
        let decoded = from_toml_str(&encoded).unwrap();

        assert_eq!(decoded.name, "Glirdir State");
        assert_eq!(decoded.scratchpad.unwrap().samples.len(), 3);
    }

    #[test]
    fn plugin_state_roundtrips() {
        let patch = GlirdirPatch {
            capture: crate::CaptureSettings {
                bars: CaptureBars::Sixteen,
                ..crate::CaptureSettings::default()
            },
            scratchpad: Some(ScratchpadAudio::new(
                44_100,
                vec![0.0, 0.25, f32::NAN, f32::INFINITY],
            )),
            ..GlirdirPatch::default()
        };

        let state = to_plugin_state(&patch).unwrap();
        let restored = from_plugin_state(state).unwrap();

        assert_eq!(restored.capture.bars, CaptureBars::Sixteen);
        let scratchpad = restored.scratchpad.expect("scratchpad should roundtrip");
        assert_eq!(scratchpad.sample_rate, 44_100);
        assert_eq!(scratchpad.samples, vec![0.0, 0.25, 0.0, 0.0]);
    }
}
