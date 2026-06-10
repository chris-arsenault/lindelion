//! Recipe-to-render-target mapping and per-family audition patches.
#![allow(clippy::wildcard_imports)]

use super::tube_targets::{tube_driver_patch, tube_target_for_recipe};
use super::*;

pub(crate) fn target_for_recipe(recipe: PatchRecipe) -> RenderTarget {
    match recipe {
        PatchRecipe::SingleFamily(family) => family_target(family),
        PatchRecipe::Driver { family, driver } => driver_target(family, driver),
        PatchRecipe::Contact { family, contact } => contact_target(family, contact),
        PatchRecipe::SourceBodyBalance { depth } => {
            let patch = StringPatch {
                body_balance: source_body_depth_value(depth),
                ..StringPatch::default()
            };
            RenderTarget::Stringed(patch)
        }
        PatchRecipe::StringBowAlternatingScale => RenderTarget::StringedBowAlternatingScale,
        PatchRecipe::StringBowHumanize { depth } => {
            let patch = StringPatch {
                humanize: match depth {
                    BowHumanizeDepth::Off => 0.0,
                    BowHumanizeDepth::Full => 1.0,
                },
                ..string_driver_patch(DriverRecipe::BowSmooth)
            };
            RenderTarget::Stringed(patch)
        }
        PatchRecipe::Surrounding {
            family,
            surrounding,
        } => surrounding_target(family, surrounding),
        PatchRecipe::ReferenceWav { path } => RenderTarget::ReferenceWav(path),
        PatchRecipe::Edge(recipe) => edge_target(recipe),
        recipe @ (PatchRecipe::TubePhrase { .. }
        | PatchRecipe::TubePathAuditPhrase { .. }
        | PatchRecipe::TubeBodyFormantPhrase { .. }
        | PatchRecipe::TubeBodyFormantMixPhrase { .. }
        | PatchRecipe::TubeRadiationShapePhrase { .. }
        | PatchRecipe::TubeBoreSteepeningPhrase { .. }
        | PatchRecipe::TubeReferenceMatchPhrase { .. }) => tube_target_for_recipe(recipe),
        PatchRecipe::MeshPhrase { .. } => {
            RenderTarget::Cymbal(cymbal_voicing_patch(MeshVoicing::Ride))
        }
        PatchRecipe::MeshVoicing(voicing) => RenderTarget::Cymbal(cymbal_voicing_patch(voicing)),
        PatchRecipe::MeshStriker { voicing, striker } => {
            let mut patch = cymbal_voicing_patch(voicing);
            patch.selected_striker = mesh_striker_slot(striker);
            RenderTarget::Cymbal(patch)
        }
    }
}

pub(crate) fn family_target(family: ResonatorFamily) -> RenderTarget {
    match family {
        ResonatorFamily::Modal => RenderTarget::Modal(modal_patch()),
        ResonatorFamily::String => RenderTarget::Stringed(StringPatch::default()),
        ResonatorFamily::Tube => RenderTarget::Tube(TubePatch::default()),
        ResonatorFamily::Mesh => RenderTarget::Cymbal(CymbalPatch::default()),
    }
}

pub(crate) fn driver_target(family: ResonatorFamily, driver: DriverRecipe) -> RenderTarget {
    match family {
        ResonatorFamily::String => RenderTarget::Stringed(string_driver_patch(driver)),
        ResonatorFamily::Tube => RenderTarget::Tube(tube_driver_patch(driver)),
        other => family_target(other),
    }
}

pub(crate) fn contact_target(family: ResonatorFamily, contact: ContactRecipe) -> RenderTarget {
    match family {
        ResonatorFamily::String => RenderTarget::Stringed(string_contact_patch(contact)),
        other => family_target(other),
    }
}

pub(crate) fn surrounding_target(
    family: ResonatorFamily,
    surrounding: SurroundingRecipe,
) -> RenderTarget {
    let mut target = family_target(family);
    if let RenderTarget::Modal(patch) = &mut target {
        patch.surrounding = surrounding_config(surrounding);
    }
    target
}

