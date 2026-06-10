use crate::catalog::{
    BowHumanizeDepth, CATALOG_BLOCK_SIZE, CATALOG_SAMPLE_RATE, CatalogCase, ContactRecipe,
    DriverRecipe, EdgeRecipe, MeshStriker, MeshVoicing, PatchRecipe, PhrasingDepth,
    ResonatorFamily, ScheduledNote, SourceBodyDepth, SurroundingRecipe, TubeBellLevel,
    TubeBodyFormantLevel, TubeRadiationShape, TubeReedAperture, TubeReferenceArticulation,
    TubeReferenceHumanize, TubeReferenceMatchGain,
};
use lamath::{ModalConfig, ModalPreset, ResonatorRouting, ResonatorSynth, ResonatorSynthPatch};
use lamath_cymbal::{CymbalExcitationSource, CymbalPatch, CymbalProcessor};
use lamath_stringed::{
    BodySelection, DriverSelection, StringExcitationSource, StringModelProbe, StringPatch,
    StringProcessor,
};
use lamath_tube::{
    TUBE_RENDER_TAP_COUNT, TUBE_RENDER_TAP_NAMES, TubeExcitationSource, TubeModelSwitchPatch,
    TubePatch, TubeProcessor,
};
use lindelion_dsp_utils::{
    analysis::{peak_abs, rms, windowed_dft_magnitude_at},
    math::midi_note_to_hz,
    resampling::WindowedSincResampler,
};
use lindelion_plugin_shell::{
    AudioBuffer, AudioPlugin, MidiEvent, NoteEvent, ProcessContext, ProcessMode, ProcessSetup,
};
use lindelion_sample_library::{
    SampleDecodeError, StereoPcm16WavError, StereoPcm16WavMetrics, decode_wav_mono,
    validate_wav_stereo_pcm16,
};
use std::{fmt, path::Path};

mod bow_diagnostic;
mod targets;
mod tube_tap_analysis;
mod tube_targets;

pub(crate) use bow_diagnostic::{BowDiagnosticRow, analyze_bow_diagnostic};
use targets::{string_driver_patch, target_for_recipe};
pub(crate) use tube_tap_analysis::analyze_tube_taps;
use tube_tap_analysis::{format_db, seconds_to_frame};
use tube_targets::tube_reference_match_gain_db;

#[derive(Debug, Clone)]
pub(crate) struct RenderedCase {
    pub(crate) left: Vec<f32>,
    pub(crate) right: Vec<f32>,
    pub(crate) metrics: StereoPcm16WavMetrics,
}

#[derive(Debug)]
pub(crate) enum RenderError {
    DecodeReference {
        path: &'static str,
        source: SampleDecodeError,
    },
    TubeTapAnalysisRequiresTube {
        case_id: &'static str,
    },
    TubeTapAnalysisRequiresSinglePitch {
        case_id: &'static str,
    },
    TubeTapAnalysisEmptyWindow {
        case_id: &'static str,
    },
    #[allow(dead_code)]
    BowDiagnosticRequiresString {
        case_id: &'static str,
    },
    #[allow(dead_code)]
    BowDiagnosticRequiresSinglePitch {
        case_id: &'static str,
    },
    #[allow(dead_code)]
    BowDiagnosticEmptyWindow {
        case_id: &'static str,
    },
    Wav(StereoPcm16WavError),
}

pub(crate) fn render_case(case: &CatalogCase) -> Result<RenderedCase, RenderError> {
    let target_frames = case.target_frames();
    let target = target_for_recipe(case.patch_recipe);
    let (mut left, mut right) = render_target(target, case.schedule.notes, target_frames)?;
    let post_gain = recipe_post_gain(case.patch_recipe);
    if post_gain != 1.0 {
        // Reference-match RMS gains are calibrated on the nominal case; extreme variants
        // (e.g. full humanize) can peak hotter, so cap this case's gain at full scale instead
        // of detuning the whole gain class to its loudest variant.
        let peak = left
            .iter()
            .chain(right.iter())
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let safe_gain = if peak * post_gain > 0.995 {
            0.995 / peak
        } else {
            post_gain
        };
        for sample in left.iter_mut().chain(right.iter_mut()) {
            *sample *= safe_gain;
        }
    }
    let metrics =
        validate_wav_stereo_pcm16(&left, &right, CATALOG_SAMPLE_RATE).map_err(RenderError::Wav)?;
    Ok(RenderedCase {
        left,
        right,
        metrics,
    })
}

/// The reference-match RMS gains can exceed the Tube patch's `output_gain_db` ceiling; the
/// overflow is applied here as a render-time scale so the "RMS-matched" cases actually match
/// their reference fixture instead of silently clamping at the patch maximum.
fn recipe_post_gain(recipe: PatchRecipe) -> f32 {
    match recipe {
        PatchRecipe::TubeReferenceMatchPhrase { gain, .. } => {
            let desired_db =
                TubePatch::default().output_gain_db + tube_reference_match_gain_db(gain);
            let residual_db = (desired_db - lamath_tube::OUTPUT_GAIN_MAX_DB).max(0.0);
            10.0_f32.powf(residual_db / 20.0)
        }
        _ => 1.0,
    }
}

