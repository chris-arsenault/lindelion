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
    Edge(EdgeRecipe),
    /// Driven wind Tube playing a multi-note phrase, with the articulation knobs the schedule
    /// can't express: `polyphony` (1 = mono voice-stealing slur; >1 = poly) and
    /// `retrigger` (re-strike the bore per note vs let it ring through note changes).
    TubePhrase {
        polyphony: u8,
        retrigger: bool,
    },
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
    UnsafeOutputPath(String),
}

const GROUPS: [CatalogGroup; 9] = [
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
            Self::UnsafeOutputPath(path) => write!(formatter, "unsafe output path: {path}"),
        }
    }
}

#[cfg(test)]
mod tests;
