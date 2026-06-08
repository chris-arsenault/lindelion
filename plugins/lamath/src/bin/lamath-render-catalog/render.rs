use crate::catalog::{
    CATALOG_BLOCK_SIZE, CATALOG_SAMPLE_RATE, CatalogCase, ContactRecipe, DriverRecipe, EdgeRecipe,
    MeshStriker, MeshVoicing, PatchRecipe, ResonatorFamily, ScheduledNote, SourceBodyDepth,
    SurroundingRecipe, TubeBellLevel, TubeBodyFormantLevel, TubeRadiationShape, TubeReedAperture,
    TubeReferenceArticulation, TubeReferenceHumanize, TubeReferenceMatchGain,
    TubeReferenceRegisterKey,
};
use lamath::{ModalConfig, ModalPreset, ResonatorRouting, ResonatorSynth, ResonatorSynthPatch};
use lamath_cymbal::{CymbalExcitationSource, CymbalPatch, CymbalProcessor};
use lamath_stringed::{
    BodySelection, DriverSelection, ModelSwitches, StringExcitationSource, StringModelProbe,
    StringPatch, StringProcessor,
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

const TAP_RATIO_HARMONIC_COUNT: usize = 8;
const TAP_FLOOR_BAND_COUNT: usize = 5;
const TAP_RATIO_HARMONICS: [usize; TAP_RATIO_HARMONIC_COUNT] = [2, 3, 4, 5, 7, 9, 11, 13];
const TAP_FLOOR_BANDS: [(&str, f32, f32); TAP_FLOOR_BAND_COUNT] = [
    ("floor_0.2_0.8k", 200.0, 800.0),
    ("floor_0.8_1.2k", 800.0, 1_200.0),
    ("floor_1.2_2k", 1_200.0, 2_000.0),
    ("floor_2_6k", 2_000.0, 6_000.0),
    ("floor_6_12k", 6_000.0, 12_000.0),
];
const TAP_ANALYSIS_MAX_SECONDS: f32 = 1.5;
#[allow(dead_code)]
const BOW_DIAGNOSTIC_WINDOW_SECONDS: f32 = 0.68;
const BOW_SPECTRUM_BAND_COUNT: usize = 7;
const BOW_SPECTRUM_STEP_HZ: f32 = 80.0;
const BOW_SPECTRUM_BANDS: [(&str, f32, f32); BOW_SPECTRUM_BAND_COUNT] = [
    ("80-500", 80.0, 500.0),
    ("0.5-1k", 500.0, 1_000.0),
    ("1-2k", 1_000.0, 2_000.0),
    ("2-4k", 2_000.0, 4_000.0),
    ("4-8k", 4_000.0, 8_000.0),
    ("8-12k", 8_000.0, 12_000.0),
    ("12-16k", 12_000.0, 16_000.0),
];

#[derive(Debug, Clone)]
pub(crate) struct TubeTapAnalysisReport {
    pub(crate) case_id: &'static str,
    pub(crate) frequency_hz: f32,
    pub(crate) start_seconds: f32,
    pub(crate) end_seconds: f32,
    pub(crate) rows: Vec<TubeTapAnalysisRow>,
}

#[derive(Debug, Clone)]
pub(crate) struct TubeTapAnalysisRow {
    pub(crate) name: &'static str,
    pub(crate) rms_dbfs: f32,
    pub(crate) h1_dbfs: f32,
    pub(crate) harmonic_ratios_db: [f32; TAP_RATIO_HARMONIC_COUNT],
    pub(crate) h15_h25_db: f32,
    pub(crate) h27_h39_db: f32,
    pub(crate) floors_dbfs: [f32; TAP_FLOOR_BAND_COUNT],
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct BowDiagnosticReport {
    pub(crate) case_id: &'static str,
    pub(crate) frequency_hz: f32,
    pub(crate) smoothed_frequency_hz: f32,
    pub(crate) one_way_delay_samples: f32,
    pub(crate) start_seconds: f32,
    pub(crate) end_seconds: f32,
    pub(crate) output: BowDiagnosticRow,
    pub(crate) pickup_tap: BowDiagnosticRow,
    pub(crate) body_radiation: BowDiagnosticRow,
    pub(crate) weighted_pickup: BowDiagnosticRow,
    pub(crate) weighted_body: BowDiagnosticRow,
    pub(crate) pickup_weight: f32,
    pub(crate) body_weight: f32,
    pub(crate) mix_residual_rms: f32,
    pub(crate) model_output: BowDiagnosticRow,
    pub(crate) bow_force: BowDiagnosticRow,
    pub(crate) bow_wave_injection: BowDiagnosticRow,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct BowDiagnosticRow {
    pub(crate) name: &'static str,
    pub(crate) peak: f32,
    pub(crate) rms: f32,
    pub(crate) dominant_hz: f32,
    pub(crate) dominant_dbfs: f32,
    pub(crate) nearest_harmonic: usize,
    pub(crate) nearest_harmonic_cents: f32,
    pub(crate) h1_dbfs: f32,
    pub(crate) h1_relative_db: f32,
    pub(crate) h2_relative_db: f32,
    pub(crate) h3_relative_db: f32,
    pub(crate) h4_relative_db: f32,
    pub(crate) h8_relative_db: f32,
    pub(crate) h16_relative_db: f32,
    pub(crate) h24_relative_db: f32,
    pub(crate) high_peak_hz: f32,
    pub(crate) high_peak_dbfs: f32,
    pub(crate) high_peak_relative_db: f32,
    pub(crate) spectrum: BowSpectrumSummary,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BowSpectrumSummary {
    pub(crate) band_power_relative_db: [f32; BOW_SPECTRUM_BAND_COUNT],
    pub(crate) high_low_power_db: f32,
    pub(crate) centroid_hz: f32,
    pub(crate) rolloff95_hz: f32,
}

pub(crate) fn render_case(case: &CatalogCase) -> Result<RenderedCase, RenderError> {
    let target_frames = case.target_frames();
    let target = target_for_recipe(case.patch_recipe);
    let (left, right) = render_target(target, case.schedule.notes, target_frames)?;
    let metrics =
        validate_wav_stereo_pcm16(&left, &right, CATALOG_SAMPLE_RATE).map_err(RenderError::Wav)?;
    Ok(RenderedCase {
        left,
        right,
        metrics,
    })
}

pub(crate) fn analyze_tube_taps(case: &CatalogCase) -> Result<TubeTapAnalysisReport, RenderError> {
    let RenderTarget::Tube(patch) = target_for_recipe(case.patch_recipe) else {
        return Err(RenderError::TubeTapAnalysisRequiresTube { case_id: case.id });
    };
    let frequency_hz = single_pitch_frequency_hz(case)?;
    let target_frames = case.target_frames();
    let (_, _, tap_traces) = render_tube_with_taps(patch, case.schedule.notes, target_frames);
    let (start, end) = tap_analysis_window(case, target_frames)?;
    let rows = TUBE_RENDER_TAP_NAMES
        .iter()
        .copied()
        .zip(tap_traces.iter())
        .map(|(name, trace)| analyze_tap_trace(name, &trace[start..end], frequency_hz))
        .collect();
    Ok(TubeTapAnalysisReport {
        case_id: case.id,
        frequency_hz,
        start_seconds: start as f32 / CATALOG_SAMPLE_RATE as f32,
        end_seconds: end as f32 / CATALOG_SAMPLE_RATE as f32,
        rows,
    })
}

#[allow(dead_code)]
pub(crate) fn analyze_bow_diagnostic(
    case: &CatalogCase,
) -> Result<BowDiagnosticReport, RenderError> {
    let RenderTarget::Stringed(patch) = target_for_recipe(case.patch_recipe) else {
        return Err(RenderError::BowDiagnosticRequiresString { case_id: case.id });
    };
    let frequency_hz = single_pitch_frequency_hz(case)
        .map_err(|_| RenderError::BowDiagnosticRequiresSinglePitch { case_id: case.id })?;
    let target_frames = case.target_frames();
    let render = render_stringed_with_probe(patch, case.schedule.notes, target_frames);
    let (start, end) = bow_diagnostic_window(case, target_frames)?;
    let output = &render.left[start..end];
    let probes = &render.probes[start..end];
    let pickup_tap = probe_signal(probes, |probe| probe.pickup_tap);
    let body_radiation = probe_signal(probes, |probe| probe.body_radiated);
    let weighted_pickup = probe_signal(probes, |probe| probe.weighted_pickup);
    let weighted_body = probe_signal(probes, |probe| probe.weighted_body);
    let model_output = probe_signal(probes, |probe| probe.output);
    let mix_residual = mix_residual(&model_output, &weighted_pickup, &weighted_body);
    let bow_force = probe_signal(probes, |probe| probe.bow_force);
    let bow_wave_injection = probe_signal(probes, |probe| probe.bow_wave_correction);
    Ok(BowDiagnosticReport {
        case_id: case.id,
        frequency_hz,
        smoothed_frequency_hz: mean_probe(probes, |probe| probe.current_frequency_hz),
        one_way_delay_samples: mean_probe(probes, |probe| probe.one_way_delay_samples),
        start_seconds: start as f32 / CATALOG_SAMPLE_RATE as f32,
        end_seconds: end as f32 / CATALOG_SAMPLE_RATE as f32,
        output: analyze_bow_signal("final output", output, frequency_hz),
        pickup_tap: analyze_bow_signal("pickup tap", &pickup_tap, frequency_hz),
        body_radiation: analyze_bow_signal("body radiation", &body_radiation, frequency_hz),
        weighted_pickup: analyze_bow_signal("weighted pickup", &weighted_pickup, frequency_hz),
        weighted_body: analyze_bow_signal("weighted body", &weighted_body, frequency_hz),
        pickup_weight: mean_probe(probes, |probe| probe.pickup_weight),
        body_weight: mean_probe(probes, |probe| probe.body_weight),
        mix_residual_rms: rms(&mix_residual),
        model_output: analyze_bow_signal("model output", &model_output, frequency_hz),
        bow_force: analyze_bow_signal("bow force", &bow_force, frequency_hz),
        bow_wave_injection: analyze_bow_signal(
            "bow wave injection",
            &bow_wave_injection,
            frequency_hz,
        ),
    })
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

fn tap_analysis_window(
    case: &CatalogCase,
    target_frames: usize,
) -> Result<(usize, usize), RenderError> {
    let Some(first) = case.schedule.notes.first() else {
        return Err(RenderError::TubeTapAnalysisEmptyWindow { case_id: case.id });
    };
    let phrase_start_seconds = case
        .schedule
        .notes
        .iter()
        .map(|note| note.start_seconds)
        .fold(first.start_seconds, f32::min);
    let phrase_end_seconds = case
        .schedule
        .notes
        .iter()
        .map(|note| note.end_seconds)
        .fold(first.end_seconds, f32::max);
    let duration = (phrase_end_seconds - phrase_start_seconds).max(0.0);
    if duration <= f32::EPSILON || target_frames == 0 {
        return Err(RenderError::TubeTapAnalysisEmptyWindow { case_id: case.id });
    }

    let trim_seconds = (duration * 0.18).clamp(0.20, 0.90);
    let trimmed_start = phrase_start_seconds + trim_seconds.min(duration * 0.35);
    let trimmed_end = phrase_end_seconds - trim_seconds.min(duration * 0.35);
    let (start_seconds, end_seconds) = if trimmed_end > trimmed_start {
        let max_duration = TAP_ANALYSIS_MAX_SECONDS.min(trimmed_end - trimmed_start);
        let center = (trimmed_start + trimmed_end) * 0.5;
        (center - max_duration * 0.5, center + max_duration * 0.5)
    } else {
        (phrase_start_seconds, phrase_end_seconds)
    };

    let start = seconds_to_frame(start_seconds).min(target_frames);
    let end = seconds_to_frame(end_seconds).min(target_frames);
    if end.saturating_sub(start) < 1024 {
        return Err(RenderError::TubeTapAnalysisEmptyWindow { case_id: case.id });
    }
    Ok((start, end))
}

fn seconds_to_frame(seconds: f32) -> usize {
    (seconds.max(0.0) * CATALOG_SAMPLE_RATE as f32).round() as usize
}

fn analyze_tap_trace(name: &'static str, samples: &[f32], frequency_hz: f32) -> TubeTapAnalysisRow {
    let h1 = windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, frequency_hz);
    let harmonic_ratios_db = std::array::from_fn(|index| {
        let harmonic = TAP_RATIO_HARMONICS[index] as f32;
        let magnitude =
            windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, frequency_hz * harmonic);
        ratio_db(magnitude, h1)
    });
    let floors_dbfs = std::array::from_fn(|index| {
        let (_, start_hz, end_hz) = TAP_FLOOR_BANDS[index];
        inter_harmonic_floor_dbfs(samples, start_hz, end_hz, frequency_hz)
    });
    TubeTapAnalysisRow {
        name,
        rms_dbfs: amplitude_db(rms(samples)),
        h1_dbfs: amplitude_db(h1),
        harmonic_ratios_db,
        h15_h25_db: harmonic_average_ratio_db(samples, frequency_hz, 15, 25, h1),
        h27_h39_db: harmonic_average_ratio_db(samples, frequency_hz, 27, 39, h1),
        floors_dbfs,
    }
}

fn harmonic_average_ratio_db(
    samples: &[f32],
    frequency_hz: f32,
    first: usize,
    last: usize,
    h1: f32,
) -> f32 {
    let nyquist = CATALOG_SAMPLE_RATE as f32 * 0.5;
    let mut power = 0.0;
    let mut count = 0;
    for harmonic in first..=last {
        let probe_hz = frequency_hz * harmonic as f32;
        if probe_hz >= nyquist {
            continue;
        }
        let magnitude = windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, probe_hz);
        power += magnitude * magnitude;
        count += 1;
    }
    if count == 0 {
        return f32::NEG_INFINITY;
    }
    ratio_db((power / count as f32).sqrt(), h1)
}

fn inter_harmonic_floor_dbfs(
    samples: &[f32],
    start_hz: f32,
    end_hz: f32,
    frequency_hz: f32,
) -> f32 {
    let nyquist = CATALOG_SAMPLE_RATE as f32 * 0.5;
    let end_hz = end_hz.min(nyquist * 0.98);
    if end_hz <= start_hz {
        return f32::NEG_INFINITY;
    }
    let step_hz = (frequency_hz * 0.73).clamp(80.0, 180.0);
    let reject_radius = frequency_hz * 0.18;
    let mut probe_hz = start_hz + step_hz * 0.5;
    let mut power = 0.0;
    let mut count = 0;
    while probe_hz < end_hz {
        let nearest_harmonic = (probe_hz / frequency_hz).round().max(1.0) * frequency_hz;
        if (probe_hz - nearest_harmonic).abs() > reject_radius {
            let magnitude =
                windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, probe_hz);
            power += magnitude * magnitude;
            count += 1;
        }
        probe_hz += step_hz;
    }
    if count == 0 {
        return f32::NEG_INFINITY;
    }
    amplitude_db((power / count as f32).sqrt())
}

fn ratio_db(magnitude: f32, reference: f32) -> f32 {
    amplitude_db(magnitude / reference.max(1.0e-12))
}

fn power_ratio_db(power: f32, reference: f32) -> f32 {
    if power.is_finite() && power > 0.0 && reference.is_finite() && reference > 0.0 {
        10.0 * (power / reference.max(1.0e-24)).log10()
    } else {
        f32::NEG_INFINITY
    }
}

fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude.is_finite() && amplitude > 0.0 {
        20.0 * amplitude.max(1.0e-12).log10()
    } else {
        f32::NEG_INFINITY
    }
}

