//! Editor-settings persistence: the durable editor-display settings encoded through
//! `lindelion-plugin-shell`'s versioned TOML state format — the shared normal-VST mechanism (mirrors
//! `caloma::patch_io`). The values are **UI-framework-neutral** discriminants/numbers; the editor
//! maps them to its `ViewMode`/`FreqScale`/`ColorMap`.

use std::sync::Mutex;

use lindelion_plugin_shell::{PluginState, TomlPatchError, TomlPatchFormat};
use serde::{Deserialize, Serialize};

/// State/format version for Cenedril editor settings.
pub const FORMAT_VERSION: u32 = 1;

/// The persisted editor-display settings. `#[serde(default)]` keeps older/newer payloads
/// forward-compatible (missing fields fall back to [`Default`], which reproduces the M5 display).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CenedrilEditorSettings {
    /// Active spectrogram view: `0` = Magnitude, `1` = Reassigned.
    pub active_view: u32,
    /// Frequency scale: `0` = Log, `1` = Linear.
    pub freq_scale: u32,
    /// Color map: `0` = Magma, `1` = Viridis, `2` = Grayscale.
    pub color_map: u32,
    /// dB display-window floor.
    pub db_floor: f32,
    /// dB display-window ceiling.
    pub db_ceil: f32,
}

impl Default for CenedrilEditorSettings {
    fn default() -> Self {
        Self {
            active_view: 0,
            freq_scale: 0,
            color_map: 0,
            db_floor: -100.0,
            db_ceil: 0.0,
        }
    }
}

const FORMAT: TomlPatchFormat<CenedrilEditorSettings> = TomlPatchFormat::new(FORMAT_VERSION);

pub fn to_plugin_state(settings: &CenedrilEditorSettings) -> Result<PluginState, TomlPatchError> {
    FORMAT.to_plugin_state(settings)
}

pub fn from_plugin_state(state: PluginState) -> Result<CenedrilEditorSettings, TomlPatchError> {
    FORMAT.from_plugin_state(state)
}

/// Off-audio-thread cell holding the current editor settings: written by the editor (per-field) and
/// by `load_state`, read by `state` and the editor on open. Not on the audio thread (ADR-0001), so a
/// `Mutex` is appropriate — no lock-free primitive needed.
pub struct SettingsCell {
    inner: Mutex<CenedrilEditorSettings>,
}

impl SettingsCell {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CenedrilEditorSettings::default()),
        }
    }

    pub fn get(&self) -> CenedrilEditorSettings {
        *self.inner.lock().expect("settings cell poisoned")
    }

    pub fn set(&self, settings: CenedrilEditorSettings) {
        *self.inner.lock().expect("settings cell poisoned") = settings;
    }

    pub fn set_active_view(&self, value: u32) {
        self.inner
            .lock()
            .expect("settings cell poisoned")
            .active_view = value;
    }

    pub fn set_freq_scale(&self, value: u32) {
        self.inner
            .lock()
            .expect("settings cell poisoned")
            .freq_scale = value;
    }

    pub fn set_color_map(&self, value: u32) {
        self.inner.lock().expect("settings cell poisoned").color_map = value;
    }

    pub fn set_db_floor(&self, value: f32) {
        self.inner.lock().expect("settings cell poisoned").db_floor = value;
    }

    pub fn set_db_ceil(&self, value: f32) {
        self.inner.lock().expect("settings cell poisoned").db_ceil = value;
    }
}

impl Default for SettingsCell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_plugin_state() {
        let s = CenedrilEditorSettings {
            active_view: 1,
            freq_scale: 1,
            color_map: 2,
            db_floor: -80.0,
            db_ceil: -6.0,
        };
        let state = to_plugin_state(&s).unwrap();
        assert_eq!(from_plugin_state(state).unwrap(), s);
    }

    #[test]
    fn serialized_settings_carry_the_format_version_envelope() {
        let state = to_plugin_state(&CenedrilEditorSettings::default()).unwrap();
        assert_eq!(state.format_version, FORMAT_VERSION);
        let toml = String::from_utf8(state.payload.clone()).unwrap();
        assert!(toml.contains("format_version"), "payload: {toml}");
    }
}
