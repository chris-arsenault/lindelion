use crate::catalog::{
    CATALOG_BLOCK_SIZE, CATALOG_SAMPLE_RATE, CatalogCase, ContactRecipe, DriverRecipe, EdgeRecipe,
    PatchRecipe, ResonatorFamily, ScheduledNote, SourceBodyDepth, SurroundingRecipe,
};
use lamath::{
    BowConfig, ContactConfig, DriverConfig, MeshConfig, ModalConfig, ModalPreset, PickConfig,
    ReedConfig, ResonatorConfig, ResonatorRouting, ResonatorSynth, ResonatorSynthPatch,
    SurroundingConfig, WaveguideConfig, WaveguideStyle,
};
use lindelion_plugin_shell::{
    AudioBuffer, AudioPlugin, MidiEvent, NoteEvent, ProcessContext, ProcessMode, ProcessSetup,
};
use lindelion_sample_library::{
    StereoPcm16WavError, StereoPcm16WavMetrics, validate_wav_stereo_pcm16,
};
use std::fmt;

#[derive(Debug, Clone)]
pub(crate) struct RenderedCase {
    pub(crate) left: Vec<f32>,
    pub(crate) right: Vec<f32>,
    pub(crate) metrics: StereoPcm16WavMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RenderError {
    Wav(StereoPcm16WavError),
}

pub(crate) fn render_case(case: &CatalogCase) -> Result<RenderedCase, RenderError> {
    let setup = ProcessSetup {
        sample_rate: f64::from(CATALOG_SAMPLE_RATE),
        max_block_size: CATALOG_BLOCK_SIZE,
        mode: ProcessMode::Realtime,
    };
    let target_frames = case.target_frames();
    let total_blocks = target_frames.div_ceil(CATALOG_BLOCK_SIZE);
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut block_right = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut left = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut right = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut events = Vec::with_capacity(case.schedule.notes.len() * 2);
    let mut actions = scheduled_actions(case.schedule.notes, total_blocks);
    let mut cursor = 0;

    actions.sort_by_key(|action| (action.block, action.order));
    synth.reset(setup);
    synth.set_patch_with_loaded_excitations(patch_for_recipe(case.patch_recipe), Vec::new());

    for block in 0..total_blocks {
        events.clear();
        while cursor < actions.len() && actions[cursor].block == block {
            events.push(actions[cursor].event);
            cursor += 1;
        }
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &events,
        );
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    left.truncate(target_frames);
    right.truncate(target_frames);
    let metrics =
        validate_wav_stereo_pcm16(&left, &right, CATALOG_SAMPLE_RATE).map_err(RenderError::Wav)?;
    Ok(RenderedCase {
        left,
        right,
        metrics,
    })
}

#[derive(Debug, Clone, Copy)]
struct ScheduledRenderAction {
    block: usize,
    order: usize,
    event: MidiEvent,
}

fn scheduled_actions(notes: &[ScheduledNote], total_blocks: usize) -> Vec<ScheduledRenderAction> {
    let mut actions = Vec::with_capacity(notes.len() * 2);
    for (index, note) in notes.iter().enumerate() {
        actions.push(ScheduledRenderAction {
            block: block_at(note.start_seconds, total_blocks),
            order: index * 2,
            event: MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: note.note,
                velocity: note.velocity,
            }),
        });
        actions.push(ScheduledRenderAction {
            block: block_at(note.end_seconds, total_blocks),
            order: index * 2 + 1,
            event: MidiEvent::Note(NoteEvent::Off {
                channel: 0,
                note: note.note,
                velocity: 0.0,
            }),
        });
    }
    actions
}

fn block_at(seconds: f32, total_blocks: usize) -> usize {
    ((seconds * CATALOG_SAMPLE_RATE as f32) as usize / CATALOG_BLOCK_SIZE)
        .min(total_blocks.saturating_sub(1))
}

fn process_block(
    synth: &mut ResonatorSynth,
    setup: ProcessSetup,
    left: &mut [f32],
    right: &mut [f32],
    events: &[MidiEvent],
) {
    synth.process(ProcessContext::new(
        setup,
        AudioBuffer { left, right },
        events,
    ));
}

