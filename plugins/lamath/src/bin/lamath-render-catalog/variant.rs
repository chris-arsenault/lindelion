//! Variant-axis derivation for the review UI's A/B/C selectors.
//!
//! Every rendered case is one point in a small combinatoric grid (family/driver ×
//! velocity × bell × …). The review UI groups those points into a single row with one
//! selector per axis instead of N flat files. Rather than hand-annotate every case, the
//! axes are derived from the *typed* [`PatchRecipe`] and the render schedule: the match is
//! exhaustive, so adding a recipe field the compiler forces a new axis to be considered,
//! and two cases that differ in any recipe field get distinct coordinates. The derived
//! axes are written verbatim into `manifest.toml` (schema v2) so the server never guesses.

use crate::catalog::{
    CatalogCase, ContactRecipe, DriverRecipe, EdgeRecipe, MeshStriker, MeshVoicing, PatchRecipe,
    ResonatorFamily, ScheduledNote, SourceBodyDepth, SurroundingRecipe, TubeBellLevel,
    TubeBodyFormantLevel, TubeRadiationShape, TubeReedAperture, TubeReferenceArticulation,
    TubeReferenceHumanize, TubeReferenceMatchGain, TubeReferenceRegisterKey,
};

/// One coordinate of a case along a named axis. `value` is a stable lowercase token used for
/// grouping/selection; `label` is the human string shown on the selector control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AxisCoord {
    pub(crate) axis: &'static str,
    pub(crate) value: String,
    pub(crate) label: String,
}

impl AxisCoord {
    fn new(axis: &'static str, value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            axis,
            value: value.into(),
            label: label.into(),
        }
    }
}

/// The full coordinate tuple for a case: the resonator family first (always — it is the primary
/// row divider, never a switchable variant, so tube and string never share a row), then the
/// recipe-derived axes, then register (single-pitch schedules only), then a representative
/// velocity. Order is stable per recipe so cases in the same row present their selectors
/// consistently.
pub(crate) fn case_axes(case: &CatalogCase) -> Vec<AxisCoord> {
    if let Some(axes) = tube_reference_low_e_axes(case) {
        return axes;
    }

    let mut axes = vec![family_axis(resonator_family(&case.patch_recipe))];
    axes.extend(recipe_axes(&case.patch_recipe));
    if let Some(strikes) = strikes_axis(case.schedule.notes) {
        axes.push(strikes);
    }
    if let Some(register) = register_axis(case.schedule.notes) {
        axes.push(register);
    }
    if let Some(velocity) = velocity_axis(case.schedule.notes) {
        axes.push(velocity);
    }
    axes
}

/// Whether an axis is a *selector* (a knob worth A/B-ing within one row) rather than *context*
/// (an identity dimension that divides cases into separate rows). The resonator `family` is always
/// context — flipping it is never a like-for-like comparison. `strikes` (gesture), `gain`/`source`
/// (reference identity), `articulation`, and `edge` are likewise identity, not knobs.
pub(crate) fn is_selector(axis: &str) -> bool {
    matches!(
        axis,
        "velocity"
            | "register"
            | "bell"
            | "body"
            | "body_level"
            | "radiation_shape"
            | "bore_steepening"
            | "reed_aperture"
            | "humanize"
            | "output_path"
            | "driver"
            | "contact"
            | "surrounding"
            | "body_depth"
            | "striker"
            | "voicing"
            | "polyphony"
            | "retrigger"
            | "register_key"
            | "reference_variant"
    )
}

/// The resonator family every case belongs to. Always defined — recipes that do not name a family
/// directly (tube/mesh phrases, reference WAVs, edges) still map to one — so the family coordinate
/// can divide rows by instrument for every case.
fn resonator_family(recipe: &PatchRecipe) -> ResonatorFamily {
    match recipe {
        PatchRecipe::SingleFamily(family)
        | PatchRecipe::Driver { family, .. }
        | PatchRecipe::Contact { family, .. }
        | PatchRecipe::Surrounding { family, .. } => *family,
        PatchRecipe::SourceBodyBalance { .. } => ResonatorFamily::String,
        PatchRecipe::StringBowAlternatingScale => ResonatorFamily::String,
        PatchRecipe::Edge(edge) => edge_family(*edge),
        PatchRecipe::ReferenceWav { .. }
        | PatchRecipe::TubePhrase { .. }
        | PatchRecipe::TubePathAuditPhrase { .. }
        | PatchRecipe::TubeBodyFormantPhrase { .. }
        | PatchRecipe::TubeBodyFormantMixPhrase { .. }
        | PatchRecipe::TubeRadiationShapePhrase { .. }
        | PatchRecipe::TubeBoreSteepeningPhrase { .. }
        | PatchRecipe::TubeReferenceMatchPhrase { .. } => ResonatorFamily::Tube,
        PatchRecipe::MeshPhrase { .. }
        | PatchRecipe::MeshVoicing(_)
        | PatchRecipe::MeshStriker { .. } => ResonatorFamily::Mesh,
    }
}

