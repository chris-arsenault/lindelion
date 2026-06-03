use lindelion_dsp_utils::{
    analysis::{assert_all_finite, peak_abs, rms},
    params::StructuralChangePolicy,
};

use super::{
    output_stage::OutputStage,
    resonator_stack::{ResonatorStack, SeriesConditioner, routing_plain},
    *,
};
use crate::dsp::{SelectedExcitations, excitation::ExcitationLayer};
use crate::{
    FilterMode, ModalConfig, ModalPreset, OutputConfig, ResonatorRouting, ResonatorSynthPatch,
};

#[test]
fn parallel_voice_renders_nonzero_stereo_output() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 0.75,
        mix_b: 0.25,
    });
    let excitation = impulse(256);
    let mut voice = Voice::new(sample_rate);
    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];

    voice.trigger(VoiceTrigger::new(57, 1.0, &excitation, sample_rate, &patch));
    voice.render_add(&mut left, &mut right);

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_1);
    assert!(rms(&right) > 0.000_1);
}

#[test]
fn layered_excitation_trigger_renders_louder_than_single_layer() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    let excitation_a = impulse(64);
    let excitation_b = impulse(64);
    let mut single_selected = SelectedExcitations::default();
    let mut selected = SelectedExcitations::default();
    single_selected.push(ExcitationLayer {
        gain: 0.1,
        ..ExcitationLayer::new(&excitation_a, sample_rate)
    });
    selected.push(ExcitationLayer {
        gain: 0.1,
        ..ExcitationLayer::new(&excitation_a, sample_rate)
    });
    selected.push(ExcitationLayer {
        gain: 0.1,
        ..ExcitationLayer::new(&excitation_b, sample_rate)
    });

    let mut single_voice = Voice::new(sample_rate);
    let mut single_left = vec![0.0; 8_192];
    let mut single_right = vec![0.0; 8_192];
    let mut layered_voice = Voice::new(sample_rate);
    let mut layered_left = vec![0.0; 8_192];
    let mut layered_right = vec![0.0; 8_192];
    single_voice.trigger(VoiceTrigger::with_excitations(
        60,
        1.0,
        single_selected,
        &patch,
    ));
    layered_voice.trigger(VoiceTrigger::with_excitations(60, 1.0, selected, &patch));
    single_voice.render_add(&mut single_left, &mut single_right);
    layered_voice.render_add(&mut layered_left, &mut layered_right);

    assert!(rms(&layered_left) > rms(&single_left) * 1.8);
}

#[test]
fn series_voice_stays_finite_and_bounded() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Series {
        mix_a: 1.0,
        mix_b: 1.0,
    });
    let excitation = impulse(256);
    let rendered = render_left(sample_rate, &patch, &excitation);
    let peak = peak_abs(&rendered);

    assert_all_finite(&rendered);
    assert!(peak < 4.0, "series voice peak {peak}");
    assert!(rms(&rendered) > 0.000_001);
}

#[test]
fn series_conditioner_deemphasizes_steady_state_after_onset() {
    let sample_rate = 48_000.0;
    let mut conditioner = SeriesConditioner::new(sample_rate);
    let mut output = Vec::with_capacity(24_000);

    for index in 0..24_000 {
        let input = (std::f32::consts::TAU * 220.0 * index as f32 / sample_rate).sin();
        output.push(conditioner.process_sample(input));
    }

    let onset_rms = rms(&output[64..1_088]);
    let steady_rms = rms(&output[20_000..23_000]);
    assert!(steady_rms < onset_rms * 0.35);
}

#[test]
fn held_voice_accepts_live_routing_changes() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 0.8,
        mix_b: 0.2,
    });
    let excitation = impulse(64);
    let mut voice = Voice::new(sample_rate);

    voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));
    assert_voice_routing_kind(&voice, 0);
    assert_eq!(
        voice.resonators.routing.policy(),
        StructuralChangePolicy::LiveMuteRamp
    );

    voice.set_routing(ResonatorRouting::Series {
        mix_a: 1.0,
        mix_b: 1.0,
    });
    assert!(voice.resonators.routing.has_pending());
    drain_structural_transitions(&mut voice);
    assert_voice_routing_kind(&voice, 1);
}

