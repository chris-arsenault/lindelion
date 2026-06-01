//! Host session model — selected devices, the ordered plugin chain, and app settings.
//!
//! The persisted, serializable state of a Galad session: which input/output devices are selected,
//! the ordered serial chain of VST3 plugin slots (path + per-slot bypass + opaque per-plugin
//! state), and app settings. Serialization mirrors the versioned-envelope shape of
//! `lindelion-plugin-shell`'s `patch_io` — host-locally; the host does not depend on the guest
//! plugin-shell crate. M0 round-trips in memory (serialize → deserialize); disk persistence and
//! restore-on-launch are M4.
//!
//! `dead_code`: the error-variant payloads are carried for `{:?}` diagnostics, and `to_session` is
//! the round-trip-tested inverse of `load_session` — both read as unused on the non-test build.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};

use serde::{Deserialize, Serialize};

/// Current on-disk format version for a persisted host session.
pub const SESSION_FORMAT_VERSION: u32 = 1;

/// A persisted Galad session: selected devices, the ordered plugin chain, and app settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HostSession {
    /// Selected input (capture) device, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<DeviceRef>,
    /// Selected output (render) device, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<DeviceRef>,
    /// The ordered serial chain of VST3 plugin slots (mic → chain → output).
    #[serde(default)]
    pub chain: Vec<ChainSlot>,
    /// Application settings.
    #[serde(default)]
    pub settings: AppSettings,
}

/// A stable, serializable reference to an audio device.
///
/// Stores the WASAPI endpoint id (stable across sessions) plus a human-readable name for display.
/// Resolved to a live device only at M2; M0 stores identifiers, not handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRef {
    /// Stable device identifier (WASAPI endpoint id).
    pub id: String,
    /// Human-readable device name for display.
    pub name: String,
}

/// One slot in the serial plugin chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainSlot {
    /// Filesystem path to the plugin's `.vst3` module.
    pub plugin_path: PathBuf,
    /// Whether this slot is bypassed (passed through without processing).
    #[serde(default)]
    pub bypassed: bool,
    /// Opaque per-plugin state captured from `IComponent::getState`, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PluginStateBlob>,
}

/// Opaque, versioned per-plugin state blob.
///
/// Mirrors `lindelion_plugin_shell::PluginState` (`{ format_version, payload }`): the host stores
/// the bytes verbatim, produced/consumed by the plugin's `IComponent` get/setState (wired at M4).
/// The payload is serialized as a base64 string so opaque binary survives the text (TOML) format
/// compactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginStateBlob {
    /// Plugin-defined state format version.
    pub format_version: u32,
    /// Opaque state bytes, base64-encoded in the persisted form.
    #[serde(with = "base64_bytes")]
    pub payload: Vec<u8>,
}

/// Application-level settings that should survive app relaunches even when no session file is used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Folders scanned for `.vst3` plugins (populated by the M6 scan UI).
    pub plugin_scan_dirs: Vec<PathBuf>,
    /// Last scanned plugin catalog. Restored at launch so startup does not load/probe plugin DLLs.
    pub plugin_catalog: Vec<CachedPluginEntry>,
    /// Post-chain master gain in decibels.
    pub master_gain_db: f32,
    /// Post-chain master mute.
    pub master_muted: bool,
}

/// One cached scanned-plugin row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPluginEntry {
    pub path: PathBuf,
    pub compatible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Errors from session (de)serialization.
#[derive(Debug)]
pub enum SessionError {
    /// Failure encoding the session to TOML.
    Encode(toml::ser::Error),
    /// Failure decoding the session from TOML.
    Decode(toml::de::Error),
    /// The persisted file is from a newer, unsupported format version.
    UnsupportedVersion { found: u32, supported: u32 },
}

/// Errors from reading/writing a session file.
#[derive(Debug)]
pub enum SessionIoError {
    /// Filesystem error.
    Io(io::Error),
    /// (De)serialization error.
    Format(SessionError),
}

impl From<io::Error> for SessionIoError {
    fn from(error: io::Error) -> Self {
        SessionIoError::Io(error)
    }
}

impl From<SessionError> for SessionIoError {
    fn from(error: SessionError) -> Self {
        SessionIoError::Format(error)
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            plugin_scan_dirs: Vec::new(),
            plugin_catalog: Vec::new(),
            master_gain_db: 0.0,
            master_muted: false,
        }
    }
}

impl AppSettings {
    /// Read app settings from Galad's default per-user settings file.
    pub fn load_default() -> Result<Self, SessionIoError> {
        match fs::read_to_string(Self::default_path()) {
            Ok(text) => toml::from_str(&text)
                .map_err(SessionError::Decode)
                .map_err(SessionIoError::Format),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(SessionIoError::Io(error)),
        }
    }

    /// Save app settings to Galad's default per-user settings file.
    pub fn save_default(&self) -> Result<(), SessionIoError> {
        let toml = toml::to_string_pretty(self)
            .map_err(SessionError::Encode)
            .map_err(SessionIoError::Format)?;
        write_atomic(&Self::default_path(), toml.as_bytes())?;
        Ok(())
    }

    /// Galad's default per-user settings path.
    pub fn default_path() -> PathBuf {
        app_config_dir().join("settings.toml")
    }
}

