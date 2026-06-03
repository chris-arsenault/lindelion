use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};

use crate::patch::TubePatch;

pub const FORMAT_VERSION: u32 = 1;

const FORMAT: TomlPatchFormat<TubePatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_plugin_state(patch: &TubePatch) -> Result<PluginState, TomlPatchError> {
    FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<TubePatch, TomlPatchError> {
    FORMAT.from_plugin_state(state).map(TubePatch::sanitized)
}