fn edge_family(edge: EdgeRecipe) -> ResonatorFamily {
    match edge {
        EdgeRecipe::StringHighLoopGain
        | EdgeRecipe::StringHighDispersion
        | EdgeRecipe::StringSourceBodyLow
        | EdgeRecipe::StringDenseHardChord => ResonatorFamily::String,
        EdgeRecipe::TubeClosedNonlinear | EdgeRecipe::TubeOpenNonlinear => ResonatorFamily::Tube,
        EdgeRecipe::MeshLowDampingHighMaterial => ResonatorFamily::Mesh,
        EdgeRecipe::ModalBrightLongDecay => ResonatorFamily::Modal,
    }
}

/// The recipe-specific axes (the family coordinate is prepended separately by [`case_axes`]).
fn recipe_axes(recipe: &PatchRecipe) -> Vec<AxisCoord> {
    match recipe {
        PatchRecipe::SingleFamily(_) => vec![],
        PatchRecipe::Driver { driver, .. } => vec![driver_axis(*driver)],
        PatchRecipe::Contact { contact, .. } => vec![contact_axis(*contact)],
        PatchRecipe::SourceBodyBalance { depth } => vec![source_body_depth_axis(*depth)],
        PatchRecipe::StringBowAlternatingScale => vec![AxisCoord::new(
            "driver",
            "bow_alternating",
            "Bow (smooth/scratch)",
        )],
        PatchRecipe::Surrounding { surrounding, .. } => vec![surrounding_axis(*surrounding)],
        PatchRecipe::ReferenceWav { path } => vec![reference_source_axis(path)],
        PatchRecipe::Edge(edge) => vec![edge_axis(*edge)],
        PatchRecipe::TubePhrase {
            polyphony,
            retrigger,
            bell,
            reed_aperture,
        } => vec![
            bool_axis("bell", *bell, "On", "Off"),
            reed_aperture_axis(*reed_aperture),
            polyphony_axis(*polyphony),
            retrigger_axis(*retrigger),
        ],
        PatchRecipe::TubePathAuditPhrase {
            bell_enabled,
            body_enabled,
        } => vec![
            bool_axis("bell", *bell_enabled, "On", "Off"),
            bool_axis("body", *body_enabled, "On", "Off"),
        ],
        PatchRecipe::TubeBodyFormantPhrase {
            bell_enabled,
            level,
        } => vec![
            bool_axis("bell", *bell_enabled, "On", "Off"),
            body_level_axis(*level),
        ],
        PatchRecipe::TubeBodyFormantMixPhrase { bell, level } => {
            vec![bell_level_axis(*bell), body_level_axis(*level)]
        }
        PatchRecipe::TubeRadiationShapePhrase { bell, shape } => {
            vec![bell_level_axis(*bell), radiation_shape_axis(*shape)]
        }
        PatchRecipe::TubeBoreSteepeningPhrase {
            bell,
            steepening_enabled,
        } => vec![
            bell_level_axis(*bell),
            bool_axis("bore_steepening", *steepening_enabled, "On", "Off"),
        ],
        PatchRecipe::TubeReferenceMatchPhrase {
            articulation,
            gain,
            humanize,
            register_key,
            body_enabled,
            reed_radiation_enabled: _,
        } => vec![
            reference_articulation_axis(*articulation),
            reference_gain_axis(*gain),
            reference_humanize_axis(*humanize),
            reference_register_key_axis(*register_key),
            bool_axis("body", *body_enabled, "On", "Off"),
        ],
        PatchRecipe::MeshPhrase {
            polyphony,
            retrigger,
        } => vec![polyphony_axis(*polyphony), retrigger_axis(*retrigger)],
        PatchRecipe::MeshVoicing(voicing) => vec![mesh_voicing_axis(*voicing)],
        PatchRecipe::MeshStriker { voicing, striker } => {
            vec![mesh_voicing_axis(*voicing), mesh_striker_axis(*striker)]
        }
    }
}

fn family_axis(family: ResonatorFamily) -> AxisCoord {
    let (value, label) = match family {
        ResonatorFamily::Modal => ("modal", "Modal"),
        ResonatorFamily::String => ("string", "String"),
        ResonatorFamily::Tube => ("tube", "Tube"),
        ResonatorFamily::Mesh => ("mesh", "Mesh"),
    };
    AxisCoord::new("family", value, label)
}

