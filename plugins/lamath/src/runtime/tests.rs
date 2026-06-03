use super::*;
use crate::{
    AudioInputMode, LiveExcitationMode, ModalConfig, ModalPreset, OutputConfig, ResonatorRouting,
    assert_no_allocations,
    test_support::{audio_note_detection_patch, sidechain_sine_note},
};
use lindelion_dsp_utils::analysis::{assert_all_finite, dft_magnitude_at, rms};
use lindelion_dsp_utils::math::midi_note_to_hz;
use lindelion_plugin_shell::{MidiEvent, NoteEvent};

#[test]
fn note_on_starts_modal_voice() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        &mut left,
        &mut right,
    );

    assert_eq!(processor.active_voice_count(), 1);
    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_001);
}

#[test]
fn runtime_process_does_not_allocate() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 128];
    let mut right = vec![0.0; 128];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    assert_no_allocations("modal runtime process", || {
        processor.process(&events, &mut left, &mut right);
    });
    assert_all_finite(&left);
    assert_all_finite(&right);
}

#[test]
fn pitch_bend_retunes_held_modal_voice() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 128];
    let mut right = vec![0.0; 128];
    let mut neutral = Vec::new();
    let mut bent = Vec::new();

    processor.process(
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
        &mut left,
        &mut right,
    );
    for _ in 0..32 {
        processor.process(&[], &mut left, &mut right);
        neutral.extend_from_slice(&left);
    }
    processor.set_pitch_bend_normalized(1.0);
    for _ in 0..32 {
        processor.process(&[], &mut left, &mut right);
        bent.extend_from_slice(&left);
    }

    let c4 = midi_note_to_hz(60.0);
    let d4 = midi_note_to_hz(62.0);
    assert!(dft_magnitude_at(&neutral, 48_000.0, c4) > dft_magnitude_at(&neutral, 48_000.0, d4));
    assert!(dft_magnitude_at(&bent, 48_000.0, d4) > dft_magnitude_at(&bent, 48_000.0, c4));
}

#[test]
fn live_excitation_sidechain_is_kept() {
    let mut patch = test_patch();
    patch.live_excitation.mode = LiveExcitationMode::Continuous;
    patch.live_excitation.gain_db = 12.0;
    let sidechain = sidechain_sine_note(60.0, 0.5, 512);
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })])
        .with_sidechain(&sidechain),
        &mut left,
        &mut right,
    );

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_001);
}

#[test]
fn audio_note_sidechain_path_still_creates_voice() {
    let patch = audio_note_detection_patch(test_patch(), AudioInputMode::AudioCreatesNotes);
    let mut processor =
        ResonatorProcessor::with_builtin_excitation_and_realtime_capacity(48_000.0, patch, 4_096);
    let sidechain = sidechain_sine_note(60.0, 0.8, 4_096);
    let mut left = vec![0.0; 4_096];
    let mut right = vec![0.0; 4_096];

    processor.process_with_runtime_input(
        ResonatorRuntimeInput::new(&[]).with_sidechain(&sidechain),
        &mut left,
        &mut right,
    );

    assert_all_finite(&left);
    assert_all_finite(&right);
}

fn test_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        name: "Runtime Test".to_string(),
        polyphony: 4,
        resonator_a: ModalConfig {
            mode_count: 32,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.8,
            brightness: 0.8,
            ..ModalConfig::default()
        },
        resonator_b: ModalConfig {
            mode_count: 32,
            preset: ModalPreset::Bell,
            decay_global: 1.2,
            brightness: 0.7,
            ..ModalConfig::default()
        },
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            master_gain_db: 0.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}