fn format_db(value: f32) -> String {
    if value.is_finite() {
        format!("{value:>7.1}")
    } else {
        "   -inf".to_string()
    }
}

fn bow_diagnostic_window(
    case: &CatalogCase,
    target_frames: usize,
) -> Result<(usize, usize), RenderError> {
    let Some(first) = case.schedule.notes.first() else {
        return Err(RenderError::BowDiagnosticEmptyWindow { case_id: case.id });
    };
    let phrase_start_seconds = case
        .schedule
        .notes
        .iter()
        .map(|note| note.start_seconds)
        .fold(first.start_seconds, f32::min);
    let phrase_end_seconds = case
        .schedule
        .notes
        .iter()
        .map(|note| note.end_seconds)
        .fold(first.end_seconds, f32::max);
    let release_margin_seconds = 0.10;
    let window_end_seconds =
        (phrase_end_seconds - release_margin_seconds).max(phrase_start_seconds);
    let window_start_seconds =
        (window_end_seconds - BOW_DIAGNOSTIC_WINDOW_SECONDS).max(phrase_start_seconds + 0.25);
    let start = seconds_to_frame(window_start_seconds).min(target_frames);
    let end = seconds_to_frame(window_end_seconds).min(target_frames);
    if end.saturating_sub(start) < 2_048 {
        return Err(RenderError::BowDiagnosticEmptyWindow { case_id: case.id });
    }
    Ok((start, end))
}

