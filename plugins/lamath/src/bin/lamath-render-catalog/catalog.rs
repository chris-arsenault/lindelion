use crate::cli::RenderSelection;
use std::{collections::BTreeSet, fmt, path::Path};

mod cases;

pub(crate) use cases::catalog_cases;

pub(crate) const CATALOG_SAMPLE_RATE: u32 = 48_000;
pub(crate) const CATALOG_BLOCK_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CatalogGroup {
    pub(crate) id: &'static str,
    pub(crate) directory: &'static str,
    pub(crate) title: &'static str,
    pub(crate) question: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CatalogCase {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) group_id: &'static str,
    pub(crate) relative_wav: &'static str,
    pub(crate) tags: &'static [&'static str],
    pub(crate) patch_recipe: PatchRecipe,
    pub(crate) schedule: RenderSchedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatchRecipe {
    SingleFamily(ResonatorFamily),
    Driver {
        family: ResonatorFamily,
        driver: DriverRecipe,
    },
    Contact {
        family: ResonatorFamily,
        contact: ContactRecipe,
    },
    SourceBodyBalance {
        depth: SourceBodyDepth,
    },
    Surrounding {
        family: ResonatorFamily,
        surrounding: SurroundingRecipe,
    },
    ReferenceWav {
        path: &'static str,
    },
    Edge(EdgeRecipe),
    /// Driven wind Tube playing a multi-note phrase, with the articulation knobs the schedule
    /// can't express: `polyphony` (1 = mono voice-stealing slur; >1 = poly) and
    /// `retrigger` (re-strike the bore per note vs let it ring through note changes).
    TubePhrase {
        polyphony: u8,
        retrigger: bool,
        /// Bell HF-radiation tap: `true` = current model, `false` = bell off (the audition
        /// A/B for the radiation tap; ADR-0032 item-B follow-up).
        bell: bool,
        reed_aperture: TubeReedAperture,
    },
    TubePathAuditPhrase {
        bell_enabled: bool,
        body_enabled: bool,
    },
    TubeBodyFormantPhrase {
        bell_enabled: bool,
        level: TubeBodyFormantLevel,
    },
    TubeBodyFormantMixPhrase {
        bell: TubeBellLevel,
        level: TubeBodyFormantLevel,
    },
    TubeRadiationShapePhrase {
        bell: TubeBellLevel,
        shape: TubeRadiationShape,
    },
    TubeBoreSteepeningPhrase {
        bell: TubeBellLevel,
        steepening_enabled: bool,
    },
    TubeReferenceMatchPhrase {
        articulation: TubeReferenceArticulation,
        gain: TubeReferenceMatchGain,
    },
    /// Struck Mesh playing a multi-note phrase, exposing the same articulation knobs the schedule
    /// can't express: `polyphony` (1 = single voice-stealing body; >1 = independent struck voices
    /// per note) and `retrigger` (re-strike the body per same-note hit vs preserve its ring).
    MeshPhrase {
        polyphony: u8,
        retrigger: bool,
    },
    /// A single struck-Mesh voicing exercising the timbre controls (grid density via `size`/
    /// `tension`, decay via `damping`, edge/strike character via `material`/strike position).
    MeshVoicing(MeshVoicing),
    MeshStriker {
        voicing: MeshVoicing,
        striker: MeshStriker,
    },
}

