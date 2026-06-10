//! Instrument voices for the song tracks and the offline block renderer.
//!
//! Each voice is one of the four Lamath resonator families with a hand-picked
//! patch (the bow, ride, and crash settings mirror the audition-approved render
//! catalog patches). Rendering follows the render-catalog pipeline: schedule
//! note on/off events into 512-sample blocks and run the family processor.

use crate::score::Note;
use lamath::{ModalConfig, ModalPreset, ResonatorRouting, ResonatorSynth, ResonatorSynthPatch};
use lamath_cymbal::{CymbalExcitationSource, CymbalPatch, CymbalProcessor};
use lamath_stringed::{
    BodySelection, DriverSelection, StringExcitationSource, StringPatch, StringProcessor,
};
use lamath_tube::{TubeExcitationSource, TubePatch, TubeProcessor};
use lindelion_plugin_shell::{
    AudioBuffer, AudioPlugin, MidiEvent, NoteEvent, ProcessContext, ProcessMode, ProcessSetup,
};

pub(crate) const SAMPLE_RATE: u32 = 48_000;
const BLOCK_SIZE: usize = 512;

/// The instrument behind one track.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Voice {
    /// Modal family, default struck-keys character (arpeggios).
    ModalKeys,
    /// Modal family, bell preset (B-section counter-melody and sparkles).
    ModalBells,
    /// String family, smooth bow on a violin body (sustained chord voices).
    StringBow,
    /// String family, picked (bass line).
    StringPick,
    /// Tube family, default clarinet voice (lead melody).
    TubeLead,
    /// Tube family, darker embouchure (harmony and pads).
    TubeDark,
    /// Mesh family, ride gong (timekeeping).
    CymbalRide,
    /// Mesh family, crash (swells and section accents).
    CymbalCrash,
}

pub(crate) fn render_voice(
    voice: Voice,
    notes: &[Note],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    match voice {
        Voice::ModalKeys => render_modal(modal_keys_patch(), notes, target_frames),
        Voice::ModalBells => render_modal(modal_bells_patch(), notes, target_frames),
        Voice::StringBow => render_stringed(string_bow_patch(), notes, target_frames),
        Voice::StringPick => render_stringed(string_pick_patch(), notes, target_frames),
        Voice::TubeLead => render_tube(TubePatch::default(), notes, target_frames),
        Voice::TubeDark => render_tube(tube_dark_patch(), notes, target_frames),
        Voice::CymbalRide => render_cymbal(cymbal_ride_patch(), notes, target_frames),
        Voice::CymbalCrash => render_cymbal(cymbal_crash_patch(), notes, target_frames),
    }
}

fn modal_keys_patch() -> ResonatorSynthPatch {
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

fn modal_bells_patch() -> ResonatorSynthPatch {
    let mut patch = modal_keys_patch();
    patch.resonator_a = ModalConfig {
        preset: ModalPreset::Bell,
        position_of_strike: 0.33,
        ..ModalConfig::default()
    };
    patch
}

fn string_bow_patch() -> StringPatch {
    StringPatch {
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
    }
}

fn string_pick_patch() -> StringPatch {
    StringPatch {
        driver: DriverSelection::Pick,
        body: BodySelection::Guitar,
        brightness: 0.45,
        stiffness: 0.40,
        damping: 0.40,
        ..StringPatch::default()
    }
}

fn tube_dark_patch() -> TubePatch {
    TubePatch {
        brightness: 0.35,
        ..TubePatch::default()
    }
}

fn cymbal_ride_patch() -> CymbalPatch {
    CymbalPatch {
        size: 0.5,
        tension: 0.45,
        damping: 0.35,
        material: 0.6,
        strike_position: 0.72,
        ..CymbalPatch::default()
    }
}

fn cymbal_crash_patch() -> CymbalPatch {
    CymbalPatch {
        size: 0.95,
        tension: 0.88,
        damping: 0.10,
        material: 0.40,
        strike_position: 0.9,
        output_gain_db: -5.0,
        ..CymbalPatch::default()
    }
}

fn render_modal(
    patch: ResonatorSynthPatch,
    notes: &[Note],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let setup = ProcessSetup {
        sample_rate: f64::from(SAMPLE_RATE),
        max_block_size: BLOCK_SIZE,
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

fn render_stringed(
    patch: StringPatch,
    notes: &[Note],
    target_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let mut processor = StringProcessor::new(
        SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(StringExcitationSource::builtin),
    );
    render_blocks(notes, target_frames, |events, left, right| {
        processor.process(events, left, right);
    })
}

fn render_tube(patch: TubePatch, notes: &[Note], target_frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut processor = TubeProcessor::new(
        SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(TubeExcitationSource::builtin),
    );
    render_blocks(notes, target_frames, |events, left, right| {
        processor.process(events, left, right);
    })
}

fn render_cymbal(patch: CymbalPatch, notes: &[Note], target_frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut processor = CymbalProcessor::new(
        SAMPLE_RATE as f32,
        patch,
        std::array::from_fn(CymbalExcitationSource::builtin),
    );
    render_blocks(notes, target_frames, |events, left, right| {
        processor.process(events, left, right);
    })
}

fn render_blocks(
    notes: &[Note],
    target_frames: usize,
    mut process: impl FnMut(&[MidiEvent], &mut [f32], &mut [f32]),
) -> (Vec<f32>, Vec<f32>) {
    let total_blocks = target_frames.div_ceil(BLOCK_SIZE);
    let mut block_left = vec![0.0; BLOCK_SIZE];
    let mut block_right = vec![0.0; BLOCK_SIZE];
    let mut left = Vec::with_capacity(total_blocks * BLOCK_SIZE);
    let mut right = Vec::with_capacity(total_blocks * BLOCK_SIZE);
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
struct ScheduledAction {
    block: usize,
    order: usize,
    event: MidiEvent,
}

fn scheduled_actions(notes: &[Note], total_blocks: usize) -> Vec<ScheduledAction> {
    let mut actions = Vec::with_capacity(notes.len() * 2);
    for (index, note) in notes.iter().enumerate() {
        actions.push(ScheduledAction {
            block: block_at(note.start_seconds, total_blocks),
            order: index * 2,
            event: MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: note.note,
                velocity: note.velocity,
            }),
        });
        actions.push(ScheduledAction {
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
    ((seconds * SAMPLE_RATE as f32) as usize / BLOCK_SIZE).min(total_blocks.saturating_sub(1))
}