fn analyze_bow_signal(name: &'static str, samples: &[f32], frequency_hz: f32) -> BowDiagnosticRow {
    let (dominant_hz, dominant_magnitude) =
        dominant_peak(samples, CATALOG_SAMPLE_RATE as f32, 80.0, 5_000.0);
    let (high_peak_hz, high_peak_magnitude) =
        dominant_peak(samples, CATALOG_SAMPLE_RATE as f32, 4_000.0, 12_000.0);
    let (nearest_harmonic, nearest_harmonic_cents) = nearest_harmonic(dominant_hz, frequency_hz);
    let h1 = windowed_dft_magnitude_at(samples, CATALOG_SAMPLE_RATE as f32, frequency_hz);
    let spectrum = bow_spectrum_summary(samples);
    BowDiagnosticRow {
        name,
        peak: peak_abs(samples),
        rms: rms(samples),
        dominant_hz,
        dominant_dbfs: amplitude_db(dominant_magnitude),
        nearest_harmonic,
        nearest_harmonic_cents,
        h1_dbfs: amplitude_db(h1),
        h1_relative_db: ratio_db(h1, dominant_magnitude),
        h2_relative_db: harmonic_relative_db(samples, frequency_hz, 2, dominant_magnitude),
        h3_relative_db: harmonic_relative_db(samples, frequency_hz, 3, dominant_magnitude),
        h4_relative_db: harmonic_relative_db(samples, frequency_hz, 4, dominant_magnitude),
        h8_relative_db: harmonic_relative_db(samples, frequency_hz, 8, dominant_magnitude),
        h16_relative_db: harmonic_relative_db(samples, frequency_hz, 16, dominant_magnitude),
        h24_relative_db: harmonic_relative_db(samples, frequency_hz, 24, dominant_magnitude),
        high_peak_hz,
        high_peak_dbfs: amplitude_db(high_peak_magnitude),
        high_peak_relative_db: ratio_db(high_peak_magnitude, dominant_magnitude),
        spectrum,
    }
}

