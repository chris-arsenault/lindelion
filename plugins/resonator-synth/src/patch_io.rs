use std::{fs, io, path::Path};

use crate::ResonatorSynthPatch;

#[derive(Debug)]
pub enum PatchIoError {
    Io(io::Error),
    Encode(toml::ser::Error),
    Decode(toml::de::Error),
}

impl From<io::Error> for PatchIoError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<toml::ser::Error> for PatchIoError {
    fn from(value: toml::ser::Error) -> Self {
        Self::Encode(value)
    }
}

impl From<toml::de::Error> for PatchIoError {
    fn from(value: toml::de::Error) -> Self {
        Self::Decode(value)
    }
}

pub fn to_toml_string(patch: &ResonatorSynthPatch) -> Result<String, PatchIoError> {
    toml::to_string_pretty(patch).map_err(PatchIoError::from)
}

pub fn from_toml_str(input: &str) -> Result<ResonatorSynthPatch, PatchIoError> {
    toml::from_str(input).map_err(PatchIoError::from)
}

pub fn save_patch(path: impl AsRef<Path>, patch: &ResonatorSynthPatch) -> Result<(), PatchIoError> {
    fs::write(path, to_toml_string(patch)?)?;
    Ok(())
}

pub fn load_patch(path: impl AsRef<Path>) -> Result<ResonatorSynthPatch, PatchIoError> {
    from_toml_str(&fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilterMode, OutputConfig};

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
}
