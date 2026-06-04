use crate::catalog::{
    CATALOG_BLOCK_SIZE, CATALOG_SAMPLE_RATE, CatalogCase, ContactRecipe, DriverRecipe, EdgeRecipe,
    MeshVoicing, PatchRecipe, ResonatorFamily, ScheduledNote, SourceBodyDepth, SurroundingRecipe,
};
use lamath::{ModalConfig, ModalPreset, ResonatorRouting, ResonatorSynth, ResonatorSynthPatch};
use lamath_cymbal::{CymbalExcitationSource, CymbalPatch, CymbalProcessor};
use lamath_stringed::{
    BodySelection, DriverSelection, StringExcitationSource, StringPatch, StringProcessor,
};
use lamath_tube::{TubeExcitationSource, TubeModelSwitchPatch, TubePatch, TubeProcessor};
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
    let target_frames = case.target_frames();
    let target = target_for_recipe(case.patch_recipe);
    let (left, right) = render_target(target, case.schedule.notes, target_frames);
    let metrics =
        validate_wav_stereo_pcm16(&left, &right, CATALOG_SAMPLE_RATE).map_err(RenderError::Wav)?;
    Ok(RenderedCase {
        left,
        right,
        metrics,
    })
}

#[derive(Debug, Clone)]
enum RenderTarget {
    Modal(ResonatorSynthPatch),
    Cymbal(CymbalPatch),
    Tube(TubePatch),
    Stringed(StringPatch),
}

fn render_target(
    target: RenderTarget,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    match target {
        RenderTarget::Modal(patch) => render_modal(patch, notes, target_frames),
        RenderTarget::Cymbal(patch) => render_cymbal(patch, notes, target_frames),
        RenderTarget::Tube(patch) => render_tube(patch, notes, target_frames),
        RenderTarget::Stringed(patch) => render_stringed(patch, notes, target_frames),
    }
}