fn harmonic_relative_db(
    samples: &[f32],
    frequency_hz: f32,
    harmonic: usize,
    reference: f32,
) -> f32 {
    ratio_db(
        windowed_dft_magnitude_at(
            samples,
            CATALOG_SAMPLE_RATE as f32,
            frequency_hz * harmonic as f32,
        ),
        reference,
    )
}

fn bow_spectrum_summary(samples: &[f32]) -> BowSpectrumSummary {
    let sample_rate = CATALOG_SAMPLE_RATE as f32;
    let high_limit = 16_000.0_f32.min(sample_rate * 0.49);
    let mut bands = [0.0; BOW_SPECTRUM_BAND_COUNT];
    let mut sampled_power = Vec::new();
    let mut total_power = 0.0;
    let mut weighted_power = 0.0;
    let mut frequency = 80.0;

    while frequency <= high_limit {
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, frequency);
        let power = magnitude * magnitude;
        total_power += power;
        weighted_power += frequency * power;
        sampled_power.push((frequency, power));
        for (index, (_, low_hz, high_hz)) in BOW_SPECTRUM_BANDS.iter().copied().enumerate() {
            if frequency >= low_hz && frequency < high_hz {
                bands[index] += power;
                break;
            }
        }
        frequency += BOW_SPECTRUM_STEP_HZ;
    }

    let mut cumulative_power = 0.0;
    let rolloff_target = total_power * 0.95;
    let mut rolloff95_hz = 0.0;
    for (frequency, power) in sampled_power {
        cumulative_power += power;
        if cumulative_power >= rolloff_target {
            rolloff95_hz = frequency;
            break;
        }
    }

    let low_mid_power = bands[..4].iter().sum::<f32>();
    let high_power = bands[4..].iter().sum::<f32>();
    BowSpectrumSummary {
        band_power_relative_db: std::array::from_fn(|index| {
            power_ratio_db(bands[index], total_power)
        }),
        high_low_power_db: power_ratio_db(high_power, low_mid_power),
        centroid_hz: if total_power > 1.0e-24 {
            weighted_power / total_power
        } else {
            0.0
        },
        rolloff95_hz,
    }
}