fn driver_axis(driver: DriverRecipe) -> AxisCoord {
    let (value, label) = match driver {
        DriverRecipe::Sample => ("sample", "Sample"),
        DriverRecipe::PickSoft => ("pick_soft", "Pick (soft)"),
        DriverRecipe::PickHard => ("pick_hard", "Pick (hard)"),
        DriverRecipe::BowSmooth => ("bow_smooth", "Bow (smooth)"),
        DriverRecipe::BowScratch => ("bow_scratch", "Bow (scratch)"),
        DriverRecipe::ReedSoft => ("reed_soft", "Reed (soft)"),
        DriverRecipe::ReedHard => ("reed_hard", "Reed (hard)"),
    };
    AxisCoord::new("driver", value, label)
}

fn contact_axis(contact: ContactRecipe) -> AxisCoord {
    let (value, label) = match contact {
        ContactRecipe::TightShort => ("tight_short", "Tight · short"),
        ContactRecipe::TightLong => ("tight_long", "Tight · long"),
        ContactRecipe::WideShort => ("wide_short", "Wide · short"),
        ContactRecipe::WideLong => ("wide_long", "Wide · long"),
    };
    AxisCoord::new("contact", value, label)
}

fn source_body_depth_axis(depth: SourceBodyDepth) -> AxisCoord {
    let (value, label) = match depth {
        SourceBodyDepth::Depth000 => ("0", "0%"),
        SourceBodyDepth::Depth050 => ("50", "50%"),
        SourceBodyDepth::Depth100 => ("100", "100%"),
    };
    AxisCoord::new("body_depth", value, label)
}

fn surrounding_axis(surrounding: SurroundingRecipe) -> AxisCoord {
    let (value, label) = match surrounding {
        SurroundingRecipe::Off => ("off", "Off"),
        SurroundingRecipe::Mechanical => ("mechanical", "Mechanical"),
        SurroundingRecipe::Radiation => ("radiation", "Radiation"),
        SurroundingRecipe::Sympathetic => ("sympathetic", "Sympathetic"),
        SurroundingRecipe::MechanicalRadiation => ("mechanical_radiation", "Mech + Rad"),
        SurroundingRecipe::MechanicalSympathetic => ("mechanical_sympathetic", "Mech + Symp"),
        SurroundingRecipe::RadiationSympathetic => ("radiation_sympathetic", "Rad + Symp"),
        SurroundingRecipe::All => ("all", "All"),
    };
    AxisCoord::new("surrounding", value, label)
}

fn edge_axis(edge: EdgeRecipe) -> AxisCoord {
    let (value, label) = match edge {
        EdgeRecipe::StringHighLoopGain => ("string_high_loop_gain", "String · high loop gain"),
        EdgeRecipe::StringHighDispersion => ("string_high_dispersion", "String · high dispersion"),
        EdgeRecipe::StringSourceBodyLow => ("string_source_body_low", "String · low source-body"),
        EdgeRecipe::TubeClosedNonlinear => ("tube_closed_nonlinear", "Tube · closed nonlinear"),
        EdgeRecipe::TubeOpenNonlinear => ("tube_open_nonlinear", "Tube · open nonlinear"),
        EdgeRecipe::MeshLowDampingHighMaterial => (
            "mesh_low_damping_high_material",
            "Mesh · low damp / high material",
        ),
        EdgeRecipe::ModalBrightLongDecay => {
            ("modal_bright_long_decay", "Modal · bright long decay")
        }
        EdgeRecipe::StringDenseHardChord => {
            ("string_dense_hard_chord", "String · dense hard chord")
        }
    };
    AxisCoord::new("edge", value, label)
}

fn reed_aperture_axis(aperture: TubeReedAperture) -> AxisCoord {
    let (value, label) = match aperture {
        TubeReedAperture::Instant => ("instant", "Instant"),
        TubeReedAperture::Inertial => ("inertial", "Inertial"),
    };
    AxisCoord::new("reed_aperture", value, label)
}

fn body_level_axis(level: TubeBodyFormantLevel) -> AxisCoord {
    let (value, label) = match level {
        TubeBodyFormantLevel::Current => ("current", "Current"),
        TubeBodyFormantLevel::Medium => ("medium", "Medium"),
        TubeBodyFormantLevel::Strong => ("strong", "Strong"),
    };
    AxisCoord::new("body_level", value, label)
}