#[derive(Debug, Clone)]
enum RenderTarget {
    ReferenceWav(&'static str),
    Modal(ResonatorSynthPatch),
    Cymbal(CymbalPatch),
    Tube(TubePatch),
    Stringed(StringPatch),
    StringedBowAlternatingScale,
}

fn render_target(
    target: RenderTarget,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> Result<(Vec<f32>, Vec<f32>), RenderError> {
    match target {
        RenderTarget::ReferenceWav(path) => render_reference_wav(path, target_frames),
        RenderTarget::Modal(patch) => Ok(render_modal(patch, notes, target_frames)),
        RenderTarget::Cymbal(patch) => Ok(render_cymbal(patch, notes, target_frames)),
        RenderTarget::Tube(patch) => Ok(render_tube(patch, notes, target_frames)),
        RenderTarget::Stringed(patch) => Ok(render_stringed(patch, notes, target_frames)),
        RenderTarget::StringedBowAlternatingScale => {
            Ok(render_stringed_bow_alternating(notes, target_frames))
        }
    }
}

fn render_reference_wav(
    path: &'static str,
    target_frames: usize,
) -> Result<(Vec<f32>, Vec<f32>), RenderError> {
    let decoded = decode_wav_mono(&repo_path(path))
        .map_err(|source| RenderError::DecodeReference { path, source })?;
    let read_ratio = f64::from(decoded.sample_rate) / f64::from(CATALOG_SAMPLE_RATE);
    let mut mono = vec![0.0; target_frames];
    WindowedSincResampler::default().render_to(&decoded.samples, read_ratio, &mut mono);
    Ok((mono.clone(), mono))
}

fn repo_path(relative_path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative_path)
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
        std::array::from_fn(CymbalExcitationSource::builtin),
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

fn render_tube_with_taps(
    patch: TubePatch,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>, [Vec<f32>; TUBE_RENDER_TAP_COUNT]) {
    let mut processor = TubeProcessor::new(
        CATALOG_SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(TubeExcitationSource::builtin),
    );
    let total_blocks = target_frames.div_ceil(CATALOG_BLOCK_SIZE);
    let mut block_left = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut block_right = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut left = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut right = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut tap_traces: [Vec<f32>; TUBE_RENDER_TAP_COUNT] =
        std::array::from_fn(|_| Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE));
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
        processor.process_with_taps(&events, &mut block_left, &mut block_right, &mut tap_traces);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    left.truncate(target_frames);
    right.truncate(target_frames);
    for trace in &mut tap_traces {
        trace.truncate(target_frames);
    }
    (left, right, tap_traces)
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

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct StringProbeRender {
    left: Vec<f32>,
    probes: Vec<StringModelProbe>,
}

#[allow(dead_code)]
fn render_stringed_with_probe(
    patch: StringPatch,
    notes: &[ScheduledNote],
    target_frames: usize,
) -> StringProbeRender {
    let mut processor = StringProcessor::new(
        CATALOG_SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(StringExcitationSource::builtin),
    );
    let total_blocks = target_frames.div_ceil(CATALOG_BLOCK_SIZE);
    let mut block_left = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut block_right = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut block_probes = vec![StringModelProbe::default(); CATALOG_BLOCK_SIZE];
    let mut left = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut probes = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
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
        processor.process_with_probe(
            &events,
            &mut block_left,
            &mut block_right,
            &mut block_probes,
        );
        left.extend_from_slice(&block_left);
        probes.extend_from_slice(&block_probes);
    }

    left.truncate(target_frames);
    probes.truncate(target_frames);
    StringProbeRender { left, probes }
}

fn render_stringed_bow_alternating(
    notes: &[ScheduledNote],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let smooth = string_driver_patch(DriverRecipe::BowSmooth);
    let scratch = string_driver_patch(DriverRecipe::BowScratch);
    let mut processor = StringProcessor::new(
        CATALOG_SAMPLE_RATE as f32,
        smooth.clone(),
        std::array::from_fn(StringExcitationSource::builtin),
    );
    render_stringed_patch_sequence(
        notes,
        target_frames,
        |index| {
            if index % 2 == 0 {
                smooth.clone()
            } else {
                scratch.clone()
            }
        },
        &mut processor,
    )
}

fn render_stringed_patch_sequence(
    notes: &[ScheduledNote],
    target_frames: usize,
    mut patch_for_note: impl FnMut(usize) -> StringPatch,
    processor: &mut StringProcessor<'_>,
) -> (Vec<f32>, Vec<f32>) {
    let total_blocks = target_frames.div_ceil(CATALOG_BLOCK_SIZE);
    let mut block_left = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut block_right = vec![0.0; CATALOG_BLOCK_SIZE];
    let mut left = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut right = Vec::with_capacity(total_blocks * CATALOG_BLOCK_SIZE);
    let mut actions = scheduled_string_patch_actions(notes, total_blocks);
    let mut events = Vec::with_capacity(notes.len() * 2);
    let mut cursor = 0;

    actions.sort_by_key(|action| (action.block, action.order));
    for block in 0..total_blocks {
        events.clear();
        while cursor < actions.len() && actions[cursor].block == block {
            let action = actions[cursor];
            if action.note_on {
                processor.set_patch(patch_for_note(action.note_index));
                events.push(MidiEvent::Note(NoteEvent::On {
                    channel: 0,
                    note: action.note,
                    velocity: action.velocity,
                }));
            } else {
                events.push(MidiEvent::Note(NoteEvent::Off {
                    channel: 0,
                    note: action.note,
                    velocity: 0.0,
                }));
            }
            cursor += 1;
        }
        processor.process(&events, &mut block_left, &mut block_right);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    left.truncate(target_frames);
    right.truncate(target_frames);
    (left, right)
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

#[derive(Debug, Clone, Copy)]
struct ScheduledStringPatchAction {
    block: usize,
    order: usize,
    note_index: usize,
    note_on: bool,
    note: u8,
    velocity: f32,
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

fn scheduled_string_patch_actions(
    notes: &[ScheduledNote],
    total_blocks: usize,
) -> Vec<ScheduledStringPatchAction> {
    let mut actions = Vec::with_capacity(notes.len() * 2);
    for (index, note) in notes.iter().enumerate() {
        actions.push(ScheduledStringPatchAction {
            block: block_at(note.start_seconds, total_blocks),
            order: index * 2,
            note_index: index,
            note_on: true,
            note: note.note,
            velocity: note.velocity,
        });
        actions.push(ScheduledStringPatchAction {
            block: block_at(note.end_seconds, total_blocks),
            order: index * 2 + 1,
            note_index: index,
            note_on: false,
            note: note.note,
            velocity: 0.0,
        });
    }
    actions
}

fn block_at(seconds: f32, total_blocks: usize) -> usize {
    ((seconds * CATALOG_SAMPLE_RATE as f32) as usize / CATALOG_BLOCK_SIZE)
        .min(total_blocks.saturating_sub(1))
}

fn single_pitch_frequency_hz(case: &CatalogCase) -> Result<f32, RenderError> {
    let Some(first) = case.schedule.notes.first() else {
        return Err(RenderError::TubeTapAnalysisRequiresSinglePitch { case_id: case.id });
    };
    if case
        .schedule
        .notes
        .iter()
        .any(|note| note.note != first.note)
    {
        return Err(RenderError::TubeTapAnalysisRequiresSinglePitch { case_id: case.id });
    }
    Ok(midi_note_to_hz(first.note as f32))
}

fn format_bow_spectrum_row(row: &BowDiagnosticRow) -> String {
    let mut output = format!("  {:<18}", row.name);
    for value in row.spectrum.band_power_relative_db {
        output.push_str(&format!(" {:>8}", format_db(value).trim()));
    }
    output.push_str(&format!(
        " {:>7} {:>8.0} {:>6.0}\n",
        format_db(row.spectrum.high_low_power_db).trim(),
        row.spectrum.centroid_hz,
        row.spectrum.rolloff95_hz,
    ));
    output
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeReference { path, source } => {
                write!(
                    formatter,
                    "reference WAV decode failed for {path}: {source}"
                )
            }
            Self::TubeTapAnalysisRequiresTube { case_id } => {
                write!(
                    formatter,
                    "tube tap analysis requires a Tube case: {case_id}"
                )
            }
            Self::TubeTapAnalysisRequiresSinglePitch { case_id } => write!(
                formatter,
                "tube tap analysis requires a non-empty single-pitch case: {case_id}"
            ),
            Self::TubeTapAnalysisEmptyWindow { case_id } => {
                write!(formatter, "tube tap analysis window is empty: {case_id}")
            }
            Self::BowDiagnosticRequiresString { case_id } => {
                write!(
                    formatter,
                    "bow diagnostic requires a String case: {case_id}"
                )
            }
            Self::BowDiagnosticRequiresSinglePitch { case_id } => write!(
                formatter,
                "bow diagnostic requires a non-empty single-pitch case: {case_id}"
            ),
            Self::BowDiagnosticEmptyWindow { case_id } => {
                write!(formatter, "bow diagnostic window is empty: {case_id}")
            }
            Self::Wav(error) => write!(formatter, "rendered WAV validation failed: {error:?}"),
        }
    }
}