fn dominant_peak(samples: &[f32], sample_rate: f32, low_hz: f32, high_hz: f32) -> (f32, f32) {
    let mut best_hz = low_hz;
    let mut best_magnitude = 0.0;
    let mut probe_hz = low_hz;
    while probe_hz <= high_hz {
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, probe_hz);
        if magnitude > best_magnitude {
            best_hz = probe_hz;
            best_magnitude = magnitude;
        }
        probe_hz += 10.0;
    }

    let refine_start = (best_hz - 12.0).max(low_hz);
    let refine_end = (best_hz + 12.0).min(high_hz);
    probe_hz = refine_start;
    while probe_hz <= refine_end {
        let magnitude = windowed_dft_magnitude_at(samples, sample_rate, probe_hz);
        if magnitude > best_magnitude {
            best_hz = probe_hz;
            best_magnitude = magnitude;
        }
        probe_hz += 1.0;
    }
    (best_hz, best_magnitude)
}

fn nearest_harmonic(frequency_hz: f32, fundamental_hz: f32) -> (usize, f32) {
    if frequency_hz <= 0.0 || fundamental_hz <= 0.0 {
        return (0, 0.0);
    }
    let harmonic = (frequency_hz / fundamental_hz).round().max(1.0) as usize;
    let harmonic_hz = fundamental_hz * harmonic as f32;
    let cents = 1_200.0 * (frequency_hz / harmonic_hz).log2();
    (harmonic, cents)
}

