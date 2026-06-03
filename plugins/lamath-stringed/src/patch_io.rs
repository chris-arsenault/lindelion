use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};

use crate::patch::StringPatch;

pub const FORMAT_VERSION: u32 = 1;

const FORMAT: TomlPatchFormat<StringPatch> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_plugin_state(patch: &StringPatch) -> Result<PluginState, TomlPatchError> {
    FORMAT.to_plugin_state(patch)
}

pub fn from_plugin_state(state: PluginState) -> Result<StringPatch, TomlPatchError> {
    FORMAT.from_plugin_state(state).map(StringPatch::sanitized)
}
