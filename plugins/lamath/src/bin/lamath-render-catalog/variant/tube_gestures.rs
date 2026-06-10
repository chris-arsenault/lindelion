//! Axis derivation for the Tube reference-gesture and pitch-probe groups.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) fn tube_reference_gesture_axes(case: &CatalogCase) -> Option<Vec<AxisCoord>> {
    let (gesture_value, gesture_label, variant) = match case.id {
        "tube_ref_low_e_sustain_reference" => {
            ("low_e_sustain", "Low E sustain", ("reference", "Reference"))
        }
        "tube_ref_low_e_sustain_current" => {
            ("low_e_sustain", "Low E sustain", ("full", "Full sim"))
        }
        "tube_ref_low_e_sustain_body_off" => {
            ("low_e_sustain", "Low E sustain", ("no_body", "No body"))
        }
        "tube_ref_low_e_sustain_no_reed" => {
            ("low_e_sustain", "Low E sustain", ("no_reed", "No reed"))
        }
        "tube_ref_register_key_high_reference" => (
            "register_key_high_sustain",
            "Register-key high sustain",
            ("reference", "Reference"),
        ),
        "tube_ref_register_key_high_current" => (
            "register_key_high_sustain",
            "Register-key high sustain",
            ("full", "Full sim"),
        ),
        "tube_ref_register_key_high_humanize_050" => (
            "register_key_high_sustain",
            "Register-key high sustain",
            ("humanize_050", "Humanize 50%"),
        ),
        "tube_ref_register_key_high_humanize_100" => (
            "register_key_high_sustain",
            "Register-key high sustain",
            ("humanize_100", "Humanize 100%"),
        ),
        "tube_ref_low_high_articulation_reference" => (
            "low_high_articulation",
            "Low/high articulation",
            ("reference", "Reference"),
        ),
        "tube_ref_low_high_articulation_current" => (
            "low_high_articulation",
            "Low/high articulation",
            ("full", "Full sim"),
        ),
        "tube_ref_low_high_articulation_humanize_050" => (
            "low_high_articulation",
            "Low/high articulation",
            ("humanize_050", "Humanize 50%"),
        ),
        "tube_ref_low_high_articulation_humanize_100" => (
            "low_high_articulation",
            "Low/high articulation",
            ("humanize_100", "Humanize 100%"),
        ),
        _ => return None,
    };

    Some(vec![
        family_axis(ResonatorFamily::Tube),
        AxisCoord::new("reference_gesture", gesture_value, gesture_label),
        AxisCoord::new("reference_variant", variant.0, variant.1),
    ])
}

pub(super) fn tube_low_register_pitch_axes(case: &CatalogCase) -> Option<Vec<AxisCoord>> {
    if case.group_id != "tube_low_register_pitch" {
        return None;
    }
    Some(vec![
        family_axis(ResonatorFamily::Tube),
        AxisCoord::new("pitch_probe", "low_register", "Low-register pitch"),
        register_axis(case.schedule.notes)?,
    ])
}

pub(super) fn tube_register_key_pitch_axes(case: &CatalogCase) -> Option<Vec<AxisCoord>> {
    if case.group_id != "tube_register_key_pitch" {
        return None;
    }
    Some(vec![
        family_axis(ResonatorFamily::Tube),
        AxisCoord::new("pitch_probe", "register_key", "Register-key pitch"),
        register_axis(case.schedule.notes)?,
    ])
}