impl HostSession {
    /// Serialize to a versioned TOML string (`format_version` + a `[session]` table).
    pub fn to_toml_string(&self) -> Result<String, SessionError> {
        toml::to_string_pretty(&SessionEnvelopeRef {
            format_version: SESSION_FORMAT_VERSION,
            session: self,
        })
        .map_err(SessionError::Encode)
    }

    /// Deserialize from a versioned TOML string, rejecting newer formats before decoding the body.
    pub fn from_toml_str(input: &str) -> Result<Self, SessionError> {
        let header: SessionHeader = toml::from_str(input).map_err(SessionError::Decode)?;
        if header.format_version > SESSION_FORMAT_VERSION {
            return Err(SessionError::UnsupportedVersion {
                found: header.format_version,
                supported: SESSION_FORMAT_VERSION,
            });
        }

        let envelope: SessionEnvelope = toml::from_str(input).map_err(SessionError::Decode)?;
        Ok(envelope.session)
    }

    /// Write the session to `path` atomically (temp file + rename).
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SessionIoError> {
        let toml = self.to_toml_string()?;
        write_atomic(path.as_ref(), toml.as_bytes())?;
        Ok(())
    }

    /// Read and parse a session from `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SessionIoError> {
        let text = fs::read_to_string(path)?;
        Ok(Self::from_toml_str(&text)?)
    }
}

/// Write `bytes` to `path` via a temp file + rename (mirrors `patch_io::write_atomic`).
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let temp = temp_path_for(path);
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

fn temp_path_for(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session");
    path.with_file_name(format!(".{name}.{pid}.{count}.tmp"))
}

#[cfg(windows)]
fn app_config_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Galad")
}

#[cfg(not(windows))]
fn app_config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join("galad")
}

#[derive(Debug, Deserialize)]
struct SessionHeader {
    format_version: u32,
}

#[derive(Debug, Serialize)]
struct SessionEnvelopeRef<'a> {
    format_version: u32,
    session: &'a HostSession,
}

#[derive(Debug, Deserialize)]
struct SessionEnvelope {
    session: HostSession,
}

/// Serde codec for `Vec<u8>` payloads as a base64 string (standard alphabet, with padding).
mod base64_bytes {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(&encoded).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_saves_and_loads_from_disk() {
        let session = HostSession {
            input: Some(DeviceRef {
                id: "in".to_string(),
                name: "Mic".to_string(),
            }),
            output: Some(DeviceRef {
                id: "out".to_string(),
                name: "VB-CABLE".to_string(),
            }),
            chain: vec![
                ChainSlot {
                    plugin_path: PathBuf::from("/plugins/A.vst3"),
                    bypassed: true,
                    state: None,
                },
                ChainSlot {
                    plugin_path: PathBuf::from("/plugins/B.vst3"),
                    bypassed: false,
                    state: Some(PluginStateBlob {
                        format_version: 1,
                        payload: vec![9, 8, 7, 0, 255, 1],
                    }),
                },
            ],
            settings: AppSettings::default(),
        };

        let path =
            std::env::temp_dir().join(format!("galad-session-test-{}.toml", std::process::id()));
        session.save(&path).expect("save");
        let loaded = HostSession::load(&path).expect("load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded, session);
    }

    #[test]
    fn session_round_trips_through_versioned_toml() {
        let session = HostSession {
            input: Some(DeviceRef {
                id: "input-endpoint-0".to_string(),
                name: "Built-in Microphone".to_string(),
            }),
            output: Some(DeviceRef {
                id: "output-endpoint-7".to_string(),
                name: "VB-CABLE Input".to_string(),
            }),
            chain: vec![
                ChainSlot {
                    plugin_path: PathBuf::from("/plugins/Caloma.vst3"),
                    bypassed: true,
                    state: None,
                },
                ChainSlot {
                    plugin_path: PathBuf::from("/plugins/Lamath.vst3"),
                    bypassed: false,
                    state: Some(PluginStateBlob {
                        format_version: 3,
                        payload: vec![0u8, 1, 2, 250, 255, 42, 7, 128],
                    }),
                },
            ],
            settings: AppSettings {
                plugin_scan_dirs: vec![PathBuf::from("/plugins"), PathBuf::from("/more/plugins")],
                plugin_catalog: vec![CachedPluginEntry {
                    path: PathBuf::from("/plugins/Caloma.vst3"),
                    compatible: true,
                    reason: None,
                }],
                master_gain_db: -3.0,
                master_muted: true,
            },
        };

        let encoded = session.to_toml_string().expect("encode");
        let decoded = HostSession::from_toml_str(&encoded).expect("decode");

        assert_eq!(decoded, session);
        // The opaque per-plugin state survives byte-for-byte through the text format.
        assert_eq!(
            decoded.chain[1].state.as_ref().unwrap().payload,
            vec![0u8, 1, 2, 250, 255, 42, 7, 128]
        );
        assert!(encoded.contains("format_version"));
    }

    #[test]
    fn newer_format_version_is_rejected() {
        let input = format!(
            "format_version = {}\n[session]\n",
            SESSION_FORMAT_VERSION + 1
        );

        let error = HostSession::from_toml_str(&input).unwrap_err();

        assert!(matches!(
            error,
            SessionError::UnsupportedVersion {
                found,
                supported,
            } if found == SESSION_FORMAT_VERSION + 1 && supported == SESSION_FORMAT_VERSION
        ));
    }
}
