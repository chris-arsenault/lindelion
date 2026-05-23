use std::path::PathBuf;

#[cfg(any(feature = "file-library", feature = "wav-decoder"))]
use std::{fmt, fs, io, path::Path};

#[cfg(feature = "file-library")]
use rusqlite::{Connection, params};

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
    pub waveform_preview: SampleWaveformPreview,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampleWaveformPoint {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleWaveformPreview {
    pub points: Vec<SampleWaveformPoint>,
}

impl SampleWaveformPreview {
    pub fn from_samples(samples: &[f32], target_points: usize) -> Self {
        if samples.is_empty() || target_points == 0 {
            return Self { points: Vec::new() };
        }

        let chunk_len = samples.len().div_ceil(target_points).max(1);
        let points = samples
            .chunks(chunk_len)
            .map(|chunk| {
                let (min, max, sum_squares) = chunk.iter().copied().fold(
                    (f32::INFINITY, f32::NEG_INFINITY, 0.0),
                    |(min, max, sum_squares), sample| {
                        (
                            min.min(sample),
                            max.max(sample),
                            sum_squares + sample * sample,
                        )
                    },
                );
                SampleWaveformPoint {
                    min,
                    max,
                    rms: (sum_squares / chunk.len() as f32).sqrt(),
                }
            })
            .collect();

        Self { points }
    }
}

#[cfg(feature = "wav-decoder")]
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSample {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
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

#[cfg(feature = "file-library")]
#[derive(Debug)]
pub enum SampleLibraryError {
    Io(io::Error),
    Sql(rusqlite::Error),
    InvalidPath(PathBuf),
    UnsupportedAudio(PathBuf),
}

#[cfg(feature = "file-library")]
impl fmt::Display for SampleLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "sample library I/O error: {error}"),
            Self::Sql(error) => write!(formatter, "sample library database error: {error}"),
            Self::InvalidPath(path) => write!(formatter, "invalid sample path: {}", path.display()),
            Self::UnsupportedAudio(path) => {
                write!(formatter, "unsupported audio file: {}", path.display())
            }
        }
    }
}

#[cfg(feature = "file-library")]
impl std::error::Error for SampleLibraryError {}

#[cfg(feature = "file-library")]
impl From<io::Error> for SampleLibraryError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(feature = "file-library")]
impl From<rusqlite::Error> for SampleLibraryError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[cfg(feature = "wav-decoder")]
#[derive(Debug)]
pub enum SampleDecodeError {
    Io(io::Error),
    UnsupportedAudio(PathBuf),
}

#[cfg(feature = "wav-decoder")]
impl fmt::Display for SampleDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "sample decode I/O error: {error}"),
            Self::UnsupportedAudio(path) => {
                write!(formatter, "unsupported audio file: {}", path.display())
            }
        }
    }
}

#[cfg(feature = "wav-decoder")]
impl std::error::Error for SampleDecodeError {}

#[cfg(feature = "wav-decoder")]
impl From<io::Error> for SampleDecodeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(feature = "file-library")]
impl From<SampleDecodeError> for SampleLibraryError {
    fn from(value: SampleDecodeError) -> Self {
        match value {
            SampleDecodeError::Io(error) => Self::Io(error),
            SampleDecodeError::UnsupportedAudio(path) => Self::UnsupportedAudio(path),
        }
    }
}

#[cfg(feature = "file-library")]
#[derive(Debug)]
pub struct FileSampleLibrary {
    paths: LibraryPaths,
    connection: Connection,
}

#[cfg(feature = "file-library")]
impl FileSampleLibrary {
    pub fn open(paths: LibraryPaths) -> Result<Self, SampleLibraryError> {
        fs::create_dir_all(&paths.root)?;
        fs::create_dir_all(&paths.samples)?;
        fs::create_dir_all(&paths.patches)?;
        let connection = Connection::open(&paths.index_db)?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS samples (
                blake3_hash TEXT PRIMARY KEY,
                relative_path TEXT NOT NULL,
                filename TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                sample_rate INTEGER NOT NULL,
                channels INTEGER NOT NULL,
                rms_db REAL,
                peak_db REAL
            );
            ",
        )?;