pub(crate) fn string_driver_patch(driver: DriverRecipe) -> StringPatch {
    match driver {
        DriverRecipe::Sample => StringPatch {
            driver: DriverSelection::None,
            body_balance: 0.15,
            ..StringPatch::default()
        },
        DriverRecipe::PickSoft => StringPatch {
            driver: DriverSelection::Pick,
            brightness: 0.35,
            stiffness: 0.35,
            damping: 0.45,
            ..StringPatch::default()
        },
        DriverRecipe::PickHard => StringPatch {
            driver: DriverSelection::Pick,
            brightness: 0.90,
            stiffness: 0.95,
            damping: 0.18,
            ..StringPatch::default()
        },
        // The bow recipes run the unmodified instrument path: no output makeup
        // gain (the bowed body-radiation level is calibrated in the model) and
        // tension modulation on (it reads the string's own stored energy, so a
        // sustained bow no longer detunes the way the old output-energy bus did).
        DriverRecipe::BowSmooth => StringPatch {
            driver: DriverSelection::Bow,
            body: BodySelection::Violin,
            brightness: 0.50,
            stiffness: 0.12,
            damping: 0.34,
            bow_position: 0.16,
            bow_pressure: 0.58,
            bow_speed: 0.36,
            bow_friction: 0.40,
            pickup_position: 0.50,
            body_balance: 0.24,
            ..StringPatch::default()
        },
        // Scratch = bowing at the chaos boundary. The Schelleng overpressure
        // ratio `N / (2·Z₀·v_b/(β(μs−μd)))` is the dial, and the periodicity
        // cliff is *sharp*: a measured pressure scan at this speed/position
        // put p=0.54 fully periodic (0.996 autocorrelation at the fundamental
        // lag), p=0.56 at 0.72 with 3.6× the smooth bow's envelope roughness
        // (a raucous note — the target), and p≥0.62 below 0.36 (chaotic
        // crunch with the note buried; ≥2× over the line stops pitched
        // oscillation entirely, leaving only sub-f0 creak). The effort scale
        // multiplies speed and force together, so the regime is
        // velocity-invariant.
        DriverRecipe::BowScratch => StringPatch {
            driver: DriverSelection::Bow,
            body: BodySelection::Violin,
            brightness: 0.58,
            stiffness: 0.42,
            damping: 0.40,
            bow_position: 0.20,
            bow_pressure: 0.48,
            bow_speed: 0.32,
            bow_friction: 0.70,
            ..StringPatch::default()
        },
        DriverRecipe::ReedSoft | DriverRecipe::ReedHard => StringPatch::default(),
    }
}

pub(crate) fn mesh_striker_slot(striker: MeshStriker) -> usize {
    match striker {
        MeshStriker::HardStick => 0,
        MeshStriker::SoftMallet => 1,
        MeshStriker::JazzBrush => 2,
        MeshStriker::BellStick => 3,
    }
}

pub(crate) fn string_contact_patch(contact: ContactRecipe) -> StringPatch {
    match contact {
        ContactRecipe::TightShort => StringPatch {
            strike_position: 0.48,
            brightness: 0.82,
            damping: 0.24,
            ..StringPatch::default()
        },
        ContactRecipe::TightLong => StringPatch {
            strike_position: 0.48,
            brightness: 0.30,
            damping: 0.58,
            ..StringPatch::default()
        },
        ContactRecipe::WideShort => StringPatch {
            strike_position: 0.18,
            brightness: 0.76,
            damping: 0.30,
            ..StringPatch::default()
        },
        ContactRecipe::WideLong => StringPatch {
            strike_position: 0.18,
            brightness: 0.24,
            damping: 0.62,
            ..StringPatch::default()
        },
    }
}

pub(crate) fn source_body_depth_value(depth: SourceBodyDepth) -> f32 {
    match depth {
        SourceBodyDepth::Depth000 => 0.0,
        SourceBodyDepth::Depth050 => 0.5,
        SourceBodyDepth::Depth100 => 1.0,
    }
}

pub(crate) fn edge_target(recipe: EdgeRecipe) -> RenderTarget {
    match recipe {
        EdgeRecipe::StringHighLoopGain => RenderTarget::Stringed(StringPatch {
            damping: 0.02,
            brightness: 0.70,
            ..StringPatch::default()
        }),
        EdgeRecipe::StringHighDispersion => RenderTarget::Stringed(StringPatch {
            stiffness: 1.0,
            damping: 0.08,
            ..StringPatch::default()
        }),
        EdgeRecipe::StringSourceBodyLow => RenderTarget::Stringed(StringPatch {
            body_balance: 1.0,
            damping: 0.08,
            ..StringPatch::default()
        }),
        EdgeRecipe::TubeClosedNonlinear => RenderTarget::Tube(TubePatch {
            bell: 0.0,
            pressure: 0.90,
            damping: 0.05,
            switches: TubeModelSwitchPatch {
                bell_enabled: false,
                ..TubeModelSwitchPatch::default()
            },
            ..TubePatch::default()
        }),
        EdgeRecipe::TubeOpenNonlinear => RenderTarget::Tube(TubePatch {
            bell: 1.0,
            pressure: 0.90,
            damping: 0.05,
            ..TubePatch::default()
        }),
        EdgeRecipe::MeshLowDampingHighMaterial => RenderTarget::Cymbal(CymbalPatch {
            material: 1.0,
            damping: 0.0,
            ..CymbalPatch::default()
        }),
        EdgeRecipe::ModalBrightLongDecay => RenderTarget::Modal(modal_bright_long_decay_patch()),
        EdgeRecipe::StringDenseHardChord => RenderTarget::Stringed(StringPatch {
            brightness: 0.95,
            stiffness: 0.95,
            damping: 0.04,
            ..StringPatch::default()
        }),
    }
}

