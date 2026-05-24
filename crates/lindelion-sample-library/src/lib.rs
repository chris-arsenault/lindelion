use std::path::PathBuf;

#[cfg(feature = "wav-decoder")]
use std::{fmt, io};

use lindelion_dsp_utils::analysis::sanitize_audio_in_place;
use serde::{Deserialize, Serialize};

#[cfg(feature = "file-library")]
mod file_library;
#[cfg(feature = "file-library")]
pub use file_library::{FileSampleLibrary, SampleLibraryError};

pub const DEFAULT_AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;
pub const MAX_RUNTIME_AUDIO_SAMPLE_RATE_HZ: f32 = 384_000.0;

pub trait IntoAudioSampleRateHz {
    fn into_audio_sample_rate_hz(self) -> u32;
}

impl IntoAudioSampleRateHz for u32 {
    fn into_audio_sample_rate_hz(self) -> u32 {
        self.max(1)
    }
}

impl IntoAudioSampleRateHz for f32 {
    fn into_audio_sample_rate_hz(self) -> u32 {
        sanitize_runtime_audio_sample_rate(self).round() as u32
    }
}

impl IntoAudioSampleRateHz for f64 {
    fn into_audio_sample_rate_hz(self) -> u32 {
        let value = if self.is_finite() {
            self.round().clamp(1.0, f64::from(u32::MAX))
        } else {
            f64::from(DEFAULT_AUDIO_SAMPLE_RATE_HZ)
        };
        value as u32
    }
}

pub fn sanitize_runtime_audio_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() {
        sample_rate.clamp(1.0, MAX_RUNTIME_AUDIO_SAMPLE_RATE_HZ)
    } else {
        DEFAULT_AUDIO_SAMPLE_RATE_HZ as f32
    }
}

pub fn sanitize_audio_samples_in_place(samples: &mut [f32]) {
    sanitize_audio_in_place(samples);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnedMonoAudioBuffer {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl OwnedMonoAudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: impl IntoAudioSampleRateHz) -> Self {
        let mut samples = samples;
        sanitize_audio_samples_in_place(&mut samples);
        Self {
            sample_rate: sample_rate.into_audio_sample_rate_hz(),
            samples,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn runtime_sample_rate(&self) -> f32 {
        sanitize_runtime_audio_sample_rate(self.sample_rate as f32)
    }
}

#[cfg(feature = "wav-decoder")]
impl From<DecodedSample> for OwnedMonoAudioBuffer {
    fn from(value: DecodedSample) -> Self {
        Self::new(value.samples, value.sample_rate)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeMonoAudioBuffer {
    samples: Box<[f32]>,
    sample_rate: f32,
}

impl RuntimeMonoAudioBuffer {
    pub fn from_owned(buffer: OwnedMonoAudioBuffer) -> Self {
        Self {
            sample_rate: buffer.runtime_sample_rate(),
            samples: buffer.samples.into_boxed_slice(),
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Extends the sample slice lifetime for hosts that keep the owning
    /// `RuntimeMonoAudioBuffer` beside the runtime object that borrows it.
    ///
    /// # Safety
    ///
    /// The caller must ensure that every returned slice is discarded before
    /// this buffer is moved, replaced, or dropped.
    pub unsafe fn samples_with_static_lifetime(&self) -> &'static [f32] {
        unsafe { std::slice::from_raw_parts(self.samples.as_ptr(), self.samples.len()) }
    }
}

impl From<OwnedMonoAudioBuffer> for RuntimeMonoAudioBuffer {
    fn from(value: OwnedMonoAudioBuffer) -> Self {
        Self::from_owned(value)
    }
}

pub type LoadedMonoAudioSlots<const N: usize> = [Option<OwnedMonoAudioBuffer>; N];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedSampleLoadReport {
    pub loaded_slots: usize,
    pub missing_samples: Vec<SampleReference>,
}

#[cfg(feature = "wav-decoder")]
#[derive(Debug)]
pub enum ReferencedSampleLoadError<E> {
    Library(E),
    Decode {
        reference: SampleReference,
        path: PathBuf,
        source: SampleDecodeError,
    },
}

#[cfg(feature = "wav-decoder")]
pub fn load_referenced_mono_audio_from_library<'a, const N: usize, L, I>(
    references: I,
    library: &L,
) -> Result<
    (LoadedMonoAudioSlots<N>, ReferencedSampleLoadReport),
    ReferencedSampleLoadError<L::Error>,
>
where
    L: SampleLibrary,
    I: IntoIterator<Item = (usize, &'a SampleReference)>,
{
    let mut buffers = empty_loaded_mono_audio_slots();
    let mut missing_samples = Vec::new();
    let mut loaded_slots = 0;

    for (index, reference) in references {
        if index >= N {
            continue;
        }
        match library
            .resolve(reference)
            .map_err(ReferencedSampleLoadError::Library)?
        {
            SampleResolution::Found(path) => {
                let decoded =
                    decode_wav_mono(&path).map_err(|source| ReferencedSampleLoadError::Decode {
                        reference: reference.clone(),
                        path,
                        source,
                    })?;
                buffers[index] = Some(OwnedMonoAudioBuffer::from(decoded));
                loaded_slots += 1;
            }
            SampleResolution::Missing(reference) => missing_samples.push(reference),
        }
    }

    Ok((
        buffers,
        ReferencedSampleLoadReport {
            loaded_slots,
            missing_samples,
        },
    ))
}

#[cfg(feature = "wav-decoder")]
pub fn load_referenced_mono_audio_from_paths<'a, const N: usize, I>(
    references: I,
) -> (LoadedMonoAudioSlots<N>, ReferencedSampleLoadReport)
where
    I: IntoIterator<Item = (usize, &'a SampleReference)>,
{
    let mut buffers = empty_loaded_mono_audio_slots();
    let mut missing_samples = Vec::new();
    let mut loaded_slots = 0;

    for (index, reference) in references {
        if index >= N {
            continue;
        }
        match decode_wav_mono(&reference.last_known_path) {
            Ok(decoded) => {
                buffers[index] = Some(OwnedMonoAudioBuffer::from(decoded));
                loaded_slots += 1;
            }
            Err(_) => missing_samples.push(reference.clone()),
        }
    }

    (
        buffers,
        ReferencedSampleLoadReport {
            loaded_slots,
            missing_samples,
        },
    )
}

pub fn empty_loaded_mono_audio_slots<const N: usize>() -> LoadedMonoAudioSlots<N> {
    std::array::from_fn(|_| None)
}

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

#[cfg(feature = "wav-decoder")]
mod wav;
#[cfg(feature = "wav-decoder")]
pub use wav::decode_wav_mono;

#[cfg(all(test, feature = "file-library"))]
mod tests;
