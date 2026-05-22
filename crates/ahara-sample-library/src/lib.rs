use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleHash(pub String);

impl SampleHash {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleReference {
    pub blake3_hash: SampleHash,
    pub last_known_path: PathBuf,
}

impl SampleReference {
    pub fn new(hash: impl Into<String>, last_known_path: impl Into<PathBuf>) -> Self {
        Self {
            blake3_hash: SampleHash::new(hash),
            last_known_path: last_known_path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleMetadata {
    pub reference: SampleReference,
    pub filename: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub rms_db: Option<f32>,
    pub peak_db: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPaths {
    pub root: PathBuf,
    pub samples: PathBuf,
    pub patches: PathBuf,
    pub index_db: PathBuf,
}

impl LibraryPaths {
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            samples: root.join("Samples"),
            patches: root.join("Patches"),
            index_db: root.join("index.db"),
            root,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleResolution {
    Found(PathBuf),
    Missing(SampleReference),
}

pub trait SampleLibrary {
    type Error;

    fn resolve(&self, reference: &SampleReference) -> Result<SampleResolution, Self::Error>;

    fn ingest(&mut self, path: PathBuf) -> Result<SampleMetadata, Self::Error>;
}