pub(crate) fn cymbal_voicing_patch(voicing: MeshVoicing) -> CymbalPatch {
    match voicing {
        MeshVoicing::Triangle => CymbalPatch {
            size: 0.12,
            tension: 0.12,
            damping: 0.18,
            material: 0.70,
            strike_position: 0.5,
            ..CymbalPatch::default()
        },
        MeshVoicing::Ride => CymbalPatch {
            size: 0.5,
            tension: 0.45,
            damping: 0.35,
            material: 0.6,
            strike_position: 0.72,
            ..CymbalPatch::default()
        },
        MeshVoicing::KitRide => CymbalPatch {
            size: 0.86,
            tension: 0.84,
            damping: 0.56,
            material: 0.95,
            strike_position: 0.82,
            pickup_spread: 0.24,
            output_gain_db: -5.0,
            ..CymbalPatch::default()
        },
        MeshVoicing::Crash => CymbalPatch {
            size: 0.95,
            tension: 0.88,
            damping: 0.10,
            material: 0.40,
            strike_position: 0.9,
            output_gain_db: -5.0,
            ..CymbalPatch::default()
        },
        MeshVoicing::KitCrash => CymbalPatch {
            size: 0.98,
            tension: 0.98,
            damping: 0.14,
            material: 0.95,
            strike_position: 0.9,
            pickup_spread: 0.18,
            output_gain_db: -8.0,
            ..CymbalPatch::default()
        },
        MeshVoicing::DensitySparse => CymbalPatch {
            size: 0.08,
            tension: 0.08,
            damping: 0.30,
            material: 0.5,
            ..CymbalPatch::default()
        },
        MeshVoicing::DensityDense => CymbalPatch {
            size: 0.97,
            tension: 0.92,
            damping: 0.30,
            material: 0.5,
            ..CymbalPatch::default()
        },
        MeshVoicing::DecayShort => CymbalPatch {
            size: 0.5,
            tension: 0.5,
            damping: 0.85,
            material: 0.5,
            ..CymbalPatch::default()
        },
        MeshVoicing::DecayLong => CymbalPatch {
            size: 0.5,
            tension: 0.5,
            damping: 0.05,
            material: 0.5,
            output_gain_db: -5.0,
            ..CymbalPatch::default()
        },
    }
}

pub(crate) fn modal_patch() -> ResonatorSynthPatch {
    let mut patch = ResonatorSynthPatch {
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        ..ResonatorSynthPatch::default()
    };
    patch.output.master_gain_db = 0.0;
    patch.output.saturation_drive = 0.0;
    patch.output.filter_cutoff = 20_000.0;
    patch
}

pub(crate) fn modal_bright_long_decay_patch() -> ResonatorSynthPatch {
    let mut patch = modal_patch();
    patch.resonator_a = ModalConfig {
        preset: ModalPreset::Bell,
        inharmonicity: 0.8,
        brightness: 1.0,
        decay_global: 2.0,
        decay_tilt: 0.0,
        position_of_strike: 0.37,
        ..ModalConfig::default()
    };
    patch
}

pub(crate) fn surrounding_config(recipe: SurroundingRecipe) -> lamath::SurroundingConfig {
    match recipe {
        SurroundingRecipe::Off => lamath::SurroundingConfig::default(),
        SurroundingRecipe::Mechanical => lamath::SurroundingConfig {
            mechanical_noise: 0.70,
            ..lamath::SurroundingConfig::default()
        },
        SurroundingRecipe::Radiation => lamath::SurroundingConfig {
            radiation_brightness: 0.80,
            ..lamath::SurroundingConfig::default()
        },
        SurroundingRecipe::Sympathetic => lamath::SurroundingConfig {
            sympathetic: 0.90,
            ..lamath::SurroundingConfig::default()
        },
        SurroundingRecipe::MechanicalRadiation => lamath::SurroundingConfig {
            mechanical_noise: 0.70,
            radiation_brightness: 0.80,
            ..lamath::SurroundingConfig::default()
        },
        SurroundingRecipe::MechanicalSympathetic => lamath::SurroundingConfig {
            mechanical_noise: 0.70,
            sympathetic: 0.90,
            ..lamath::SurroundingConfig::default()
        },
        SurroundingRecipe::RadiationSympathetic => lamath::SurroundingConfig {
            radiation_brightness: 0.80,
            sympathetic: 0.90,
            ..lamath::SurroundingConfig::default()
        },
        SurroundingRecipe::All => lamath::SurroundingConfig {
            mechanical_noise: 0.70,
            radiation_brightness: 0.80,
            sympathetic: 0.90,
        },
    }
}
