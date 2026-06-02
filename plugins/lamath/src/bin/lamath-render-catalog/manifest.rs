use crate::catalog::{CATALOG_BLOCK_SIZE, CATALOG_SAMPLE_RATE, CatalogCase, CatalogGroup};
use lindelion_sample_library::StereoPcm16WavMetrics;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(crate) struct ManifestRecord {
    pub(crate) case: CatalogCase,
    pub(crate) metrics: StereoPcm16WavMetrics,
}

#[derive(Debug)]
pub(crate) enum ManifestReadError {
    Io { path: PathBuf, kind: io::ErrorKind },
    Parse { path: PathBuf, message: String },
    Schema { field: &'static str, value: String },
}

pub(crate) fn manifest_toml(
    groups: &[CatalogGroup],
    records: &[ManifestRecord],
    scope: &str,
) -> String {
    let mut output = format!(
        "schema_version = 1\nsample_rate = {CATALOG_SAMPLE_RATE}\nblock_size = {CATALOG_BLOCK_SIZE}\nscope = \"{}\"\n\n",
        escape(scope)
    );
    for group in selected_groups(groups, records) {
        output.push_str("[[groups]]\n");
        output.push_str(&format!("id = \"{}\"\n", group.id));
        output.push_str(&format!("directory = \"{}\"\n", group.directory));
        output.push_str(&format!("title = \"{}\"\n", escape(group.title)));
        output.push_str(&format!("question = \"{}\"\n\n", escape(group.question)));
    }
    for record in records {
        write_case_record(&mut output, record);
    }
    output
}

pub(crate) fn index_markdown(
    groups: &[CatalogGroup],
    records: &[ManifestRecord],
    scope: &str,
) -> String {
    let mut output = format!("# Lamath Review Render Catalog\n\nScope: `{scope}`\n\n");
    for group in selected_groups(groups, records) {
        output.push_str(&format!(
            "## {} {}\n\n",
            group.directory_number(),
            group.title
        ));
        output.push_str(group.question);
        output.push_str("\n\n");
        for record in records
            .iter()
            .filter(|record| record.case.group_id == group.id)
        {
            output.push_str(&format!(
                "- `{}` - {} - `{}`\n",
                record.case.id, record.case.title, record.case.relative_wav
            ));
            output.push_str(&format!("  Tags: {}\n", record.case.tags.join(", ")));
            output.push_str(&format!(
                "  Peak: {:.2} dBFS; RMS: {:.2} dBFS\n",
                record.metrics.peak_dbfs, record.metrics.rms_dbfs
            ));
        }
        output.push('\n');
    }
    output
}

pub(crate) fn read_existing_records(
    manifest_path: &Path,
    output_root: &Path,
    catalog_cases: &[CatalogCase],
) -> Result<Vec<ManifestRecord>, ManifestReadError> {
    let text = match fs::read_to_string(manifest_path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(ManifestReadError::Io {
                path: manifest_path.to_path_buf(),
                kind: error.kind(),
            });
        }
    };
    let manifest: StoredManifest =
        toml::from_str(&text).map_err(|error| ManifestReadError::Parse {
            path: manifest_path.to_path_buf(),
            message: error.to_string(),
        })?;
    validate_stored_manifest(&manifest)?;

    let stored_by_id: BTreeMap<_, _> = manifest
        .cases
        .into_iter()
        .map(|case| (case.id.clone(), case))
        .collect();
    Ok(catalog_cases
        .iter()
        .filter(|case| output_root.join(case.relative_wav).is_file())
        .filter_map(|case| {
            stored_by_id
                .get(case.id)
                .map(|stored| ManifestRecord::from_stored(case.clone(), stored))
        })
        .collect())
}