#[test]
fn output_stage_updates_filter_and_gain_targets() {
    let sample_rate = 48_000.0;
    let mut output = OutputStage::new(sample_rate);
    let config = OutputConfig {
        filter_mode: FilterMode::HighPass,
        filter_cutoff: 400.0,
        filter_resonance: 0.7,
        master_gain_db: -18.0,
        saturation_drive: 0.6,
        master_pan: 0.5,
    };

    output.set_config(config);

    assert_eq!(output.filter_mode.current(), FilterMode::LowPass);
    assert!(output.filter_mode.has_pending());
    assert!(output.filter_cutoff.is_smoothing());
    assert!(output.filter_resonance.is_smoothing());
    assert!(output.master_gain.is_smoothing());
    assert!(output.saturation_drive.is_smoothing());
    assert!(output.master_pan.is_smoothing());

    let sample = output.process_sample(0.25, sample_rate, 0.0, 1.0, 1.0);
    assert!(sample.is_finite());

    drain_output_stage_transitions(&mut output, sample_rate);
    assert_eq!(output.filter_mode.current(), FilterMode::HighPass);
}

#[test]
fn resonator_stack_updates_routing_and_modal_state() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    let mut stack = ResonatorStack::new(sample_rate);

    stack.set_base_configs(patch.resonator_a, patch.resonator_b);
    stack.configure(440.0, true, true);
    stack.set_routing(ResonatorRouting::Series {
        mix_a: 1.0,
        mix_b: 1.0,
    });

    assert!(stack.routing.has_pending());
    assert_eq!(stack.parallel_mix_a.target(), 1.0);
    assert_eq!(stack.parallel_mix_b.target(), 1.0);
    assert!(stack.process_sample(1.0).is_finite());

    drain_resonator_stack_transitions(&mut stack, sample_rate);
    assert_eq!(routing_plain(stack.routing.current()), 1);
}

fn assert_voice_routing_kind(voice: &Voice<'_>, expected: u8) {
    assert_eq!(routing_plain(voice.resonators.routing.current()), expected);
}

fn drain_structural_transitions(voice: &mut Voice<'_>) {
    for _ in 0..(structural_ramp_samples(voice.sample_rate) * 2 + 1) {
        voice.apply_structural_transitions();
    }
}

fn drain_output_stage_transitions(output: &mut OutputStage, sample_rate: f32) {
    for _ in 0..(structural_ramp_samples(sample_rate) * 2 + 1) {
        output.apply_structural_transitions();
    }
}

fn drain_resonator_stack_transitions(stack: &mut ResonatorStack, sample_rate: f32) {
    for _ in 0..(structural_ramp_samples(sample_rate) * 2 + 1) {
        stack.apply_structural_transitions(sample_rate);
    }
}

fn render_left(sample_rate: f32, patch: &ResonatorSynthPatch, excitation: &[f32]) -> Vec<f32> {
    let mut voice = Voice::new(sample_rate);
    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];
    voice.trigger(VoiceTrigger::new(60, 1.0, excitation, sample_rate, patch));
    voice.render_add(&mut left, &mut right);
    left
}

fn impulse(len: usize) -> Vec<f32> {
    let mut excitation = vec![0.0; len];
    excitation[0] = 1.0;
    excitation
}

fn test_patch(routing: ResonatorRouting) -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a: ModalConfig {
            mode_count: 24,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.8,
            brightness: 0.6,
            ..ModalConfig::default()
        },
        resonator_b: ModalConfig {
            mode_count: 32,
            preset: ModalPreset::Bell,
            decay_global: 1.2,
            brightness: 0.8,
            ..ModalConfig::default()
        },
        routing,
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            filter_resonance: 0.0,
            saturation_drive: 0.0,
            master_gain_db: 0.0,
            master_pan: 0.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}