        Ok(Self { paths, connection })
    }

    pub const fn paths(&self) -> &LibraryPaths {
        &self.paths
    }

    pub fn waveform_preview(
        &self,
        reference: &SampleReference,
    ) -> Result<SampleWaveformPreview, SampleLibraryError> {
        let Some(audio) = self.decode(reference)? else {
            return Ok(SampleWaveformPreview { points: Vec::new() });
        };
        Ok(SampleWaveformPreview::from_samples(&audio.samples, 128))
    }

    pub fn list_samples(&self) -> Result<Vec<SampleMetadata>, SampleLibraryError> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                blake3_hash,
                relative_path,
                filename,
                duration_ms,
                sample_rate,
                channels,
                rms_db,
                peak_db
            FROM samples
            ORDER BY filename COLLATE NOCASE
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SampleMetadata {
                reference: SampleReference {
                    blake3_hash: SampleHash(row.get(0)?),
                    last_known_path: PathBuf::from(row.get::<_, String>(1)?),
                },
                filename: row.get(2)?,
                duration_ms: row.get::<_, i64>(3)?.max(0) as u64,
                sample_rate: row.get::<_, i64>(4)?.max(0) as u32,
                channels: row.get::<_, i64>(5)?.max(0) as u16,
                rms_db: row.get(6)?,
                peak_db: row.get(7)?,
                waveform_preview: SampleWaveformPreview { points: Vec::new() },
            })
        })?;

        let mut samples = Vec::new();
        for row in rows {
            let mut metadata = row?;
            metadata.waveform_preview = self.waveform_preview(&metadata.reference)?;
            samples.push(metadata);
        }
        Ok(samples)
    }

    pub fn decode(
        &self,
        reference: &SampleReference,
    ) -> Result<Option<DecodedSample>, SampleLibraryError> {
        match self.resolve(reference)? {
            SampleResolution::Found(path) => Ok(Some(decode_wav_mono(&path)?)),
            SampleResolution::Missing(_) => Ok(None),
        }
    }

    fn metadata_for_path(
        &self,
        hash: String,
        relative_path: PathBuf,
        source_path: &Path,
    ) -> Result<SampleMetadata, SampleLibraryError> {
        let audio = decode_wav_mono(source_path)?;
        let peak = audio
            .samples
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0, f32::max);
        let sum_squares = audio
            .samples
            .iter()
            .copied()
            .map(|sample| sample * sample)
            .sum::<f32>();
        let rms = if audio.samples.is_empty() {
            0.0
        } else {
            (sum_squares / audio.samples.len() as f32).sqrt()
        };
        let duration_ms = if audio.sample_rate == 0 {
            0
        } else {
            (audio.samples.len() as u64 * 1_000) / u64::from(audio.sample_rate)
        };

        Ok(SampleMetadata {
            reference: SampleReference {
                blake3_hash: SampleHash(hash),
                last_known_path: relative_path,
            },
            filename: source_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("sample")
                .to_string(),
            duration_ms,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            rms_db: amplitude_db(rms),
            peak_db: amplitude_db(peak),
            waveform_preview: SampleWaveformPreview::from_samples(&audio.samples, 128),
        })
    }

    fn insert_metadata(&self, metadata: &SampleMetadata) -> Result<(), SampleLibraryError> {
        self.connection.execute(
            "
            INSERT INTO samples (
                blake3_hash,
                relative_path,
                filename,
                duration_ms,
                sample_rate,
                channels,
                rms_db,
                peak_db
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(blake3_hash) DO UPDATE SET
                relative_path = excluded.relative_path,
                filename = excluded.filename,
                duration_ms = excluded.duration_ms,
                sample_rate = excluded.sample_rate,
                channels = excluded.channels,
                rms_db = excluded.rms_db,
                peak_db = excluded.peak_db
            ",
            params![
                metadata.reference.blake3_hash.0,
                metadata.reference.last_known_path.to_string_lossy(),
                metadata.filename,
                metadata.duration_ms,
                metadata.sample_rate,
                metadata.channels,
                metadata.rms_db,
                metadata.peak_db,
            ],
        )?;
        Ok(())
    }

    fn db_relative_path(&self, hash: &SampleHash) -> Result<Option<PathBuf>, SampleLibraryError> {
        let mut statement = self
            .connection
            .prepare("SELECT relative_path FROM samples WHERE blake3_hash = ?1")?;
        let mut rows = statement.query(params![hash.0])?;
        if let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            Ok(Some(PathBuf::from(path)))
        } else {
            Ok(None)
        }
    }

    fn absolute_from_reference(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            return path.to_path_buf();
        }
        self.paths.root.join(path)
    }

    fn update_relative_path(
        &self,
        hash: &SampleHash,
        path: &Path,
    ) -> Result<(), SampleLibraryError> {
        let relative = relative_to_root(&self.paths.root, path);
        self.connection.execute(
            "UPDATE samples SET relative_path = ?1 WHERE blake3_hash = ?2",
            params![relative.to_string_lossy(), hash.0],
        )?;
        Ok(())
    }

    fn find_by_hash(&self, hash: &SampleHash) -> Result<Option<PathBuf>, SampleLibraryError> {
        find_matching_file_by_hash(&self.paths.samples, hash)
    }
}