/// Named Mesh timbre points: character presets (triangle → ride → crash) plus single-axis
/// sweeps so each control can be heard in isolation. The Mesh is a fixed-pitch struck
/// idiophone, so these are auditioned by ear — grid cell count is the density/timbre lever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshVoicing {
    Triangle,
    Ride,
    KitRide,
    Crash,
    KitCrash,
    DensitySparse,
    DensityDense,
    DecayShort,
    DecayLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshStriker {
    HardStick,
    SoftMallet,
    JazzBrush,
    BellStick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DriverRecipe {
    Sample,
    PickSoft,
    PickHard,
    BowSmooth,
    BowScratch,
    ReedSoft,
    ReedHard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeReedAperture {
    Instant,
    Inertial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeBodyFormantLevel {
    Current,
    Medium,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeBellLevel {
    Off,
    Nominal,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeRadiationShape {
    Current,
    Gentle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeReferenceArticulation {
    Legato,
    Tongue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeReferenceMatchGain {
    LowESustain,
    RegisterKeyHighSustain,
    LowHighArticulation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContactRecipe {
    TightShort,
    TightLong,
    WideShort,
    WideLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceBodyDepth {
    Depth000,
    Depth050,
    Depth100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurroundingRecipe {
    Off,
    Mechanical,
    Radiation,
    Sympathetic,
    MechanicalRadiation,
    MechanicalSympathetic,
    RadiationSympathetic,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeRecipe {
    StringHighLoopGain,
    StringHighDispersion,
    StringSourceBodyLow,
    TubeClosedNonlinear,
    TubeOpenNonlinear,
    MeshLowDampingHighMaterial,
    ModalBrightLongDecay,
    StringDenseHardChord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResonatorFamily {
    Modal,
    String,
    Tube,
    Mesh,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RenderSchedule {
    pub(crate) duration_seconds: f32,
    pub(crate) notes: &'static [ScheduledNote],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScheduledNote {
    pub(crate) start_seconds: f32,
    pub(crate) end_seconds: f32,
    pub(crate) note: u8,
    pub(crate) velocity: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CatalogError {
    DuplicateCaseId(String),
    DuplicateOutputPath(String),
    UnknownCase(String),
    UnknownGroup(String),
    UnknownTag(String),
    UnsafeOutputPath(String),
}

const GROUPS: [CatalogGroup; 19] = [
    CatalogGroup {
        id: "baseline_dynamics",
        directory: "01_baseline_dynamics",
        title: "Baseline Dynamics",
        question: "How do Modal/String/Tube/Mesh compare at C4 across soft/medium/hard velocity?",
    },
    CatalogGroup {
        id: "register_range",
        directory: "02_register_range",
        title: "Register Range",
        question: "Does each family stay voiced and tuned across C2/C4/C6?",
    },
    CatalogGroup {
        id: "drivers",
        directory: "03_drivers",
        title: "Drivers",
        question: "How do Sample, Pick, Reed, and Bow differ as excitation drivers?",
    },
    CatalogGroup {
        id: "contact",
        directory: "04_contact",
        title: "Contact",
        question: "How audible are picked/strummed spread and contact-time darkening?",
    },
    CatalogGroup {
        id: "source_body_balance",
        directory: "05_source_body_balance",
        title: "Source-Body Balance",
        question: "Does String move from soft-warm to loud-bright without acting like a fader?",
    },
    CatalogGroup {
        id: "surrounding",
        directory: "06_surrounding",
        title: "Surrounding Effects",
        question: "What do mechanical noise, radiation brightness, and sympathetic depth add?",
    },
    CatalogGroup {
        id: "chords",
        directory: "07_chords",
        title: "Chords",
        question: "How do polyphony, tails, sympathetic resonance, and master safety behave?",
    },
    CatalogGroup {
        id: "edges",
        directory: "08_edges",
        title: "Edges",
        question: "Do bounded extreme settings expose harshness, weak output, or instability?",
    },
    CatalogGroup {
        id: "articulation",
        directory: "09_articulation",
        title: "Articulation",
        question: "Does the driven wind voice phrase a scale — tongued, legato, slurred — with audibly distinct articulation?",
    },
    CatalogGroup {
        id: "mesh_timbre",
        directory: "10_mesh_timbre",
        title: "Mesh Timbre",
        question: "Does the Mesh span distinct decay/harmonic/tone characters (triangle → ride → crash) as size, density, and damping change?",
    },
    CatalogGroup {
        id: "tube_dynamics",
        directory: "11_tube_dynamics",
        title: "Tube Dynamics & Bell",
        question: "Does the wind Tube brighten with velocity (cuivré, not just louder) across a C4-C5 scale, and what does the bell HF-radiation tap contribute (on vs off)?",
    },
    CatalogGroup {
        id: "tube_reed_aperture",
        directory: "12_tube_reed_aperture",
        title: "Tube Reed Aperture A/B",
        question: "Does a finite-inertia reed aperture reduce digital HF while preserving articulation and pitch?",
    },
    CatalogGroup {
        id: "mesh_strikers",
        directory: "13_mesh_strikers",
        title: "Mesh Sticks & Mallets",
        question: "Do the four Mesh striker impulses stay distinct across single hits, repeated notes, overlap phrases, and ride/crash body settings?",
    },
    CatalogGroup {
        id: "tube_output_paths",
        directory: "14_tube_output_paths",
        title: "Tube Output Path Audit",
        question: "Which existing Tube output path carries the audible square/HF character: body pickup, bell radiation, or dry pickup?",
    },
    CatalogGroup {
        id: "tube_body_formant_levels",
        directory: "15_tube_body_formant_levels",
        title: "Tube Body Formant Levels",
        question: "Does a louder tracked-h3 body path become audible by itself and in the full Tube mix?",
    },
    CatalogGroup {
        id: "tube_body_formant_mix",
        directory: "16_tube_body_formant_mix",
        title: "Tube Body Formant Mix",
        question: "Does a stronger tracked-h3 body path survive the nominal 50% bell mix while full bell remains a useful upper comparison?",
    },
    CatalogGroup {
        id: "tube_radiation_shape",
        directory: "17_tube_radiation_shape",
        title: "Tube Radiation Shape A/B",
        question: "Does gentler first-order bell radiation keep useful brightness while reducing the square-wave edge?",
    },
    CatalogGroup {
        id: "tube_bore_steepening",
        directory: "18_tube_bore_steepening",
        title: "Tube Bore Steepening A/B",
        question: "Is the amplitude-dependent bore steepening in the feedback path the dominant source of square-wave edge?",
    },
    CatalogGroup {
        id: "tube_reference_match",
        directory: "19_tube_reference_match",
        title: "Tube Reference Match",
        question: "How does current Tube compare directly against owner clarinet reference gestures for low sustain, register-key sustain, and articulation?",
    },
];

pub(crate) fn catalog_groups() -> &'static [CatalogGroup] {
    &GROUPS
}

pub(crate) fn catalog_listing() -> String {
    let cases = catalog_cases();
    let mut output = String::new();
    for group in catalog_groups() {
        let group_cases: Vec<_> = cases
            .iter()
            .filter(|case| case.group_id == group.id)
            .collect();
        if group_cases.is_empty() {
            continue;
        }
        output.push_str(&format!("{} - {}\n", group.id, group.title));
        for case in group_cases {
            output.push_str(&format!("  {} - {}\n", case.id, case.title));
        }
    }
    output
}

pub(crate) fn selected_cases(
    cases: &[CatalogCase],
    selection: &RenderSelection,
) -> Result<Vec<CatalogCase>, CatalogError> {
    validate_catalog(cases)?;
    match selection {
        RenderSelection::All => Ok(cases.to_vec()),
        RenderSelection::Group(group_id) => selected_group(cases, group_id),
        RenderSelection::Case(case_id) => selected_case(cases, case_id),
        RenderSelection::Tag(tag) => selected_tag(cases, tag),
    }
}

pub(crate) fn validate_catalog(cases: &[CatalogCase]) -> Result<(), CatalogError> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for case in cases {
        if !safe_relative_wav(case.relative_wav) {
            return Err(CatalogError::UnsafeOutputPath(
                case.relative_wav.to_string(),
            ));
        }
        if !ids.insert(case.id) {
            return Err(CatalogError::DuplicateCaseId(case.id.to_string()));
        }
        if !paths.insert(case.relative_wav) {
            return Err(CatalogError::DuplicateOutputPath(
                case.relative_wav.to_string(),
            ));
        }
        if !catalog_groups()
            .iter()
            .any(|group| group.id == case.group_id)
        {
            return Err(CatalogError::UnknownGroup(case.group_id.to_string()));
        }
    }
    Ok(())
}

fn selected_group(cases: &[CatalogCase], group_id: &str) -> Result<Vec<CatalogCase>, CatalogError> {
    if !catalog_groups().iter().any(|group| group.id == group_id) {
        return Err(CatalogError::UnknownGroup(group_id.to_string()));
    }
    let selected: Vec<_> = cases
        .iter()
        .filter(|case| case.group_id == group_id)
        .cloned()
        .collect();
    if selected.is_empty() {
        return Err(CatalogError::UnknownGroup(group_id.to_string()));
    }
    Ok(selected)
}

fn selected_case(cases: &[CatalogCase], case_id: &str) -> Result<Vec<CatalogCase>, CatalogError> {
    cases
        .iter()
        .find(|case| case.id == case_id)
        .cloned()
        .map(|case| vec![case])
        .ok_or_else(|| CatalogError::UnknownCase(case_id.to_string()))
}

/// All cases carrying `tag` (e.g. `--tag mesh` renders every Mesh case in one pass).
fn selected_tag(cases: &[CatalogCase], tag: &str) -> Result<Vec<CatalogCase>, CatalogError> {
    let selected: Vec<_> = cases
        .iter()
        .filter(|case| case.tags.contains(&tag))
        .cloned()
        .collect();
    if selected.is_empty() {
        return Err(CatalogError::UnknownTag(tag.to_string()));
    }
    Ok(selected)
}

fn safe_relative_wav(path: &str) -> bool {
    let path = Path::new(path);
    path.extension().is_some_and(|extension| extension == "wav")
        && path.components().all(|component| match component {
            std::path::Component::Normal(segment) => {
                segment.to_str().is_some_and(safe_file_or_directory)
            }
            _ => false,
        })
}

fn safe_file_or_directory(segment: &str) -> bool {
    segment
        .strip_suffix(".wav")
        .map_or_else(|| safe_segment(segment), safe_segment)
}

fn safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

impl CatalogCase {
    pub(crate) fn target_frames(&self) -> usize {
        (f64::from(self.schedule.duration_seconds) * f64::from(CATALOG_SAMPLE_RATE)).round()
            as usize
    }
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCaseId(id) => write!(formatter, "duplicate case ID: {id}"),
            Self::DuplicateOutputPath(path) => write!(formatter, "duplicate output path: {path}"),
            Self::UnknownCase(id) => write!(formatter, "unknown case: {id}"),
            Self::UnknownGroup(id) => write!(formatter, "unknown group: {id}"),
            Self::UnknownTag(tag) => write!(formatter, "no cases with tag: {tag}"),
            Self::UnsafeOutputPath(path) => write!(formatter, "unsafe output path: {path}"),
        }
    }
}

#[cfg(test)]
mod tests;
