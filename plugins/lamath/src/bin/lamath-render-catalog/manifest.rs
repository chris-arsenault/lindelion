use crate::catalog::{CATALOG_BLOCK_SIZE, CATALOG_SAMPLE_RATE, CatalogCase, CatalogGroup};
use crate::variant::{AxisCoord, case_axes, is_selector};
use lindelion_sample_library::StereoPcm16WavMetrics;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

/// Manifest format version. v2 adds the per-case `family` key and `axes` coordinates that drive
/// the review UI's A/B/C variant selectors. The audio metrics are unchanged from v1, so an
/// existing v1 manifest is still readable for non-destructive regeneration.
const MANIFEST_SCHEMA_VERSION: u32 = 2;

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
        "schema_version = {MANIFEST_SCHEMA_VERSION}\nsample_rate = {CATALOG_SAMPLE_RATE}\nblock_size = {CATALOG_BLOCK_SIZE}\nscope = \"{}\"\n\n",
        escape(scope)
    );
    for group in selected_groups(groups, records) {
        output.push_str("[[groups]]\n");
        output.push_str(&format!("id = \"{}\"\n", group.id));
        output.push_str(&format!("directory = \"{}\"\n", group.directory));
        output.push_str(&format!("title = \"{}\"\n", escape(group.title)));
        output.push_str(&format!("question = \"{}\"\n\n", escape(group.question)));
    }
    for resolved in resolve_cases(records) {
        write_case_record(
            &mut output,
            resolved.record,
            &resolved.family,
            &resolved.axes,
        );
    }
    output
}

/// A case paired with its resolved family key and the axes that will drive the UI selectors.
struct ResolvedCase<'a> {
    record: &'a ManifestRecord,
    family: String,
    axes: Vec<AxisCoord>,
}

/// Assign each case its family (row) key and selector axes. A row is the group plus its *context*
/// coordinates (resonator family, gesture, reference identity — see [`is_selector`]) plus the set
/// of selector axes present, so different instruments and gestures never share a row. Within a
/// row the *selector* axes form the A/B grid. If two cases in a row would land on the same selector
/// coordinate (bespoke phrases distinguished only by an unmodelled note pattern), that row degrades
/// to singleton rows — one per case — rather than hiding a render behind another.
fn resolve_cases(records: &[ManifestRecord]) -> Vec<ResolvedCase<'_>> {
    let row_keys: Vec<String> = records.iter().map(|record| row_key(&record.case)).collect();

    let mut members: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, key) in row_keys.iter().enumerate() {
        members.entry(key.as_str()).or_default().push(index);
    }
    let mut singletonize: BTreeSet<&str> = BTreeSet::new();
    for (key, indices) in &members {
        let mut seen = BTreeSet::new();
        for &index in indices {
            if !seen.insert(selector_coords(&records[index].case)) {
                singletonize.insert(key);
                break;
            }
        }
    }

    records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let key = &row_keys[index];
            let family = if singletonize.contains(key.as_str()) {
                record.case.id.to_string()
            } else {
                key.clone()
            };
            ResolvedCase {
                record,
                family,
                axes: case_axes(&record.case),
            }
        })
        .collect()
}

/// The row key: group id, the context-axis coordinates (resonator family, gesture, …), and the set
/// of selector-axis names. Cases sharing it are the same instrument and gesture and so belong in
/// one switchable row; the selector axes are what varies inside it.
fn row_key(case: &CatalogCase) -> String {
    let axes = case_axes(case);
    let context = axes
        .iter()
        .filter(|coord| !is_selector(coord.axis))
        .map(|coord| format!("{}={}", coord.axis, coord.value))
        .collect::<Vec<_>>()
        .join(",");
    let shape = axes
        .iter()
        .filter(|coord| is_selector(coord.axis))
        .map(|coord| coord.axis)
        .collect::<Vec<_>>()
        .join("-");
    format!("{}|{context}|{shape}", case.group_id)
}