#[cfg(feature = "file-library")]
impl SampleLibrary for FileSampleLibrary {
    type Error = SampleLibraryError;

    fn resolve(&self, reference: &SampleReference) -> Result<SampleResolution, Self::Error> {
        let candidates = [
            self.db_relative_path(&reference.blake3_hash)?,
            Some(reference.last_known_path.clone()),
        ];

        for candidate in candidates.into_iter().flatten() {
            let absolute = self.absolute_from_reference(&candidate);
            if absolute.exists() && file_hash(&absolute)? == reference.blake3_hash {
                return Ok(SampleResolution::Found(absolute));
            }
        }

        if let Some(found) = self.find_by_hash(&reference.blake3_hash)? {
            self.update_relative_path(&reference.blake3_hash, &found)?;
            return Ok(SampleResolution::Found(found));
        }

        Ok(SampleResolution::Missing(reference.clone()))
    }

    fn ingest(&mut self, path: PathBuf) -> Result<SampleMetadata, Self::Error> {
        if !path.is_file() {
            return Err(SampleLibraryError::InvalidPath(path));
        }

        let hash = file_hash(&path)?.0;
        let filename = path
            .file_name()
            .ok_or_else(|| SampleLibraryError::InvalidPath(path.clone()))?;
        let target_dir = self.paths.samples.join("incoming");
        fs::create_dir_all(&target_dir)?;
        let target = target_dir.join(format!(
            "{}-{}",
            &hash[..hash.len().min(12)],
            filename.to_string_lossy()
        ));
        fs::copy(&path, &target)?;

        let relative = relative_to_root(&self.paths.root, &target);
        let metadata = self.metadata_for_path(hash, relative, &target)?;
        self.insert_metadata(&metadata)?;
        Ok(metadata)
    }
}

#[cfg(feature = "file-library")]
fn file_hash(path: &Path) -> Result<SampleHash, SampleLibraryError> {
    let bytes = fs::read(path)?;
    Ok(SampleHash(blake3::hash(&bytes).to_hex().to_string()))
}

#[cfg(feature = "file-library")]
fn find_matching_file_by_hash(
    root: &Path,
    hash: &SampleHash,
) -> Result<Option<PathBuf>, SampleLibraryError> {
    if !root.exists() {
        return Ok(None);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_matching_file_by_hash(&path, hash)? {
                return Ok(Some(found));
            }
        } else if path.is_file() && file_hash(&path)? == *hash {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

#[cfg(feature = "file-library")]
fn relative_to_root(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

#[cfg(feature = "file-library")]
fn amplitude_db(value: f32) -> Option<f32> {
    if value.is_finite() && value > 0.0 {
        Some(20.0 * value.log10())
    } else {
        None
    }
}

#[cfg(feature = "wav-decoder")]
pub fn decode_wav_mono(path: &Path) -> Result<DecodedSample, SampleDecodeError> {
    let bytes = fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf()));
    }

    let mut offset = 12;
    let mut format = None;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let len = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start.saturating_add(len).min(bytes.len());

        match id {
            b"fmt " if len >= 16 => {
                format = Some(WavFormat {
                    audio_format: u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                    channels: u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()),
                    sample_rate: u32::from_le_bytes(
                        bytes[start + 4..start + 8].try_into().unwrap(),
                    ),
                    bits_per_sample: u16::from_le_bytes(
                        bytes[start + 14..start + 16].try_into().unwrap(),
                    ),
                });
            }
            b"data" => data = Some((start, end)),
            _ => {}
        }

        offset = end + (len % 2);
    }

    let Some(format) = format else {
        return Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf()));
    };
    let Some((data_start, data_end)) = data else {
        return Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf()));
    };

    let frame_samples = decode_wav_samples(&bytes[data_start..data_end], format, path)?;
    let mono = mix_to_mono(&frame_samples, format.channels);
    Ok(DecodedSample {
        samples: mono,
        sample_rate: format.sample_rate,
        channels: format.channels,
    })
}