fn patch_for_recipe(recipe: PatchRecipe) -> ResonatorSynthPatch {
    match recipe {
        PatchRecipe::SingleFamily(family) => single_family_patch(family),
        PatchRecipe::Driver { family, driver } => {
            let mut patch = single_family_patch(family);
            patch.driver = driver_config(driver);
            patch
        }
        PatchRecipe::Contact { family, contact } => {
            let mut patch = single_family_patch(family);
            patch.contact = contact_config(contact);
            patch
        }
        PatchRecipe::SourceBodyBalance { depth } => source_body_balance_patch(depth),
        PatchRecipe::Surrounding {
            family,
            surrounding,
        } => {
            let mut patch = single_family_patch(family);
            patch.surrounding = surrounding_config(surrounding);
            patch
        }
        PatchRecipe::Edge(recipe) => edge_patch(recipe),
    }
}

fn single_family_patch(family: ResonatorFamily) -> ResonatorSynthPatch {
    let mut patch = ResonatorSynthPatch {
        resonator_a: family_resonator_config(family),
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

fn family_resonator_config(family: ResonatorFamily) -> ResonatorConfig {
    match family {
        ResonatorFamily::Modal => ResonatorConfig::Modal(ModalConfig::default()),
        ResonatorFamily::String => ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::String,
            ..WaveguideConfig::default()
        }),
        ResonatorFamily::Tube => ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::Tube,
            ..WaveguideConfig::default()
        }),
        ResonatorFamily::Mesh => ResonatorConfig::Mesh(MeshConfig::default()),
    }
}

fn driver_config(recipe: DriverRecipe) -> DriverConfig {
    match recipe {
        DriverRecipe::Sample => DriverConfig::Sample,
        DriverRecipe::PickSoft => DriverConfig::Pick(PickConfig {
            hardness: 0.25,
            contact_time: 0.70,
        }),
        DriverRecipe::PickHard => DriverConfig::Pick(PickConfig {
            hardness: 0.85,
            contact_time: 0.20,
        }),
        DriverRecipe::BowSmooth => DriverConfig::Bow(BowConfig {
            pressure_depth: 0.55,
            bow_speed: 0.45,
            friction: 0.25,
        }),
        DriverRecipe::BowScratch => DriverConfig::Bow(BowConfig {
            pressure_depth: 0.75,
            bow_speed: 0.65,
            friction: 0.90,
        }),
        DriverRecipe::ReedSoft => DriverConfig::Reed(ReedConfig {
            pressure_depth: 0.35,
            stiffness: 0.35,
            embouchure: 0.45,
        }),
        DriverRecipe::ReedHard => DriverConfig::Reed(ReedConfig {
            pressure_depth: 0.80,
            stiffness: 0.80,
            embouchure: 0.55,
        }),
    }
}

fn contact_config(recipe: ContactRecipe) -> ContactConfig {
    match recipe {
        ContactRecipe::TightShort => ContactConfig {
            spread: 0.0,
            contact_time: 0.0,
        },
        ContactRecipe::TightLong => ContactConfig {
            spread: 0.0,
            contact_time: 0.75,
        },
        ContactRecipe::WideShort => ContactConfig {
            spread: 0.85,
            contact_time: 0.0,
        },
        ContactRecipe::WideLong => ContactConfig {
            spread: 0.85,
            contact_time: 0.75,
        },
    }
}

fn source_body_balance_patch(depth: SourceBodyDepth) -> ResonatorSynthPatch {
    let mut patch = single_family_patch(ResonatorFamily::String);
    if let ResonatorConfig::Waveguide(config) = &mut patch.resonator_a {
        config.source_body_balance = source_body_depth_value(depth);
    }
    patch
}

fn source_body_depth_value(depth: SourceBodyDepth) -> f32 {
    match depth {
        SourceBodyDepth::Depth000 => 0.0,
        SourceBodyDepth::Depth050 => 0.5,
        SourceBodyDepth::Depth100 => 1.0,
    }
}