/// The case's coordinate within its row: only the selector axes (context is fixed per row).
fn selector_coords(case: &CatalogCase) -> String {
    case_axes(case)
        .iter()
        .filter(|coord| is_selector(coord.axis))
        .map(|coord| format!("{}={}", coord.axis, coord.value))
        .collect::<Vec<_>>()
        .join(",")
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

fn write_case_record(
    output: &mut String,
    record: &ManifestRecord,
    family: &str,
    axes: &[AxisCoord],
) {
    output.push_str("[[cases]]\n");
    output.push_str(&format!("id = \"{}\"\n", record.case.id));
    output.push_str(&format!("title = \"{}\"\n", escape(record.case.title)));
    output.push_str(&format!("group_id = \"{}\"\n", record.case.group_id));
    output.push_str(&format!("family = \"{}\"\n", escape(family)));
    output.push_str(&format!("wav = \"{}\"\n", record.case.relative_wav));
    output.push_str(&format!("tags = [{}]\n", quoted_list(record.case.tags)));
    write_axes(output, axes);
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

fn write_axes(output: &mut String, axes: &[AxisCoord]) {
    if axes.is_empty() {
        output.push_str("axes = []\n");
        return;
    }
    output.push_str("axes = [\n");
    for coord in axes {
        output.push_str(&format!(
            "  {{ axis = \"{}\", value = \"{}\", label = \"{}\" }},\n",
            escape(coord.axis),
            escape(&coord.value),
            escape(&coord.label),
        ));
    }
    output.push_str("]\n");
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
    if manifest.schema_version != 1 && manifest.schema_version != MANIFEST_SCHEMA_VERSION {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{catalog_cases, catalog_groups};

    fn dummy_metrics() -> StereoPcm16WavMetrics {
        StereoPcm16WavMetrics {
            sample_rate: CATALOG_SAMPLE_RATE,
            frames: 1,
            duration_seconds: 0.1,
            peak: 0.1,
            rms: 0.05,
            peak_dbfs: -20.0,
            rms_dbfs: -26.0,
            data_bytes: 4,
            file_bytes: 48,
        }
    }

    fn all_records() -> Vec<ManifestRecord> {
        catalog_cases()
            .into_iter()
            .map(|case| ManifestRecord {
                case,
                metrics: dummy_metrics(),
            })
            .collect()
    }

    /// Every case must resolve to a variant whose coordinates are unique within its family — no
    /// two renders may collapse onto the same selectable point, or one would be unreachable.
    #[test]
    fn resolved_variants_are_unique_within_each_family() {
        let records = all_records();
        let resolved = resolve_cases(&records);
        assert_eq!(resolved.len(), records.len());

        let mut seen: BTreeMap<(String, String), &str> = BTreeMap::new();
        for case in &resolved {
            assert!(
                !case.axes.is_empty(),
                "case '{}' resolved to no axes",
                case.record.case.id
            );
            let coords = case
                .axes
                .iter()
                .map(|coord| format!("{}={}", coord.axis, coord.value))
                .collect::<Vec<_>>()
                .join(",");
            if let Some(previous) =
                seen.insert((case.family.clone(), coords.clone()), case.record.case.id)
            {
                panic!(
                    "family '{}' coordinates '{coords}' shared by '{previous}' and '{}'",
                    case.family, case.record.case.id
                );
            }
        }
    }

    /// The resonator family is never a switchable variant: within any row, every case shares one
    /// `family` value. This is the core "tube and string never share a row" guarantee.
    #[test]
    fn resonator_family_never_varies_within_a_row() {
        let records = all_records();
        let resolved = resolve_cases(&records);
        let mut family_values: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for case in &resolved {
            if let Some(family) = case.axes.iter().find(|coord| coord.axis == "family") {
                family_values
                    .entry(case.family.clone())
                    .or_default()
                    .insert(family.value.clone());
            }
        }
        for (row, families) in &family_values {
            assert!(
                families.len() <= 1,
                "row '{row}' mixes resonator families {families:?}"
            );
        }
    }

    /// Baseline splits into one row per resonator family (not one cross-family row), each switching
    /// velocity; and the bespoke articulation phrases that collide become singleton rows (no
    /// generic `variant` bucket axis exists any more).
    #[test]
    fn baseline_splits_per_family_and_no_variant_bucket() {
        let records = all_records();
        let resolved = resolve_cases(&records);

        let baseline_families: BTreeSet<&str> = resolved
            .iter()
            .filter(|case| case.record.case.group_id == "baseline_dynamics")
            .map(|case| case.family.as_str())
            .collect();
        assert_eq!(
            baseline_families.len(),
            4,
            "baseline should split into one row per resonator family"
        );

        let modal = resolved
            .iter()
            .find(|case| case.record.case.id == "baseline_modal_c4_v020")
            .expect("baseline modal present");
        let string = resolved
            .iter()
            .find(|case| case.record.case.id == "baseline_string_c4_v020")
            .expect("baseline string present");
        assert_ne!(modal.family, string.family, "modal and string share a row");

        assert!(
            !resolved
                .iter()
                .any(|case| case.axes.iter().any(|coord| coord.axis == "variant")),
            "the generic variant bucket axis must no longer be emitted"
        );

        // mesh_scale and its overlap twin collide on selector coords, so each is its own row.
        let scale = resolved
            .iter()
            .find(|case| case.record.case.id == "mesh_scale_c4_c5")
            .expect("mesh scale present");
        assert_eq!(
            scale.family, "mesh_scale_c4_c5",
            "collision should singletonize"
        );
    }

    /// The emitted manifest must be valid TOML at schema v2 and carry the family + axes per case.
    #[test]
    fn manifest_emits_valid_v2_with_axes() {
        #[derive(Deserialize)]
        struct ParsedAxis {
            axis: String,
            value: String,
            label: String,
        }
        #[derive(Deserialize)]
        struct ParsedCase {
            id: String,
            family: String,
            #[serde(default)]
            axes: Vec<ParsedAxis>,
        }
        #[derive(Deserialize)]
        struct Parsed {
            schema_version: u32,
            cases: Vec<ParsedCase>,
        }

        let records = all_records();
        let toml_text = manifest_toml(catalog_groups(), &records, "library");
        let parsed: Parsed = toml::from_str(&toml_text).expect("emitted manifest parses");
        assert_eq!(parsed.schema_version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(parsed.cases.len(), records.len());

        let baseline = parsed
            .cases
            .iter()
            .find(|case| case.id == "baseline_modal_c4_v020")
            .expect("baseline case present");
        assert!(
            baseline.family.contains("baseline_dynamics"),
            "family key should be scoped to the group: {}",
            baseline.family
        );
        assert!(baseline.axes.iter().all(|axis| {
            !axis.axis.is_empty() && !axis.value.is_empty() && !axis.label.is_empty()
        }));
        assert!(
            baseline
                .axes
                .iter()
                .any(|axis| axis.axis == "family" && axis.value == "modal")
        );
    }
}