#[cfg(feature = "wav-decoder")]
#[derive(Debug, Clone, Copy)]
struct WavFormat {
    audio_format: u16,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

#[cfg(feature = "wav-decoder")]
fn decode_wav_samples(
    data: &[u8],
    format: WavFormat,
    path: &Path,
) -> Result<Vec<f32>, SampleDecodeError> {
    match (format.audio_format, format.bits_per_sample) {
        (1, 16) => Ok(data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes(chunk.try_into().unwrap()) as f32 / i16::MAX as f32)
            .collect()),
        (1, 24) => Ok(data.chunks_exact(3).map(decode_i24).collect()),
        (1, 32) => Ok(data
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()) as f32 / i32::MAX as f32)
            .collect()),
        (3, 32) => Ok(data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()).clamp(-1.0, 1.0))
            .collect()),
        _ => Err(SampleDecodeError::UnsupportedAudio(path.to_path_buf())),
    }
}

#[cfg(feature = "wav-decoder")]
fn decode_i24(chunk: &[u8]) -> f32 {
    let sign = if chunk[2] & 0x80 == 0 { 0 } else { 0xFF };
    let value = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], sign]);
    (value as f32 / 8_388_607.0).clamp(-1.0, 1.0)
}

#[cfg(feature = "wav-decoder")]
fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

#[cfg(all(test, feature = "file-library"))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Seek, Write},
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn file_library_ingests_hashes_indexes_and_previews_wav_samples() {
        let root = temp_root("ingest");
        let source = root.join("source.wav");
        write_test_wav(&source, &[0.0, 0.5, -0.5, 0.25]);
        let paths = LibraryPaths::from_root(root.join("Library"));
        let mut library = FileSampleLibrary::open(paths.clone()).unwrap();

        let metadata = library.ingest(source).unwrap();

        assert_ingested_metadata(&paths, &metadata);
        assert_resolves_existing_sample(&library, &metadata);
        let listed = library.list_samples().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].reference, metadata.reference);
        assert!(!listed[0].waveform_preview.points.is_empty());
    }

    fn assert_ingested_metadata(paths: &LibraryPaths, metadata: &SampleMetadata) {
        assert!(paths.index_db.exists());
        assert!(metadata.reference.blake3_hash.0.len() >= 32);
        assert_eq!(metadata.sample_rate, 48_000);
        assert_eq!(metadata.channels, 1);
        assert_eq!(metadata.duration_ms, 0);
        assert!(metadata.peak_db.unwrap() <= 0.0);
        assert!(metadata.rms_db.unwrap() < 0.0);
        assert!(!metadata.waveform_preview.points.is_empty());
    }

    fn assert_resolves_existing_sample(library: &FileSampleLibrary, metadata: &SampleMetadata) {
        assert!(matches!(
            library.resolve(&metadata.reference).unwrap(),
            SampleResolution::Found(path) if path.exists()
        ));
    }

    #[test]
    fn file_library_resolves_moved_samples_by_hash_and_reports_missing_samples() {
        let root = temp_root("resolve");
        let source = root.join("source.wav");
        write_test_wav(&source, &[0.25, -0.25, 0.75, -0.75]);
        let paths = LibraryPaths::from_root(root.join("Library"));
        let mut library = FileSampleLibrary::open(paths.clone()).unwrap();
        let metadata = library.ingest(source).unwrap();
        let original = match library.resolve(&metadata.reference).unwrap() {
            SampleResolution::Found(path) => path,
            SampleResolution::Missing(_) => panic!("expected sample to resolve"),
        };
        let moved = paths.samples.join("moved").join("source.wav");
        fs::create_dir_all(moved.parent().unwrap()).unwrap();
        fs::rename(&original, &moved).unwrap();

        let resolved = library.resolve(&metadata.reference).unwrap();
        assert_eq!(resolved, SampleResolution::Found(moved.clone()));

        fs::remove_file(moved).unwrap();
        let missing = library.resolve(&metadata.reference).unwrap();
        assert_eq!(missing, SampleResolution::Missing(metadata.reference));
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lindelion-{name}-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_test_wav(path: &Path, samples: &[f32]) {
        let mut file = fs::File::create(path).unwrap();
        let data_len = samples.len() as u32 * 2;
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&48_000u32.to_le_bytes()).unwrap();
        file.write_all(&(48_000u32 * 2).to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_len.to_le_bytes()).unwrap();
        for sample in samples {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            file.write_all(&pcm.to_le_bytes()).unwrap();
        }
        file.flush().unwrap();
        file.rewind().unwrap();
    }
}