fn bell_level_axis(bell: TubeBellLevel) -> AxisCoord {
    let (value, label) = match bell {
        TubeBellLevel::Off => ("off", "Off"),
        TubeBellLevel::Nominal => ("nominal", "Nominal"),
        TubeBellLevel::Full => ("full", "Full"),
    };
    AxisCoord::new("bell", value, label)
}

fn radiation_shape_axis(shape: TubeRadiationShape) -> AxisCoord {
    let (value, label) = match shape {
        TubeRadiationShape::Current => ("current", "Current"),
        TubeRadiationShape::Gentle => ("gentle", "Gentle"),
    };
    AxisCoord::new("radiation_shape", value, label)
}

fn reference_articulation_axis(articulation: TubeReferenceArticulation) -> AxisCoord {
    let (value, label) = match articulation {
        TubeReferenceArticulation::Legato => ("legato", "Legato"),
        TubeReferenceArticulation::Tongue => ("tongue", "Tongue"),
    };
    AxisCoord::new("articulation", value, label)
}

fn reference_gain_axis(gain: TubeReferenceMatchGain) -> AxisCoord {
    let (value, label) = match gain {
        TubeReferenceMatchGain::LowESustainPhysical => {
            ("low_e_sustain_physical", "Low-E sustain (physical)")
        }
        TubeReferenceMatchGain::LowESustainBodyOff => {
            ("low_e_sustain_body_off", "Low-E sustain body off")
        }
        TubeReferenceMatchGain::RegisterKeyHighSustain => {
            ("register_key_high_sustain", "Register-key high sustain")
        }
        TubeReferenceMatchGain::RegisterKeyHighSustainVented => (
            "register_key_high_sustain_vented",
            "Register-key high sustain (vented)",
        ),
        TubeReferenceMatchGain::LowHighArticulation => {
            ("low_high_articulation", "Low/high articulation")
        }
    };
    AxisCoord::new("gain", value, label)
}

fn reference_humanize_axis(humanize: TubeReferenceHumanize) -> AxisCoord {
    let (value, label) = match humanize {
        TubeReferenceHumanize::Off => ("off", "Off"),
        TubeReferenceHumanize::Medium => ("medium", "Medium"),
        TubeReferenceHumanize::Full => ("full", "Full"),
    };
    AxisCoord::new("humanize", value, label)
}

fn reference_register_key_axis(register_key: TubeReferenceRegisterKey) -> AxisCoord {
    let (value, label) = match register_key {
        TubeReferenceRegisterKey::Default => ("default", "Default"),
        TubeReferenceRegisterKey::Disabled => ("disabled", "Disabled"),
    };
    AxisCoord::new("register_key", value, label)
}

fn tube_reference_low_e_axes(case: &CatalogCase) -> Option<Vec<AxisCoord>> {
    let variant = match case.id {
        "tube_ref_low_e_sustain_reference" => ("reference", "Reference"),
        "tube_ref_low_e_sustain_current" => ("full", "Full sim"),
        "tube_ref_low_e_sustain_body_off" => ("no_body", "No body"),
        "tube_ref_low_e_sustain_no_reed" => ("no_reed", "No reed"),
        _ => return None,
    };

    Some(vec![
        family_axis(ResonatorFamily::Tube),
        AxisCoord::new("reference_gesture", "low_e_sustain", "Low E sustain"),
        AxisCoord::new("reference_variant", variant.0, variant.1),
    ])
}

fn mesh_voicing_axis(voicing: MeshVoicing) -> AxisCoord {
    let (value, label) = match voicing {
        MeshVoicing::Triangle => ("triangle", "Triangle"),
        MeshVoicing::Ride => ("ride", "Ride"),
        MeshVoicing::KitRide => ("kit_ride", "Kit ride"),
        MeshVoicing::Crash => ("crash", "Crash"),
        MeshVoicing::KitCrash => ("kit_crash", "Kit crash"),
        MeshVoicing::DensitySparse => ("density_sparse", "Density · sparse"),
        MeshVoicing::DensityDense => ("density_dense", "Density · dense"),
        MeshVoicing::DecayShort => ("decay_short", "Decay · short"),
        MeshVoicing::DecayLong => ("decay_long", "Decay · long"),
    };
    AxisCoord::new("voicing", value, label)
}

fn mesh_striker_axis(striker: MeshStriker) -> AxisCoord {
    let (value, label) = match striker {
        MeshStriker::HardStick => ("hard_stick", "Hard stick"),
        MeshStriker::SoftMallet => ("soft_mallet", "Soft mallet"),
        MeshStriker::JazzBrush => ("jazz_brush", "Jazz brush"),
        MeshStriker::BellStick => ("bell_stick", "Bell stick"),
    };
    AxisCoord::new("striker", value, label)
}

