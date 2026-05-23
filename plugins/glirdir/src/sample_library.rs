use std::{
    fmt, fs, io,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use lindelion_midi::{RootNote, Scale};
use lindelion_sample_library::{FileSampleLibrary, LibraryPaths, SampleLibrary, SampleMetadata};

use crate::{GlirdirPatch, ScratchpadAudio};

const DEFAULT_LIBRARY_DIR: &str = "Ahara";
const SAMPLE_LIBRARY_SAVE_MAGIC: [u8; 4] = *b"GLS1";
const TEMP_SAMPLE_DIR: &str = "lindelion-glirdir-sample-save";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleLibrarySaveStatus {
    Idle,
    Saving,
    Saved,
    EmptyScratchpad,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SampleLibrarySavePayload {
    pub status: SampleLibrarySaveStatus,
    pub message: String,
}

impl SampleLibrarySavePayload {
    pub fn saved(filename: &str) -> Self {
        Self {
            status: SampleLibrarySaveStatus::Saved,
            message: format!("Saved {filename}"),
        }
    }

    pub fn empty_scratchpad() -> Self {
        Self {
            status: SampleLibrarySaveStatus::EmptyScratchpad,
            message: "No scratchpad to save".to_string(),
        }
    }

    pub fn error(error: impl fmt::Display) -> Self {
        Self {
            status: SampleLibrarySaveStatus::Error,
            message: format!("Save failed: {error}"),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let message = self.message.as_bytes();
        let mut payload =
            Vec::with_capacity(SAMPLE_LIBRARY_SAVE_MAGIC.len() + 1 + 4 + message.len());
        payload.extend_from_slice(&SAMPLE_LIBRARY_SAVE_MAGIC);
        payload.push(status_id(self.status));
        payload.extend_from_slice(&(message.len() as u32).to_le_bytes());
        payload.extend_from_slice(message);
        payload
    }

    pub fn decode(payload: &[u8]) -> Option<Self> {
        if payload.len() < 9 || payload[..4] != SAMPLE_LIBRARY_SAVE_MAGIC {
            return None;
        }
        let status = status_from_id(payload[4])?;
        let message_len = u32::from_le_bytes(payload[5..9].try_into().ok()?) as usize;
        let message_end = 9usize.checked_add(message_len)?;
        let message = std::str::from_utf8(payload.get(9..message_end)?).ok()?;
        (message_end == payload.len()).then(|| Self {
            status,
            message: message.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SampleLibrarySaveJob {
    pub sequence: u64,
    scratchpad: ScratchpadAudio,
    file_name: String,
    library_root: PathBuf,
}

impl SampleLibrarySaveJob {
    pub fn new(sequence: u64, patch: &GlirdirPatch) -> Result<Self, SampleLibrarySavePayload> {
        Self::with_library_root(sequence, patch, default_library_root())
    }

    pub fn with_library_root(
        sequence: u64,
        patch: &GlirdirPatch,
        library_root: PathBuf,
    ) -> Result<Self, SampleLibrarySavePayload> {
        let Some(scratchpad) = patch.scratchpad.clone() else {
            return Err(SampleLibrarySavePayload::empty_scratchpad());
        };
        if scratchpad.samples.is_empty() {
            return Err(SampleLibrarySavePayload::empty_scratchpad());
        }

        Ok(Self {
            sequence,
            file_name: sample_file_name(patch, &scratchpad),
            scratchpad,
            library_root,
        })
    }

    pub fn save(self) -> SampleLibrarySavePayload {
        match self.save_inner() {
            Ok(metadata) => SampleLibrarySavePayload::saved(&metadata.filename),
            Err(error) => SampleLibrarySavePayload::error(error),
        }
    }

    fn save_inner(&self) -> Result<SampleMetadata, SampleLibrarySaveError> {
        let source = unique_temp_wav_path(&self.file_name)?;
        let result = (|| {
            write_wav_mono(&source, &self.scratchpad)?;
            let paths = LibraryPaths::from_root(self.library_root.clone());
            let mut library = FileSampleLibrary::open(paths)?;
            Ok(library.ingest(source.clone())?)
        })();
        let _ = remove_temp_source(&source);
        result
    }
}

fn default_library_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Music")
        .join(DEFAULT_LIBRARY_DIR)
}

fn sample_file_name(patch: &GlirdirPatch, scratchpad: &ScratchpadAudio) -> String {
    let key = format!(
        "{}{}",
        root_label(patch.quantize.root),
        scale_label(&patch.quantize.scale)
    );
    let metadata = scratchpad.metadata;
    sanitize_sample_file_name(&format!(
        "glirdir-{key}-{}bar-{}bpm.wav",
        metadata.capture_bars, metadata.bpm
    ))
}

fn sanitize_sample_file_name(input: &str) -> String {
    let mut file_name = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if !file_name.ends_with(".wav") {
        file_name.push_str(".wav");
    }
    if file_name == ".wav" {
        "glirdir.wav".to_string()
    } else {
        file_name
    }
}

fn unique_temp_wav_path(file_name: &str) -> io::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir()
        .join(TEMP_SAMPLE_DIR)
        .join(format!("{}-{timestamp}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir.join(file_name))
}

fn write_wav_mono(path: &Path, scratchpad: &ScratchpadAudio) -> io::Result<()> {
    let sample_rate = scratchpad.sample_rate.max(1);
    let data_len = checked_data_len(scratchpad.samples.len())?;
    let mut file = fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for sample in &scratchpad.samples {
        file.write_all(&pcm16(*sample).to_le_bytes())?;
    }
    Ok(())
}

fn checked_data_len(samples: usize) -> io::Result<u32> {
    samples
        .checked_mul(2)
        .and_then(|len| u32::try_from(len).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "scratchpad is too large"))
}

fn pcm16(sample: f32) -> i16 {
    let sample = if sample.is_finite() { sample } else { 0.0 };
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn remove_temp_source(path: &Path) -> io::Result<()> {
    fs::remove_file(path)?;
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}

fn root_label(root: RootNote) -> &'static str {
    match root {
        RootNote::C => "C",
        RootNote::CSharp => "Cs",
        RootNote::D => "D",
        RootNote::DSharp => "Ds",
        RootNote::E => "E",
        RootNote::F => "F",
        RootNote::FSharp => "Fs",
        RootNote::G => "G",
        RootNote::GSharp => "Gs",
        RootNote::A => "A",
        RootNote::ASharp => "As",
        RootNote::B => "B",
    }
}

fn scale_label(scale: &Scale) -> &'static str {
    match scale {
        Scale::Chromatic => "chrom",
        Scale::Major => "maj",
        Scale::NaturalMinor => "min",
        Scale::HarmonicMinor => "harm-min",
        Scale::MelodicMinor => "mel-min",
        Scale::PentatonicMajor => "pent-maj",
        Scale::PentatonicMinor => "pent-min",
        Scale::Blues => "blues",
        Scale::Dorian => "dor",
        Scale::Mixolydian => "mix",
        Scale::Custom(_) => "custom",
    }
}

fn status_id(status: SampleLibrarySaveStatus) -> u8 {
    match status {
        SampleLibrarySaveStatus::Idle => 0,
        SampleLibrarySaveStatus::Saving => 1,
        SampleLibrarySaveStatus::Saved => 2,
        SampleLibrarySaveStatus::EmptyScratchpad => 3,
        SampleLibrarySaveStatus::Error => 4,
    }
}

fn status_from_id(id: u8) -> Option<SampleLibrarySaveStatus> {
    match id {
        0 => Some(SampleLibrarySaveStatus::Idle),
        1 => Some(SampleLibrarySaveStatus::Saving),
        2 => Some(SampleLibrarySaveStatus::Saved),
        3 => Some(SampleLibrarySaveStatus::EmptyScratchpad),
        4 => Some(SampleLibrarySaveStatus::Error),
        _ => None,
    }
}

#[derive(Debug)]
enum SampleLibrarySaveError {
    Io(io::Error),
    Library(lindelion_sample_library::SampleLibraryError),
}

impl fmt::Display for SampleLibrarySaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Library(error) => write!(formatter, "{error}"),
        }
    }
}

impl From<io::Error> for SampleLibrarySaveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<lindelion_sample_library::SampleLibraryError> for SampleLibrarySaveError {
    fn from(value: lindelion_sample_library::SampleLibraryError) -> Self {
        Self::Library(value)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use lindelion_midi::{QuantizeSettings, RootNote, Scale};
    use lindelion_sample_library::{SampleLibrary, SampleResolution};

    use super::*;
    use crate::patch::ScratchpadMetadata;

    #[test]
    fn save_job_ingests_scratchpad_into_shared_library() {
        let root = temp_root("save");
        let library_root = root.join("Library");
        let patch = GlirdirPatch {
            quantize: QuantizeSettings {
                root: RootNote::C,
                scale: Scale::NaturalMinor,
                ..QuantizeSettings::default()
            },
            scratchpad: Some(ScratchpadAudio::with_metadata(
                48_000,
                ScratchpadMetadata::new(120.0, 4, 4, 4),
                vec![0.0, 0.5, -0.5, 0.25],
            )),
            ..GlirdirPatch::default()
        };
        let job = SampleLibrarySaveJob::with_library_root(7, &patch, library_root.clone())
            .expect("scratchpad should create save job");

        let payload = job.save();

        assert_eq!(payload.status, SampleLibrarySaveStatus::Saved);
        let library = FileSampleLibrary::open(LibraryPaths::from_root(library_root)).unwrap();
        let samples = library.list_samples().unwrap();
        assert_eq!(samples.len(), 1);
        assert!(
            samples[0]
                .filename
                .ends_with("glirdir-Cmin-4bar-120bpm.wav")
        );
        assert_eq!(samples[0].sample_rate, 48_000);
        assert!(matches!(
            library.resolve(&samples[0].reference).unwrap(),
            SampleResolution::Found(path) if path.exists()
        ));
    }

    #[test]
    fn empty_scratchpad_returns_visible_failure() {
        let payload = SampleLibrarySaveJob::with_library_root(
            1,
            &GlirdirPatch::default(),
            temp_root("empty"),
        )
        .expect_err("empty patch should not create save job");

        assert_eq!(payload.status, SampleLibrarySaveStatus::EmptyScratchpad);
        assert_eq!(payload.message, "No scratchpad to save");
    }

    #[test]
    fn save_payload_roundtrips_status_and_message() {
        let payload = SampleLibrarySavePayload::saved("glirdir-Cmin-4bar-120bpm.wav");

        assert_eq!(
            SampleLibrarySavePayload::decode(&payload.encode()),
            Some(payload)
        );
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lindelion-glirdir-{name}-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