fn write_case_record(output: &mut String, record: &ManifestRecord) {
    output.push_str("[[cases]]\n");
    output.push_str(&format!("id = \"{}\"\n", record.case.id));
    output.push_str(&format!("title = \"{}\"\n", escape(record.case.title)));
    output.push_str(&format!("group_id = \"{}\"\n", record.case.group_id));
    output.push_str(&format!("wav = \"{}\"\n", record.case.relative_wav));
    output.push_str(&format!("tags = [{}]\n", quoted_list(record.case.tags)));
    output.push_str(&format!("frames = {}\n", record.metrics.frames));
    output.push_str(&format!(
        "duration_seconds = {:.6}\n",
        record.metrics.duration_seconds
    ));
    output.push_str(&format!("peak = {:.9}\n", record.metrics.peak));
    output.push_str(&format!("rms = {:.9}\n", record.metrics.rms));
    output.push_str(&format!("peak_dbfs = {:.6}\n", record.metrics.peak_dbfs));
    output.push_str(&format!("rms_dbfs = {:.6}\n", record.metrics.rms_dbfs));
    output.push_str(&format!("data_bytes = {}\n", record.metrics.data_bytes));
    output.push_str(&format!("file_bytes = {}\n\n", record.metrics.file_bytes));
}

fn selected_groups<'a>(
    groups: &'a [CatalogGroup],
    records: &[ManifestRecord],
) -> Vec<&'a CatalogGroup> {
    groups
        .iter()
        .filter(|group| {
            records
                .iter()
                .any(|record| record.case.group_id == group.id)
        })
        .collect()
}

fn quoted_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("\"{}\"", escape(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn validate_stored_manifest(manifest: &StoredManifest) -> Result<(), ManifestReadError> {
    if manifest.schema_version != 1 {
        return Err(ManifestReadError::Schema {
            field: "schema_version",
            value: manifest.schema_version.to_string(),
        });
    }
    if manifest.sample_rate != CATALOG_SAMPLE_RATE {
        return Err(ManifestReadError::Schema {
            field: "sample_rate",
            value: manifest.sample_rate.to_string(),
        });
    }
    if manifest.block_size != CATALOG_BLOCK_SIZE {
        return Err(ManifestReadError::Schema {
            field: "block_size",
            value: manifest.block_size.to_string(),
        });
    }
    Ok(())
}

impl ManifestRecord {
    fn from_stored(case: CatalogCase, stored: &StoredCase) -> Self {
        Self {
            case,
            metrics: StereoPcm16WavMetrics {
                sample_rate: CATALOG_SAMPLE_RATE,
                frames: stored.frames,
                duration_seconds: stored.duration_seconds,
                peak: stored.peak,
                rms: stored.rms,
                peak_dbfs: stored.peak_dbfs,
                rms_dbfs: stored.rms_dbfs,
                data_bytes: stored.data_bytes,
                file_bytes: stored.file_bytes,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct StoredManifest {
    schema_version: u32,
    sample_rate: u32,
    block_size: usize,
    #[serde(default)]
    cases: Vec<StoredCase>,
}

#[derive(Debug, Deserialize)]
struct StoredCase {
    id: String,
    frames: usize,
    duration_seconds: f64,
    peak: f32,
    rms: f32,
    peak_dbfs: f32,
    rms_dbfs: f32,
    data_bytes: u32,
    file_bytes: u64,
}

impl fmt::Display for ManifestReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, kind } => {
                write!(
                    formatter,
                    "manifest read failed for {}: {kind:?}",
                    path.display()
                )
            }
            Self::Parse { path, message } => {
                write!(
                    formatter,
                    "manifest parse failed for {}: {message}",
                    path.display()
                )
            }
            Self::Schema { field, value } => {
                write!(formatter, "manifest {field} is incompatible: {value}")
            }
        }
    }
}

impl Error for ManifestReadError {}

trait GroupDirectoryNumber {
    fn directory_number(&self) -> &str;
}

impl GroupDirectoryNumber for CatalogGroup {
    fn directory_number(&self) -> &str {
        self.directory.split('_').next().unwrap_or(self.directory)
    }
}