fn render_modal(
    patch: ResonatorSynthPatch,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let setup = ProcessSetup {
        sample_rate: f64::from(CATALOG_SAMPLE_RATE),
        max_block_size: CATALOG_BLOCK_SIZE,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_with_loaded_excitations(patch, Vec::new());
    render_blocks(notes, target_frames, |events, left, right| {
        synth.process(ProcessContext::new(
            setup,
            AudioBuffer { left, right },
            events,
        ));
    })
}

fn render_cymbal(
    patch: CymbalPatch,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let mut processor = CymbalProcessor::new(
        CATALOG_SAMPLE_RATE as f32,
        patch,
        CymbalExcitationSource::builtin(),
    );
    render_blocks(notes, target_frames, |events, left, right| {
        processor.process(events, left, right);
    })
}

fn render_tube(
    patch: TubePatch,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let mut processor = TubeProcessor::new(
        CATALOG_SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(TubeExcitationSource::builtin),
    );
    render_blocks(notes, target_frames, |events, left, right| {
        processor.process(events, left, right);
    })
}

fn render_stringed(
    patch: StringPatch,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let mut processor = StringProcessor::new(
        CATALOG_SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(StringExcitationSource::builtin),
    );
    render_blocks(notes, target_frames, |events, left, right| {
        processor.process(events, left, right);
    })
}

fn render_blocks(
    notes: &[ScheduledNote],
    target_frames: usize,
    mut process: impl FnMut(&[MidiEvent], &mut [f32], &mut [f32]),
) -> (Vec<f32>, Vec<f32>) {
    let total_blocks = target_frames.div_ceil(CATALOG_BLOCK_SIZE);
    let mut block_left = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut block_right = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut left = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut right = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut actions = scheduled_actions(notes, total_blocks);
    let mut events = Vec::with_capacity(notes.len() * 2);
    let mut cursor = 0;

    actions.sort_by_key(|action| (action.block, action.order));
    for block in 0..total_blocks {
        events.clear();
        while cursor < actions.len() && actions[cursor].block == block {
            events.push(actions[cursor].event);
            cursor += 1;
        }
        process(&events, &mut block_left, &mut block_right);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    left.truncate(target_frames);
    right.truncate(target_frames);
    (left, right)
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

fn target_for_recipe(recipe: PatchRecipe) -> RenderTarget {
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
        PatchRecipe::Surrounding {
            family,
            surrounding,
        } => surrounding_target(family, surrounding),
        PatchRecipe::Edge(recipe) => edge_target(recipe),
        PatchRecipe::TubePhrase {
            retrigger, bell, ..
        } => {
            let patch = TubePatch {
                bell: if bell { 1.0 } else { 0.0 },
                selected_articulation: if retrigger { 0 } else { 2 },
                switches: TubeModelSwitchPatch {
                    bell_enabled: bell,
                    ..TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            };
            RenderTarget::Tube(patch)
        }
        PatchRecipe::MeshPhrase { .. } => {
            RenderTarget::Cymbal(cymbal_voicing_patch(MeshVoicing::Ride))
        }
        PatchRecipe::MeshVoicing(voicing) => RenderTarget::Cymbal(cymbal_voicing_patch(voicing)),
    }
}

fn family_target(family: ResonatorFamily) -> RenderTarget {
    match family {
        ResonatorFamily::Modal => RenderTarget::Modal(modal_patch()),
        ResonatorFamily::String => RenderTarget::Stringed(StringPatch::default()),
        ResonatorFamily::Tube => RenderTarget::Tube(TubePatch::default()),
        ResonatorFamily::Mesh => RenderTarget::Cymbal(CymbalPatch::default()),
    }
}

fn driver_target(family: ResonatorFamily, driver: DriverRecipe) -> RenderTarget {
    match family {
        ResonatorFamily::String => RenderTarget::Stringed(string_driver_patch(driver)),
        ResonatorFamily::Tube => RenderTarget::Tube(tube_driver_patch(driver)),
        other => family_target(other),
    }
}

fn contact_target(family: ResonatorFamily, contact: ContactRecipe) -> RenderTarget {
    match family {
        ResonatorFamily::String => RenderTarget::Stringed(string_contact_patch(contact)),
        other => family_target(other),
    }
}

fn surrounding_target(family: ResonatorFamily, surrounding: SurroundingRecipe) -> RenderTarget {
    let mut target = family_target(family);
    if let RenderTarget::Modal(patch) = &mut target {
        patch.surrounding = surrounding_config(surrounding);
    }
    target
}

fn string_driver_patch(driver: DriverRecipe) -> StringPatch {
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
        DriverRecipe::BowSmooth => StringPatch {
            driver: DriverSelection::Bow,
            body: BodySelection::Violin,
            brightness: 0.45,
            stiffness: 0.35,
            damping: 0.55,
            ..StringPatch::default()
        },
        DriverRecipe::BowScratch => StringPatch {
            driver: DriverSelection::Bow,
            body: BodySelection::Violin,
            brightness: 0.95,
            stiffness: 0.80,
            damping: 0.18,
            ..StringPatch::default()
        },
        DriverRecipe::ReedSoft | DriverRecipe::ReedHard => StringPatch::default(),
    }
}

fn tube_driver_patch(driver: DriverRecipe) -> TubePatch {
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

fn string_contact_patch(contact: ContactRecipe) -> StringPatch {
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

fn source_body_depth_value(depth: SourceBodyDepth) -> f32 {
    match depth {
        SourceBodyDepth::Depth000 => 0.0,
        SourceBodyDepth::Depth050 => 0.5,
        SourceBodyDepth::Depth100 => 1.0,
    }
}

fn edge_target(recipe: EdgeRecipe) -> RenderTarget {
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

fn cymbal_voicing_patch(voicing: MeshVoicing) -> CymbalPatch {
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
        MeshVoicing::Crash => CymbalPatch {
            size: 0.95,
            tension: 0.88,
            damping: 0.10,
            material: 0.40,
            strike_position: 0.9,
            output_gain_db: -5.0,
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

fn modal_patch() -> ResonatorSynthPatch {
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

fn modal_bright_long_decay_patch() -> ResonatorSynthPatch {
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

fn surrounding_config(recipe: SurroundingRecipe) -> lamath::SurroundingConfig {
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

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav(error) => write!(formatter, "rendered WAV validation failed: {error:?}"),
        }
    }
}
