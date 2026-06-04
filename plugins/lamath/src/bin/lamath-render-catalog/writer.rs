use crate::{
    catalog::{self, CatalogCase},
    cli::RenderSelection,
    manifest::{self, ManifestRecord},
    render,
};
use lindelion_sample_library::{StereoPcm16WavError, write_wav_stereo_pcm16};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(crate) struct RenderReport {
    pub(crate) scope: String,
    pub(crate) output_root: PathBuf,
    pub(crate) wav_files: usize,
    pub(crate) total_bytes: u64,
    pub(crate) manifest_path: PathBuf,
    pub(crate) index_path: PathBuf,
    pub(crate) highest_peak_case: String,
    pub(crate) highest_peak_dbfs: f32,
    pub(crate) quietest_audible_case: String,
    pub(crate) quietest_rms_dbfs: f32,
}

#[derive(Debug)]
pub(crate) enum WriteError {
    EmptySelection,
    Io { path: PathBuf, kind: io::ErrorKind },
    Manifest(manifest::ManifestReadError),
    Render(render::RenderError),
    Wav(StereoPcm16WavError),
}

pub(crate) fn write_catalog(
    output_root: &Path,
    selection: &RenderSelection,
    cases: &[CatalogCase],
) -> Result<RenderReport, WriteError> {
    if cases.is_empty() {
        return Err(WriteError::EmptySelection);
    }
    fs::create_dir_all(output_root).map_err(|error| io_error(output_root, error))?;

    let manifest_path = output_root.join("manifest.toml");
    let index_path = output_root.join("index.md");
    let all_cases = catalog::catalog_cases();
    let existing_records = manifest::read_existing_records(&manifest_path, output_root, &all_cases)
        .map_err(WriteError::Manifest)?;
    let mut rendered_records = Vec::with_capacity(cases.len());
    let mut total_bytes = 0_u64;
    let mut highest_peak_case = String::new();
    let mut highest_peak_dbfs = f32::NEG_INFINITY;
    let mut quietest_audible_case = String::new();
    let mut quietest_rms_dbfs = f32::INFINITY;

    for case in cases {
        let rendered = render::render_case(case).map_err(WriteError::Render)?;
        let wav_path = output_root.join(case.relative_wav);
        if let Some(parent) = wav_path.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        }
        let metrics = write_wav_stereo_pcm16(
            &wav_path,
            &rendered.left,
            &rendered.right,
            catalog::CATALOG_SAMPLE_RATE,
        )
        .map_err(WriteError::Wav)?;
        debug_assert_eq!(metrics.frames, rendered.metrics.frames);

        total_bytes += metrics.file_bytes;
        if metrics.peak_dbfs > highest_peak_dbfs {
            highest_peak_dbfs = metrics.peak_dbfs;
            highest_peak_case = case.id.to_string();
        }
        if metrics.rms_dbfs < quietest_rms_dbfs {
            quietest_rms_dbfs = metrics.rms_dbfs;
            quietest_audible_case = case.id.to_string();
        }
        rendered_records.push(ManifestRecord {
            case: case.clone(),
            metrics,
        });
    }

    let scope = selection.to_string();
    let library_records = merged_records(&all_cases, existing_records, &rendered_records);
    let manifest = manifest::manifest_toml(catalog::catalog_groups(), &library_records, "library");
    let index = manifest::index_markdown(catalog::catalog_groups(), &library_records, "library");
    write_text(&manifest_path, &manifest)?;
    write_text(&index_path, &index)?;
    total_bytes += manifest.len() as u64 + index.len() as u64;

    Ok(RenderReport {
        scope,
        output_root: output_root.to_path_buf(),
        wav_files: rendered_records.len(),
        total_bytes,
        manifest_path,
        index_path,
        highest_peak_case,
        highest_peak_dbfs,
        quietest_audible_case,
        quietest_rms_dbfs,
    })
}

fn merged_records(
    catalog_cases: &[CatalogCase],
    existing_records: Vec<ManifestRecord>,
    rendered_records: &[ManifestRecord],
) -> Vec<ManifestRecord> {
    catalog_cases
        .iter()
        .filter_map(|case| {
            rendered_records
                .iter()
                .find(|record| record.case.id == case.id)
                .or_else(|| {
                    existing_records
                        .iter()
                        .find(|record| record.case.id == case.id)
                })
                .cloned()
        })
        .collect()
}

impl RenderReport {
    pub(crate) fn summary(&self) -> String {
        format!(
            "Lamath render catalog\nscope: {}\noutput: {}\nwavs: {}\ntotal bytes: {}\nmanifest: {}\nindex: {}\nhighest peak: {} {:.2} dBFS\nquietest audible: {} {:.2} dBFS",
            self.scope,
            self.output_root.display(),
            self.wav_files,
            self.total_bytes,
            self.manifest_path.display(),
            self.index_path.display(),
            self.highest_peak_case,
            self.highest_peak_dbfs,
            self.quietest_audible_case,
            self.quietest_rms_dbfs
        )
    }
}

fn write_text(path: &Path, text: &str) -> Result<(), WriteError> {
    fs::write(path, text).map_err(|error| io_error(path, error))
}

fn io_error(path: &Path, error: io::Error) -> WriteError {
    WriteError::Io {
        path: path.to_path_buf(),
        kind: error.kind(),
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySelection => write!(formatter, "no catalog cases selected"),
            Self::Io { path, kind } => {
                write!(formatter, "I/O failed for {}: {kind:?}", path.display())
            }
            Self::Manifest(error) => write!(formatter, "{error}"),
            Self::Render(error) => write!(formatter, "{error}"),
            Self::Wav(error) => write!(formatter, "WAV write failed: {error:?}"),
        }
    }
}

impl Error for WriteError {}
