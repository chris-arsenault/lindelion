use super::*;
use lindelion_dsp_utils::{
    analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
    math::midi_note_to_hz,
};
use lindelion_plugin_shell::{
    AudioBuffer, AudioPlugin, MidiEvent, NoteEvent, ParameterId, ProcessContext, ProcessMode,
    ProcessSetup,
};

#[test]
fn audio_plugin_process_renders_default_modal_patch() {
    let mut synth = ResonatorSynth::default();
    let setup = test_setup(512);
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    synth.reset(setup);
    synth.process(ProcessContext::new(
        setup,
        AudioBuffer {
            left: &mut left,
            right: &mut right,
        },
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
    ));

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_001);
    assert!(rms(&right) > 0.000_001);
}

#[test]
fn default_patch_is_dual_modal_with_a_audible() {
    let patch = ResonatorSynthPatch::default();

    assert_eq!(patch.resonator_a.preset, ModalPreset::Marimba);
    assert_eq!(patch.resonator_b.preset, ModalPreset::Bell);
    assert!(matches!(
        patch.routing,
        ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0
        }
    ));
    assert_eq!(patch.excitation_slots.len(), 1);
    assert!(patch.excitation_slots[0].sample.is_none());
}

#[test]
fn removed_model_family_parameters_are_not_registered() {
    for id in [20, 30, 35, 40, 50, 80, 81, 82, 140, 156] {
        assert!(
            parameter_info(id).is_none(),
            "removed Lamath parameter id {id} should not be registered"
        );
    }
}

#[test]
fn routing_parameter_preserves_series_mode_for_modal_pair() {
    let mut synth = ResonatorSynth::default();

    set_parameter_plain(&mut synth, 11, 0.75);
    set_parameter_plain(&mut synth, 12, 0.25);
    set_parameter_plain(&mut synth, 10, 1.0);

    let ResonatorRouting::Series { mix_a, mix_b } = synth.patch().routing else {
        panic!("expected series routing, got {:?}", synth.patch().routing);
    };
    assert!((mix_a - 0.75).abs() < 0.001);
    assert!((mix_b - 0.25).abs() < 0.001);
}

#[test]
fn pitch_bend_moves_held_modal_note() {
    let sample_rate = 48_000.0_f32;
    let mut synth = ResonatorSynth::default();
    let setup = test_setup(128);
    let mut left = vec![0.0; 128];
    let mut right = vec![0.0; 128];
    let mut neutral = Vec::new();
    let mut bent = Vec::new();

    synth.reset(setup);
    process_block(
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })],
    );
    for _ in 0..48 {
        process_block(&mut synth, setup, &mut left, &mut right, &[]);
        neutral.extend_from_slice(&left);
    }
    synth.set_pitch_bend_normalized(1.0);
    for _ in 0..48 {
        process_block(&mut synth, setup, &mut left, &mut right, &[]);
        bent.extend_from_slice(&left);
    }

    let c4 = midi_note_to_hz(60.0);
    let d4 = midi_note_to_hz(62.0);
    assert!(
        dft_magnitude_at(&neutral, sample_rate, c4) > dft_magnitude_at(&neutral, sample_rate, d4),
        "neutral held note should favor C4"
    );
    assert!(
        dft_magnitude_at(&bent, sample_rate, d4) > dft_magnitude_at(&bent, sample_rate, c4),
        "bent held note should favor D4"
    );
}

#[test]
fn default_single_note_stays_bounded() {
    let mut synth = ResonatorSynth::default();
    let setup = test_setup(16_384);
    let mut left = vec![0.0; 16_384];
    let mut right = vec![0.0; 16_384];

    synth.reset(setup);
    process_block(
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 100.0 / 127.0,
        })],
    );

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(peak_abs(&left).max(peak_abs(&right)) < 1.0);
}

fn test_setup(max_block_size: usize) -> ProcessSetup {
    ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size,
        mode: ProcessMode::Realtime,
    }
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

fn set_parameter_plain(synth: &mut ResonatorSynth, id: u32, plain: f32) {
    let normalized =
        normalized_parameter_value(id, plain).unwrap_or_else(|| panic!("missing parameter {id}"));
    synth.set_parameter_normalized(ParameterId(id), normalized);
}