fn probe_signal(
    probes: &[StringModelProbe],
    mut value: impl FnMut(&StringModelProbe) -> f32,
) -> Vec<f32> {
    probes.iter().map(|probe| value(probe)).collect()
}

fn mean_probe(probes: &[StringModelProbe], mut value: impl FnMut(&StringModelProbe) -> f32) -> f32 {
    if probes.is_empty() {
        return 0.0;
    }
    probes.iter().map(|probe| value(probe)).sum::<f32>() / probes.len() as f32
}

fn mix_residual(output: &[f32], pickup: &[f32], body: &[f32]) -> Vec<f32> {
    output
        .iter()
        .copied()
        .zip(pickup.iter().copied())
        .zip(body.iter().copied())
        .map(|((output, pickup), body)| output - pickup - body)
        .collect()
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
        PatchRecipe::StringBowAlternatingScale => RenderTarget::StringedBowAlternatingScale,
        PatchRecipe::Surrounding {
            family,
            surrounding,
        } => surrounding_target(family, surrounding),
        PatchRecipe::ReferenceWav { path } => RenderTarget::ReferenceWav(path),
        PatchRecipe::Edge(recipe) => edge_target(recipe),
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
        PatchRecipe::TubeReferenceMatchPhrase {
            articulation,
            gain,
            humanize,
            register_key,
            body_enabled,
            reed_radiation_enabled,
        } => {
            let patch = TubePatch {
                pressure: tube_reference_match_pressure(gain),
                reed_aperture_inertia: reed_aperture_inertia(TubeReedAperture::Inertial),
                body_formant: tube_body_formant_level(TubeBodyFormantLevel::Strong),
                humanize: tube_reference_humanize_value(humanize),
                register_break_note: tube_reference_register_break_note(register_key),
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
            brightness: 0.50,
            stiffness: 0.12,
            damping: 0.34,
            bow_position: 0.16,
            bow_pressure: 0.58,
            bow_speed: 0.36,
            bow_friction: 0.40,
            pickup_position: 0.50,
            body_balance: 0.24,
            output_gain_db: 8.0,
            switches: ModelSwitches {
                tension: false,
                ..ModelSwitches::default()
            },
            ..StringPatch::default()
        },
        DriverRecipe::BowScratch => StringPatch {
            driver: DriverSelection::Bow,
            body: BodySelection::Violin,
            brightness: 0.58,
            stiffness: 0.42,
            damping: 0.40,
            bow_position: 0.22,
            bow_pressure: 0.88,
            bow_speed: 0.68,
            bow_friction: 0.56,
            output_gain_db: 8.0,
            switches: ModelSwitches {
                tension: false,
                ..ModelSwitches::default()
            },
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

fn reed_aperture_inertia(reed_aperture: TubeReedAperture) -> f32 {
    match reed_aperture {
        TubeReedAperture::Instant => 0.0,
        TubeReedAperture::Inertial => 1.0,
    }
}

fn tube_body_formant_level(level: TubeBodyFormantLevel) -> f32 {
    match level {
        TubeBodyFormantLevel::Current => 0.0,
        TubeBodyFormantLevel::Medium => 0.55,
        TubeBodyFormantLevel::Strong => 1.0,
    }
}

fn tube_bell_level(level: TubeBellLevel) -> (bool, f32) {
    match level {
        TubeBellLevel::Off => (false, 0.0),
        TubeBellLevel::Nominal => (true, TubePatch::default().bell),
        TubeBellLevel::Full => (true, 1.0),
    }
}

fn tube_radiation_shape_value(shape: TubeRadiationShape) -> f32 {
    match shape {
        TubeRadiationShape::Current => 0.0,
        TubeRadiationShape::Gentle => 1.0,
    }
}

fn tube_reference_articulation_slot(articulation: TubeReferenceArticulation) -> usize {
    match articulation {
        TubeReferenceArticulation::Legato => 2,
        TubeReferenceArticulation::Tongue => 0,
    }
}

fn tube_reference_match_gain_db(gain: TubeReferenceMatchGain) -> f32 {
    match gain {
        TubeReferenceMatchGain::LowESustainPhysical => 24.0,
        TubeReferenceMatchGain::LowESustainBodyOff => 3.8,
        TubeReferenceMatchGain::RegisterKeyHighSustain => 11.6,
        TubeReferenceMatchGain::RegisterKeyHighSustainVented => 22.9,
        TubeReferenceMatchGain::LowHighArticulation => 8.35,
    }
}

fn tube_reference_match_pressure(gain: TubeReferenceMatchGain) -> f32 {
    match gain {
        TubeReferenceMatchGain::LowESustainPhysical
        | TubeReferenceMatchGain::LowESustainBodyOff => TubePatch::default().pressure,
        _ => TubePatch::default().pressure,
    }
}

fn tube_reference_humanize_value(humanize: TubeReferenceHumanize) -> f32 {
    match humanize {
        TubeReferenceHumanize::Off => 0.0,
        TubeReferenceHumanize::Medium => 0.5,
        TubeReferenceHumanize::Full => 1.0,
    }
}

fn tube_reference_register_break_note(register_key: TubeReferenceRegisterKey) -> f32 {
    match register_key {
        TubeReferenceRegisterKey::Default => TubePatch::default().register_break_note,
        TubeReferenceRegisterKey::Disabled => 96.0,
    }
}

fn mesh_striker_slot(striker: MeshStriker) -> usize {
    match striker {
        MeshStriker::HardStick => 0,
        MeshStriker::SoftMallet => 1,
        MeshStriker::JazzBrush => 2,
        MeshStriker::BellStick => 3,
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

impl TubeTapAnalysisReport {
    pub(crate) fn summary(&self) -> String {
        let mut output = String::new();
        output.push_str("Lamath Tube tap analysis\n");
        output.push_str(&format!("case: {}\n", self.case_id));
        output.push_str(&format!("f0: {:.2} Hz\n", self.frequency_hz));
        output.push_str(&format!(
            "window: {:.3}-{:.3} s\n",
            self.start_seconds, self.end_seconds
        ));
        output.push_str(
            "tap                           rms     h1     h2     h3     h4     h5     h7     h9    h11    h13  h15-25 h27-39  fl0.2  fl0.8  fl1.2    fl2    fl6\n",
        );
        for row in &self.rows {
            output.push_str(&format!(
                "{:<28} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                row.name,
                format_db(row.rms_dbfs),
                format_db(row.h1_dbfs),
                format_db(row.harmonic_ratios_db[0]),
                format_db(row.harmonic_ratios_db[1]),
                format_db(row.harmonic_ratios_db[2]),
                format_db(row.harmonic_ratios_db[3]),
                format_db(row.harmonic_ratios_db[4]),
                format_db(row.harmonic_ratios_db[5]),
                format_db(row.harmonic_ratios_db[6]),
                format_db(row.harmonic_ratios_db[7]),
                format_db(row.h15_h25_db),
                format_db(row.h27_h39_db),
                format_db(row.floors_dbfs[0]),
                format_db(row.floors_dbfs[1]),
                format_db(row.floors_dbfs[2]),
                format_db(row.floors_dbfs[3]),
                format_db(row.floors_dbfs[4]),
            ));
        }
        output.push_str("harmonic columns after h1 are dB relative to that tap's h1; floor columns are inter-harmonic dBFS bands.\n");
        output
    }
}

#[allow(dead_code)]
impl BowDiagnosticReport {
    pub(crate) fn summary(&self) -> String {
        let mut output = String::new();
        output.push_str("Lamath bow physical diagnostic\n");
        output.push_str(&format!("case: {}\n", self.case_id));
        output.push_str(&format!(
            "window: {:.3}-{:.3} s\n",
            self.start_seconds, self.end_seconds
        ));
        output.push_str(&format!(
            "target: {:.2} Hz; smoothed string: {:.2} Hz; one-way delay: {:.2} samples\n\n",
            self.frequency_hz, self.smoothed_frequency_hz, self.one_way_delay_samples
        ));

        output.push_str("speaker sanity\n");
        output.push_str(&format!(
            "  peak {:.4} ({})  rms {:.4} ({})  dominant {}\n",
            self.output.peak,
            format_db(amplitude_db(self.output.peak)).trim(),
            self.output.rms,
            format_db(amplitude_db(self.output.rms)).trim(),
            self.output.peak_description()
        ));
        output.push_str(&format!("  flags: {}\n\n", self.safety_flags()));

        output.push_str("internal spectra\n");
        output.push_str(&format_bow_row(&self.pickup_tap));
        output.push_str(&format_bow_row(&self.body_radiation));
        output.push_str(&format_bow_row(&self.weighted_pickup));
        output.push_str(&format_bow_row(&self.weighted_body));
        output.push_str(&format_bow_row(&self.model_output));
        output.push_str(&format_bow_row(&self.bow_force));
        output.push_str(&format_bow_row(&self.bow_wave_injection));
        output.push_str(&format_bow_row(&self.output));
        output.push_str("\nfull-spectrum sampled power\n");
        output.push_str(&format_bow_spectrum_header());
        output.push_str(&format_bow_spectrum_row(&self.pickup_tap));
        output.push_str(&format_bow_spectrum_row(&self.weighted_body));
        output.push_str(&format_bow_spectrum_row(&self.model_output));
        output.push_str(&format_bow_spectrum_row(&self.bow_wave_injection));
        output.push_str(&format_bow_spectrum_row(&self.output));
        output.push_str(&format!(
            "\nmean pickup/body weights: {:.3}/{:.3}; raw body/pickup rms: {} dB; weighted body/pickup rms: {} dB; bow-wave/model-output rms: {} dB; mix residual rms: {}\n",
            self.pickup_weight,
            self.body_weight,
            format_db(ratio_db(self.body_radiation.rms, self.pickup_tap.rms)).trim(),
            format_db(ratio_db(self.weighted_body.rms, self.weighted_pickup.rms)).trim(),
            format_db(ratio_db(self.bow_wave_injection.rms, self.model_output.rms)).trim(),
            format_db(amplitude_db(self.mix_residual_rms)).trim(),
        ));
        output.push_str("physical read: ");
        output.push_str(self.physical_read());
        output.push('\n');
        output
    }

    fn safety_flags(&self) -> &'static str {
        if self.output.peak > 0.98 {
            "peak near full scale"
        } else if self.output.nearest_harmonic >= 4 && self.output.h1_relative_db < -18.0 {
            "dominant upper harmonic, weak fundamental"
        } else {
            "ok"
        }
    }

    fn physical_read(&self) -> &'static str {
        let contact_high =
            self.bow_force.nearest_harmonic >= 4 || self.bow_wave_injection.nearest_harmonic >= 4;
        let output_high =
            self.output.nearest_harmonic >= 4 || self.model_output.nearest_harmonic >= 4;
        if contact_high {
            "bow contact is already producing a high-harmonic stick-slip period; fix the contact junction before touching body/output balance."
        } else if output_high {
            "bow contact is lower-period than the rendered tone; bridge/body/output weighting is selecting upper partials over the string fundamental."
        } else {
            "dominant periods agree through contact and output; remaining mismatch is likely harmonic balance/decay rather than period selection."
        }
    }
}

#[allow(dead_code)]
impl BowDiagnosticRow {
    fn peak_description(&self) -> String {
        format!(
            "{:.1} Hz ~= H{} ({:+.0} cents), {} dBFS",
            self.dominant_hz,
            self.nearest_harmonic,
            self.nearest_harmonic_cents,
            format_db(self.dominant_dbfs).trim()
        )
    }
}

#[allow(dead_code)]
fn format_bow_row(row: &BowDiagnosticRow) -> String {
    format!(
        "  {:<18} rms {} dBFS  dom {:>7.1} Hz H{:<2} {:+5.0}c {} dBFS  H1 {} dBFS ({:>7}) H2 {:>7} H3 {:>7} H4 {:>7} H8 {:>7} H16 {:>7} H24 {:>7} >4k {:>7.1}Hz {:>7} dBFS ({:>7})\n",
        row.name,
        format_db(amplitude_db(row.rms)).trim(),
        row.dominant_hz,
        row.nearest_harmonic,
        row.nearest_harmonic_cents,
        format_db(row.dominant_dbfs).trim(),
        format_db(row.h1_dbfs).trim(),
        format_db(row.h1_relative_db).trim(),
        format_db(row.h2_relative_db).trim(),
        format_db(row.h3_relative_db).trim(),
        format_db(row.h4_relative_db).trim(),
        format_db(row.h8_relative_db).trim(),
        format_db(row.h16_relative_db).trim(),
        format_db(row.h24_relative_db).trim(),
        row.high_peak_hz,
        format_db(row.high_peak_dbfs).trim(),
        format_db(row.high_peak_relative_db).trim(),
    )
}

fn format_bow_spectrum_header() -> String {
    let mut output = String::from("  signal             ");
    for (label, _, _) in BOW_SPECTRUM_BANDS {
        output.push_str(&format!(" {label:>8}"));
    }
    output.push_str("   hi/lo centroid roll95\n");
    output
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