fn polyphony_axis(polyphony: u8) -> AxisCoord {
    let label = if polyphony <= 1 {
        "Mono".to_string()
    } else {
        format!("Poly ×{polyphony}")
    };
    AxisCoord::new("polyphony", polyphony.to_string(), label)
}

fn retrigger_axis(retrigger: bool) -> AxisCoord {
    bool_axis("retrigger", retrigger, "Re-strike", "Ring through")
}

fn bool_axis(axis: &'static str, value: bool, on_label: &str, off_label: &str) -> AxisCoord {
    if value {
        AxisCoord::new(axis, "on", on_label.to_string())
    } else {
        AxisCoord::new(axis, "off", off_label.to_string())
    }
}

fn reference_source_axis(path: &str) -> AxisCoord {
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .strip_suffix(".wav")
        .unwrap_or(path);
    let value: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    AxisCoord::new("source", value, stem.replace(['_', '-'], " "))
}

/// A `strikes` axis: how many note onsets the schedule fires. This separates an otherwise
/// identical single strike from a repeated-strike phrase (same family, switchable A/B) rather
/// than letting them collide on the same coordinate. Emitted for every case so single and
/// repeated variants share one family; it collapses to a constant where every case agrees.
fn strikes_axis(notes: &[ScheduledNote]) -> Option<AxisCoord> {
    if notes.is_empty() {
        return None;
    }
    let count = notes.len();
    let label = if count == 1 {
        "Single".to_string()
    } else {
        format!("{count} hits")
    };
    Some(AxisCoord::new("strikes", count.to_string(), label))
}

/// A `register` axis only when every scheduled note is the same pitch (single-note auditions);
/// phrases that span pitches have no single register and are left out.
fn register_axis(notes: &[ScheduledNote]) -> Option<AxisCoord> {
    let first = notes.first()?;
    if notes.iter().any(|note| note.note != first.note) {
        return None;
    }
    let name = note_name(first.note);
    Some(AxisCoord::new("register", name.to_ascii_lowercase(), name))
}

/// A representative velocity for the case: the loudest scheduled note, mapped back to 0–127.
fn velocity_axis(notes: &[ScheduledNote]) -> Option<AxisCoord> {
    let velocity = notes
        .iter()
        .map(|note| (note.velocity * 127.0).round().clamp(0.0, 127.0) as u8)
        .max()?;
    Some(AxisCoord::new(
        "velocity",
        velocity.to_string(),
        velocity.to_string(),
    ))
}

fn note_name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = i16::from(midi) / 12 - 1;
    format!("{}{}", NAMES[usize::from(midi % 12)], octave)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog_cases;
    use std::collections::BTreeSet;

    fn coord(axis: &str, axes: &[AxisCoord]) -> Option<String> {
        axes.iter()
            .find(|coord| coord.axis == axis)
            .map(|coord| coord.value.clone())
    }

    #[test]
    fn baseline_case_exposes_family_register_velocity() {
        let cases = catalog_cases();
        let case = cases
            .iter()
            .find(|case| case.id == "baseline_modal_c4_v020")
            .expect("baseline case present");
        let axes = case_axes(case);
        assert_eq!(coord("family", &axes).as_deref(), Some("modal"));
        assert_eq!(coord("register", &axes).as_deref(), Some("c4"));
        assert_eq!(coord("velocity", &axes).as_deref(), Some("20"));
    }

    #[test]
    fn note_names_follow_scientific_octave() {
        assert_eq!(note_name(36), "C2");
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(84), "C6");
    }

    /// Every axis value must be a stable lowercase token (used as a selection key), and every
    /// case must carry at least one axis so it can be placed in a family.
    #[test]
    fn axis_values_are_stable_tokens() {
        let cases = catalog_cases();
        let mut axis_ids = BTreeSet::new();
        for case in &cases {
            let axes = case_axes(case);
            assert!(!axes.is_empty(), "case '{}' has no axes", case.id);
            for coord in axes {
                axis_ids.insert(coord.axis);
                assert!(
                    !coord.value.is_empty()
                        && coord
                            .value
                            .chars()
                            .all(|character| character.is_ascii_lowercase()
                                || character.is_ascii_digit()
                                || character == '_'
                                || character == '#'),
                    "case '{}' axis '{}' has non-token value '{}'",
                    case.id,
                    coord.axis,
                    coord.value
                );
            }
        }
        assert!(axis_ids.contains("family"));
        assert!(axis_ids.contains("velocity"));
    }
}
