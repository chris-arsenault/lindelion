//! Tube recipe render targets and reference-match patch values.
#![allow(clippy::wildcard_imports)]

use super::*;

/// Render targets for the Tube recipe family (split from `target_for_recipe` for the
/// function-size lint).
pub(crate) fn tube_target_for_recipe(recipe: PatchRecipe) -> RenderTarget {
    match recipe {
        PatchRecipe::TubePhrase {
            retrigger,
            bell,
            reed_aperture,
            ..
        } => {
            let patch = TubePatch {
                bell: if bell { TubePatch::default().bell } else { 0.0 },
                reed_aperture_inertia: reed_aperture_inertia(reed_aperture),
                selected_articulation: if retrigger { 0 } else { 2 },
                switches: TubeModelSwitchPatch {
                    bell_enabled: bell,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        PatchRecipe::TubePathAuditPhrase {
            bell_enabled,
            body_enabled,
        } => {
            let patch = TubePatch {
                bell: if bell_enabled {
                    TubePatch::default().bell
                } else {
                    0.0
                },
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                selected_articulation: 2,
                switches: TubeModelSwitchPatch {
                    bell_enabled,
                    body_enabled,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        recipe @ (PatchRecipe::TubeBodyFormantPhrase { .. }
        | PatchRecipe::TubeBodyFormantMixPhrase { .. }
        | PatchRecipe::TubeRadiationShapePhrase { .. }
        | PatchRecipe::TubeBoreSteepeningPhrase { .. }
        | PatchRecipe::TubeReferenceMatchPhrase { .. }) => tube_voicing_target_for_recipe(recipe),
        _ => unreachable!("tube_target_for_recipe only receives Tube recipes"),
    }
}

/// Render targets for the Tube body/voicing audition recipes (split from
/// `tube_target_for_recipe` for the function-size lint).
pub(crate) fn tube_voicing_target_for_recipe(recipe: PatchRecipe) -> RenderTarget {
    match recipe {
        PatchRecipe::TubeBodyFormantPhrase {
            bell_enabled,
            level,
        } => {
            let patch = TubePatch {
                bell: if bell_enabled {
                    TubePatch::default().bell
                } else {
                    0.0
                },
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                body_formant: tube_body_formant_level(level),
                selected_articulation: 2,
                switches: TubeModelSwitchPatch {
                    bell_enabled,
                    body_enabled: true,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        PatchRecipe::TubeBodyFormantMixPhrase { bell, level } => {
            let (bell_enabled, bell_gain) = tube_bell_level(bell);
            let patch = TubePatch {
                bell: bell_gain,
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                body_formant: tube_body_formant_level(level),
                selected_articulation: 2,
                switches: TubeModelSwitchPatch {
                    bell_enabled,
                    body_enabled: true,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        PatchRecipe::TubeRadiationShapePhrase { bell, shape } => {
            let (bell_enabled, bell_gain) = tube_bell_level(bell);
            let patch = TubePatch {
                bell: bell_gain,
                bell_radiation_shape: tube_radiation_shape_value(shape),
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                body_formant: tube_body_formant_level(TubeBodyFormantLevel::Strong),
                selected_articulation: 2,
                switches: TubeModelSwitchPatch {
                    bell_enabled,
                    body_enabled: true,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        PatchRecipe::TubeBoreSteepeningPhrase {
            bell,
            steepening_enabled,
        } => {
            let (bell_enabled, bell_gain) = tube_bell_level(bell);
            let patch = TubePatch {
                bell: bell_gain,
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                body_formant: tube_body_formant_level(TubeBodyFormantLevel::Strong),
                selected_articulation: 2,
                switches: TubeModelSwitchPatch {
                    bell_enabled,
                    body_enabled: true,
                    bore_steepening_enabled: steepening_enabled,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        recipe @ PatchRecipe::TubeReferenceMatchPhrase { .. } => {
            tube_reference_match_target(recipe)
        }
        _ => unreachable!("tube_voicing_target_for_recipe only receives Tube voicing recipes"),
    }
}

pub(crate) fn tube_reference_match_target(recipe: PatchRecipe) -> RenderTarget {
    match recipe {
        PatchRecipe::TubeReferenceMatchPhrase {
            articulation,
            gain,
            humanize,
            body_enabled,
            reed_radiation_enabled,
        } => {
            let patch = TubePatch {
                pressure: tube_reference_match_pressure(gain),
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                body_formant: tube_body_formant_level(TubeBodyFormantLevel::Strong),
                humanize: tube_reference_humanize_value(humanize),
                output_gain_db: TubePatch::default().output_gain_db
                    + tube_reference_match_gain_db(gain),
                selected_articulation: tube_reference_articulation_slot(articulation),
                switches: TubeModelSwitchPatch {
                    bell_enabled: true,
                    body_enabled,
                    reed_radiation_enabled,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        _ => unreachable!("tube_reference_match_target only receives the reference-match recipe"),
    }
}

pub(crate) fn tube_driver_patch(driver: DriverRecipe) -> TubePatch {
    match driver {
        DriverRecipe::ReedSoft => TubePatch {
            pressure: 0.35,
            reed_stiffness: 0.35,
            embouchure: 0.45,
            brightness: 0.35,
            ..TubePatch::default()
        },
        DriverRecipe::ReedHard => TubePatch {
            pressure: 0.90,
            reed_stiffness: 0.80,
            embouchure: 0.55,
            brightness: 0.80,
            ..TubePatch::default()
        },
        _ => TubePatch::default(),
    }
}

pub(crate) fn reed_aperture_inertia(reed_aperture: TubeReedAperture) -> f32 {
    match reed_aperture {
        TubeReedAperture::Instant => 0.0,
        TubeReedAperture::Inertial => 1.0,
    }
}

pub(crate) fn tube_body_formant_level(level: TubeBodyFormantLevel) -> f32 {
    match level {
        TubeBodyFormantLevel::Current => 0.0,
        TubeBodyFormantLevel::Medium => 0.55,
        TubeBodyFormantLevel::Strong => 1.0,
    }
}

pub(crate) fn tube_bell_level(level: TubeBellLevel) -> (bool, f32) {
    match level {
        TubeBellLevel::Off => (false, 0.0),
        TubeBellLevel::Nominal => (true, TubePatch::default().bell),
        TubeBellLevel::Full => (true, 1.0),
    }
}

pub(crate) fn tube_radiation_shape_value(shape: TubeRadiationShape) -> f32 {
    match shape {
        TubeRadiationShape::Current => 0.0,
        TubeRadiationShape::Gentle => 1.0,
    }
}

pub(crate) fn tube_reference_articulation_slot(articulation: TubeReferenceArticulation) -> usize {
    match articulation {
        TubeReferenceArticulation::Legato => 2,
        TubeReferenceArticulation::Tongue => 0,
    }
}

pub(crate) fn tube_reference_match_gain_db(gain: TubeReferenceMatchGain) -> f32 {
    match gain {
        TubeReferenceMatchGain::LowESustainPhysical => 15.9,
        TubeReferenceMatchGain::LowESustainBodyOff => 3.8,
        TubeReferenceMatchGain::RegisterKeyHighSustainVented => 14.1,
        TubeReferenceMatchGain::LowHighArticulation => 8.35,
    }
}

pub(crate) fn tube_reference_match_pressure(gain: TubeReferenceMatchGain) -> f32 {
    match gain {
        TubeReferenceMatchGain::LowESustainPhysical
        | TubeReferenceMatchGain::LowESustainBodyOff => TubePatch::default().pressure,
        _ => TubePatch::default().pressure,
    }
}

pub(crate) fn tube_reference_humanize_value(humanize: TubeReferenceHumanize) -> f32 {
    match humanize {
        TubeReferenceHumanize::Off => 0.0,
        TubeReferenceHumanize::Medium => 0.5,
        TubeReferenceHumanize::Full => 1.0,
    }
}
