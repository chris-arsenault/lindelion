use std::{
    fs, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::PluginState;

#[derive(Debug)]
pub enum TomlPatchError {
    Io(io::Error),
    Encode(toml::ser::Error),
    Decode(toml::de::Error),
    Utf8(std::str::Utf8Error),
    UnsupportedVersion { found: u32, supported: u32 },
}

impl From<io::Error> for TomlPatchError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<toml::ser::Error> for TomlPatchError {
    fn from(value: toml::ser::Error) -> Self {
        Self::Encode(value)
    }
}

impl From<toml::de::Error> for TomlPatchError {
    fn from(value: toml::de::Error) -> Self {
        Self::Decode(value)
    }
}

impl From<std::str::Utf8Error> for TomlPatchError {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::Utf8(value)
    }
}

pub trait TomlPatchMigration<T> {
    fn migrate_legacy(&self, _input: &str, _error: &toml::de::Error) -> Option<String> {
        None
    }

    fn migrate_version(
        &self,
        _found_version: u32,
        _input: &str,
    ) -> Result<Option<T>, TomlPatchError> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoPatchMigration;

impl<T> TomlPatchMigration<T> for NoPatchMigration {}

#[derive(Debug, PartialEq, Eq)]
pub struct TomlPatchFormat<T> {
    current_version: u32,
    _marker: PhantomData<T>,
}

impl<T> Clone for TomlPatchFormat<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TomlPatchFormat<T> {}

impl<T> TomlPatchFormat<T>
where
    T: Serialize + DeserializeOwned,
{
    pub const fn new(current_version: u32) -> Self {
        Self {
            current_version,
            _marker: PhantomData,
        }
    }

    pub const fn current_version(self) -> u32 {
        self.current_version
    }

    pub fn to_toml_string(self, patch: &T) -> Result<String, TomlPatchError> {
        toml::to_string_pretty(&TomlPatchEnvelopeRef {
            format_version: self.current_version,
            patch,
        })
        .map_err(TomlPatchError::from)
    }

    pub fn from_toml_str(self, input: &str) -> Result<T, TomlPatchError> {
        self.from_toml_str_with_migration(input, NoPatchMigration)
    }

    pub fn from_toml_str_with_migration<M>(
        self,
        input: &str,
        migration: M,
    ) -> Result<T, TomlPatchError>
    where
        M: TomlPatchMigration<T>,
    {
        match toml::from_str::<TomlPatchHeader>(input) {
            Ok(header) => self.decode_versioned(input, header.format_version, migration),
            Err(_) => self.decode_legacy(input, migration),
        }
    }

    pub fn save_patch(self, path: impl AsRef<Path>, patch: &T) -> Result<(), TomlPatchError> {
        self.write_atomic(path, self.to_toml_string(patch)?.as_bytes())
    }

    pub fn load_patch(self, path: impl AsRef<Path>) -> Result<T, TomlPatchError> {
        self.from_toml_str(&fs::read_to_string(path)?)
    }

    pub fn load_patch_with_migration<M>(
        self,
        path: impl AsRef<Path>,
        migration: M,
    ) -> Result<T, TomlPatchError>
    where
        M: TomlPatchMigration<T>,
    {
        self.from_toml_str_with_migration(&fs::read_to_string(path)?, migration)
    }

    pub fn to_plugin_state(self, patch: &T) -> Result<PluginState, TomlPatchError> {
        Ok(PluginState {
            format_version: self.current_version,
            payload: self.to_toml_string(patch)?.into_bytes(),
        })
    }

    pub fn from_plugin_state(self, state: PluginState) -> Result<T, TomlPatchError> {
        self.from_plugin_state_with_migration(state, NoPatchMigration)
    }

    pub fn from_plugin_state_with_migration<M>(
        self,
        state: PluginState,
        migration: M,
    ) -> Result<T, TomlPatchError>
    where
        M: TomlPatchMigration<T>,
    {
        if state.format_version > self.current_version {
            return Err(TomlPatchError::UnsupportedVersion {
                found: state.format_version,
                supported: self.current_version,
            });
        }

        let payload = std::str::from_utf8(&state.payload)?;
        self.from_toml_str_with_migration(payload, migration)
    }

    pub fn write_atomic(self, path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), TomlPatchError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = temporary_path_for(path);
        fs::write(&temp_path, bytes)?;
        fs::rename(&temp_path, path).map_err(|error| {
            let _ = fs::remove_file(&temp_path);
            TomlPatchError::Io(error)
        })
    }

