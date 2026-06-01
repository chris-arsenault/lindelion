//! Patch persistence: encode/decode [`CalomaPatch`] through `lindelion-plugin-shell`'s versioned
//! TOML patch format. This is the normal-VST-patch mechanism shared with the other plugins.

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};

use crate::patch::CalomaPatch;

pub type PatchIoError = TomlPatchError;

/// On-disk/state format version for Calóma patches.
pub const FORMAT_VERSION: u32 = 1;

const PATCH_FORMAT: TomlPatchFormat<CalomaPatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_toml_string(patch: &CalomaPatch) -> Result<String, PatchIoError> {
    PATCH_FORMAT.to_toml_string(patch)
}

pub fn from_toml_str(input: &str) -> Result<CalomaPatch, PatchIoError> {
    PATCH_FORMAT.from_toml_str(input)
}

pub fn to_plugin_state(patch: &CalomaPatch) -> Result<PluginState, PatchIoError> {
    PATCH_FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<CalomaPatch, PatchIoError> {
    PATCH_FORMAT.from_plugin_state(state)
}