fn surrounding_config(recipe: SurroundingRecipe) -> SurroundingConfig {
    match recipe {
        SurroundingRecipe::Off => SurroundingConfig::default(),
        SurroundingRecipe::Mechanical => SurroundingConfig {
            mechanical_noise: 0.70,
            ..SurroundingConfig::default()
        },
        SurroundingRecipe::Radiation => SurroundingConfig {
            radiation_brightness: 0.80,
            ..SurroundingConfig::default()
        },
        SurroundingRecipe::Sympathetic => SurroundingConfig {
            sympathetic: 0.90,
            ..SurroundingConfig::default()
        },
        SurroundingRecipe::MechanicalRadiation => SurroundingConfig {
            mechanical_noise: 0.70,
            radiation_brightness: 0.80,
            ..SurroundingConfig::default()
        },
        SurroundingRecipe::MechanicalSympathetic => SurroundingConfig {
            mechanical_noise: 0.70,
            sympathetic: 0.90,
            ..SurroundingConfig::default()
        },
        SurroundingRecipe::RadiationSympathetic => SurroundingConfig {
            radiation_brightness: 0.80,
            sympathetic: 0.90,
            ..SurroundingConfig::default()
        },
        SurroundingRecipe::All => SurroundingConfig {
            mechanical_noise: 0.70,
            radiation_brightness: 0.80,
            sympathetic: 0.90,
        },
    }
}

fn edge_patch(recipe: EdgeRecipe) -> ResonatorSynthPatch {
    match recipe {
        EdgeRecipe::StringHighLoopGain => {
            let mut patch = single_family_patch(ResonatorFamily::String);
            if let ResonatorConfig::Waveguide(config) = &mut patch.resonator_a {
                config.loop_gain = 0.999;
                config.loop_filter_cutoff = 12_000.0;
            }
            patch
        }
        EdgeRecipe::StringHighDispersion => {
            let mut patch = single_family_patch(ResonatorFamily::String);
            if let ResonatorConfig::Waveguide(config) = &mut patch.resonator_a {
                config.dispersion = 1.0;
                config.loop_gain = 0.985;
            }
            patch
        }
        EdgeRecipe::StringSourceBodyLow => {
            let mut patch = single_family_patch(ResonatorFamily::String);
            if let ResonatorConfig::Waveguide(config) = &mut patch.resonator_a {
                config.source_body_balance = 1.0;
                config.loop_gain = 0.985;
            }
            patch
        }
        EdgeRecipe::TubeClosedNonlinear => {
            let mut patch = single_family_patch(ResonatorFamily::Tube);
            if let ResonatorConfig::Waveguide(config) = &mut patch.resonator_a {
                config.boundary_reflection = -1.0;
                config.loop_nonlinearity = 1.0;
                config.loop_gain = 0.985;
            }
            patch
        }
        EdgeRecipe::TubeOpenNonlinear => {
            let mut patch = single_family_patch(ResonatorFamily::Tube);
            if let ResonatorConfig::Waveguide(config) = &mut patch.resonator_a {
                config.boundary_reflection = 1.0;
                config.loop_nonlinearity = 1.0;
                config.loop_gain = 0.985;
            }
            patch
        }
        EdgeRecipe::MeshLowDampingHighMaterial => {
            let mut patch = single_family_patch(ResonatorFamily::Mesh);
            if let ResonatorConfig::Mesh(config) = &mut patch.resonator_a {
                config.material = 1.0;
                config.damping = 0.0;
            }
            patch
        }
        EdgeRecipe::ModalBrightLongDecay => {
            let mut patch = single_family_patch(ResonatorFamily::Modal);
            patch.resonator_a = ResonatorConfig::Modal(ModalConfig {
                preset: ModalPreset::Bell,
                inharmonicity: 0.8,
                brightness: 1.0,
                decay_global: 2.0,
                decay_tilt: 0.0,
                position_of_strike: 0.37,
                ..ModalConfig::default()
            });
            patch
        }
        EdgeRecipe::StringDenseHardChord => {
            let mut patch = single_family_patch(ResonatorFamily::String);
            patch.polyphony = 16;
            patch
        }
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav(error) => write!(formatter, "rendered WAV validation failed: {error:?}"),
        }
    }
}