    fn decode_versioned<M>(
        self,
        input: &str,
        found_version: u32,
        migration: M,
    ) -> Result<T, TomlPatchError>
    where
        M: TomlPatchMigration<T>,
    {
        if found_version > self.current_version {
            return Err(TomlPatchError::UnsupportedVersion {
                found: found_version,
                supported: self.current_version,
            });
        }

        if found_version == self.current_version {
            return toml::from_str::<TomlPatchEnvelope<T>>(input)
                .map(|envelope| envelope.patch)
                .map_err(TomlPatchError::from);
        }

        if let Some(patch) = migration.migrate_version(found_version, input)? {
            return Ok(patch);
        }

        Err(TomlPatchError::UnsupportedVersion {
            found: found_version,
            supported: self.current_version,
        })
    }

    fn decode_legacy<M>(self, input: &str, migration: M) -> Result<T, TomlPatchError>
    where
        M: TomlPatchMigration<T>,
    {
        match toml::from_str::<T>(input) {
            Ok(patch) => Ok(patch),
            Err(error) => {
                let Some(migrated) = migration.migrate_legacy(input, &error) else {
                    return Err(TomlPatchError::Decode(error));
                };
                toml::from_str(&migrated).map_err(TomlPatchError::from)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct TomlPatchHeader {
    format_version: u32,
}

#[derive(Debug, Deserialize)]
struct TomlPatchEnvelope<T> {
    patch: T,
}

#[derive(Debug, Serialize)]
struct TomlPatchEnvelopeRef<'a, T> {
    format_version: u32,
    patch: &'a T,
}

fn temporary_path_for(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let process = std::process::id();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("patch");
    path.with_file_name(format!(".{file_name}.{process}.{stamp}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct SyntheticPatch {
        name: String,
        amount: i32,
    }

    #[derive(Debug, Deserialize)]
    struct V0Envelope {
        patch: V0Patch,
    }

    #[derive(Debug, Deserialize)]
    struct V0Patch {
        name: String,
    }

    struct SyntheticMigration;

    impl TomlPatchMigration<SyntheticPatch> for SyntheticMigration {
        fn migrate_version(
            &self,
            found_version: u32,
            input: &str,
        ) -> Result<Option<SyntheticPatch>, TomlPatchError> {
            if found_version != 0 {
                return Ok(None);
            }

            let envelope: V0Envelope = toml::from_str(input)?;
            Ok(Some(SyntheticPatch {
                name: envelope.patch.name,
                amount: 0,
            }))
        }
    }

    #[test]
    fn synthetic_patch_round_trips_through_versioned_toml() {
        let format = TomlPatchFormat::<SyntheticPatch>::new(1);
        let patch = SyntheticPatch {
            name: "Synthetic".to_string(),
            amount: 7,
        };

        let encoded = format.to_toml_string(&patch).unwrap();
        let decoded = format.from_toml_str(&encoded).unwrap();

        assert!(encoded.contains("format_version = 1"));
        assert_eq!(decoded, patch);
    }

    #[test]
    fn malformed_patch_returns_typed_decode_error() {
        let format = TomlPatchFormat::<SyntheticPatch>::new(1);

        let error = format.from_toml_str("not valid =").unwrap_err();

        assert!(matches!(error, TomlPatchError::Decode(_)));
    }

    #[test]
    fn forward_version_fails_before_payload_decode() {
        let format = TomlPatchFormat::<SyntheticPatch>::new(1);
        let input = "format_version = 99\n[patch]\nname = \"Future\"\n";

        let error = format.from_toml_str(input).unwrap_err();

        assert!(matches!(
            error,
            TomlPatchError::UnsupportedVersion {
                found: 99,
                supported: 1
            }
        ));
    }

    #[test]
    fn older_version_uses_migration_hook() {
        let format = TomlPatchFormat::<SyntheticPatch>::new(1);
        let input = "format_version = 0\n[patch]\nname = \"Old\"\n";

        let decoded = format
            .from_toml_str_with_migration(input, SyntheticMigration)
            .unwrap();

        assert_eq!(
            decoded,
            SyntheticPatch {
                name: "Old".to_string(),
                amount: 0
            }
        );
    }

    #[test]
    fn plugin_state_round_trips_payload() {
        let format = TomlPatchFormat::<SyntheticPatch>::new(1);
        let patch = SyntheticPatch {
            name: "State".to_string(),
            amount: 42,
        };

        let state = format.to_plugin_state(&patch).unwrap();
        let decoded = format.from_plugin_state(state).unwrap();

        assert_eq!(decoded, patch);
    }

    #[test]
    fn atomic_write_creates_parent_and_replaces_file() {
        let format = TomlPatchFormat::<SyntheticPatch>::new(1);
        let root = std::env::temp_dir().join(format!(
            "ahara-shell-patch-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("nested").join("patch.toml");

        format.write_atomic(&path, b"first").unwrap();
        format.write_atomic(&path, b"second").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        let _ = fs::remove_dir_all(root);
    }
}
