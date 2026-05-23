use lindelion_dsp_utils::{
    analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
    math::midi_note_to_hz,
    params::StructuralChangePolicy,
};

use super::{
    modulation_state::{ModulationSources, ModulationState},
    output_stage::{OutputStage, output_gain},
    resonator_stack::{ResonatorStack, SeriesConditioner, routing_plain},
    *,
};
use crate::dsp::{ExcitationLayer, SelectedExcitations};
use crate::{
    FilterMode, ModalConfig, ModalPreset, ModulationConfig, OutputConfig, ResonatorConfig,
    ResonatorRouting, ResonatorSynthPatch, WaveguideConfig,
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
fn master_pan_moves_signal_to_right_channel() {
    let sample_rate = 48_000.0;
    let mut patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    patch.output.master_pan = 1.0;
    let excitation = impulse(64);
    let mut voice = Voice::new(sample_rate);
    let mut left = vec![0.0; 4_096];
    let mut right = vec![0.0; 4_096];

    voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));
    voice.render_add(&mut left, &mut right);

    assert!(rms(&right) > 0.000_1);
    assert!(rms(&left) < rms(&right) * 0.001);
}

#[test]
fn master_gain_changes_render_level() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let mut unity_patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    let mut quiet_patch = unity_patch.clone();
    unity_patch.output.master_gain_db = 0.0;
    quiet_patch.output.master_gain_db = -12.0;

    let unity = render_mono_energy(sample_rate, &unity_patch, &excitation);
    let quiet = render_mono_energy(sample_rate, &quiet_patch, &excitation);

    assert!(quiet < unity * 0.35, "quiet={quiet}, unity={unity}");
    assert!(quiet > unity * 0.20, "quiet={quiet}, unity={unity}");
}

#[test]
fn output_lowpass_reduces_high_frequency_content() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let mut open_patch = bright_modal_patch();
    let mut dark_patch = open_patch.clone();
    open_patch.output.filter_cutoff = 20_000.0;
    dark_patch.output.filter_cutoff = 800.0;

    let open = render_left(sample_rate, &open_patch, &excitation);
    let dark = render_left(sample_rate, &dark_patch, &excitation);

    assert!(
        dft_magnitude_at(&dark[512..], sample_rate, 6_000.0)
            < dft_magnitude_at(&open[512..], sample_rate, 6_000.0) * 0.6
    );
}

#[test]
fn series_voice_stays_finite_and_bounded() {
    let sample_rate = 48_000.0;
    let mut patch = test_patch(ResonatorRouting::Series {
        mix_a: 0.5,
        mix_b: 0.5,
    });
    patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
        loop_gain: 0.995,
        loop_filter_cutoff: 18_000.0,
        loop_nonlinearity: 0.4,
        ..WaveguideConfig::default()
    });
    let excitation = impulse(256);
    let rendered = render_left(sample_rate, &patch, &excitation);

    assert_all_finite(&rendered);
    assert!(peak_abs(&rendered) < 4.0);
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
    assert!(
        steady_rms < onset_rms * 0.35,
        "onset_rms={onset_rms}, steady_rms={steady_rms}"
    );
}

#[test]
fn retrigger_off_preserves_resonator_state_for_reused_voice() {
    let sample_rate = 48_000.0;
    let preserved = render_silent_retrigger_after_impulse(sample_rate, false);
    let reset = render_silent_retrigger_after_impulse(sample_rate, true);

    assert!(preserved > 0.000_01, "preserved={preserved}");
    assert!(
        reset < preserved * 0.05,
        "reset={reset}, preserved={preserved}"
    );
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
        mix_a: 0.8,
        mix_b: 0.2,
    });
    assert_voice_routing_kind(&voice, 0);
    assert!(voice.resonators.routing.has_pending());
    drain_structural_transitions(&mut voice);
    assert_voice_routing_kind(&voice, 1);

    voice.set_routing(ResonatorRouting::Parallel {
        mix_a: 0.1,
        mix_b: 0.9,
    });
    assert_voice_routing_kind(&voice, 1);
    drain_structural_transitions(&mut voice);
    assert_voice_routing_kind(&voice, 0);
    assert_parallel_mix_targets(&voice, 0.1, 0.9);
    assert_parallel_mix_is_smoothing(&voice);
}

#[test]
fn filter_mode_is_structural_and_applies_with_mute_ramp() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    let mut voice = Voice::new(sample_rate);
    voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));

    let mut output = patch.output;
    output.filter_mode = FilterMode::HighPass;
    voice.set_output_config(output);

    assert_eq!(
        voice.output.filter_mode.policy(),
        StructuralChangePolicy::LiveMuteRamp
    );
    assert_eq!(voice.output.filter_mode.current(), FilterMode::LowPass);
    assert!(voice.output.filter_mode.has_pending());
    drain_structural_transitions(&mut voice);
    assert_eq!(voice.output.filter_mode.current(), FilterMode::HighPass);
    assert!(!voice.output.filter_mode.has_pending());
}

#[test]
fn live_parameter_changes_are_smoothed_per_sample() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let mut patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 0.0,
        mix_b: 1.0,
    });
    patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig {
        loop_gain: 0.92,
        ..WaveguideConfig::default()
    });
    let mut voice = Voice::new(sample_rate);
    voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));

    let mut output = patch.output;
    output.master_gain_db = -60.0;
    output.saturation_drive = 1.0;
    output.master_pan = 1.0;
    output.filter_cutoff = 200.0;
    output.filter_resonance = 0.8;
    voice.set_output_config(output);
    voice.set_routing(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    voice.set_waveguide_loop_gain(0.1);
    voice.set_pitch_bend(2.0);

    assert_output_params_are_smoothing(&voice);
    assert_parallel_mix_is_smoothing(&voice);
    assert_resonator_controls_are_smoothing(&voice);
    assert_master_gain_is_ramping_down(&mut voice);
}

#[test]
fn output_stage_updates_filter_and_gain_targets() {
    let sample_rate = 48_000.0;
    let mut output = OutputStage::new(sample_rate);
    let mut config = OutputConfig::default();
    config.filter_mode = FilterMode::HighPass;
    config.filter_cutoff = 400.0;
    config.filter_resonance = 0.7;
    config.master_gain_db = -18.0;
    config.saturation_drive = 0.6;
    config.master_pan = 0.5;

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
fn modulation_state_updates_pitch_lfo_and_loop_gain_paths() {
    let sample_rate = 48_000.0;
    let mut modulation = ModulationConfig::default();
    modulation.lfo.rate_hz = 3.0;
    let mut state = ModulationState::new(sample_rate);

    state.trigger(64, VoiceExpression::note_on(0.75), modulation, 0.0);
    state.set_pitch_bend(2.0);
    state.set_waveguide_loop_gain(0.35);

    assert!(state.pitch_bend_semitones.is_smoothing());
    assert!(state.waveguide_loop_gain.is_smoothing());
    assert!(state.next_pitch_bend_change().is_some());
    assert!(state.next_waveguide_loop_gain_change().is_some());

    let sources = state.next_sources(sample_rate);
    assert!(sources.amp_envelope.is_finite());
    assert!(sources.secondary_envelope.is_finite());
    assert!(sources.lfo.is_finite());
    assert_eq!(sources.velocity, 0.75);
}

#[test]
fn resonator_stack_updates_routing_and_base_waveguide_state() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    let mut stack = ResonatorStack::new(sample_rate);

    stack.set_base_configs(patch.resonator_a, patch.resonator_b);
    stack.configure_modulated(
        ModulationConfig::default(),
        ModulationSources {
            amp_envelope: 1.0,
            secondary_envelope: 0.0,
            lfo: 0.0,
            velocity: 1.0,
            aftertouch: 0.0,
            mod_wheel: 0.0,
            brightness: 0.0,
        },
        midi_note_to_hz(60.0),
        true,
        true,
    );
    stack.set_routing(ResonatorRouting::Series {
        mix_a: 0.25,
        mix_b: 0.75,
    });
    stack.set_base_waveguide_loop_gain(0.42);

    assert!(stack.routing.has_pending());
    assert_eq!(stack.parallel_mix_a.target(), 0.25);
    assert_eq!(stack.parallel_mix_b.target(), 0.75);
    assert_eq!(stack.current_loop_gain(), 0.94);
    match stack.base_resonator_b_config {
        ResonatorConfig::Waveguide(config) => assert_eq!(config.loop_gain, 0.42),
        _ => panic!("expected resonator B to remain a waveguide"),
    }

    let output = stack.process_sample(1.0);
    assert!(output.is_finite());

    drain_resonator_stack_transitions(&mut stack, sample_rate);
    assert_eq!(routing_plain(stack.routing.current()), 1);
}

fn assert_voice_routing_kind(voice: &Voice<'_>, expected: u8) {
    assert_eq!(routing_plain(voice.resonators.routing.current()), expected);
}

fn assert_parallel_mix_targets(voice: &Voice<'_>, mix_a: f32, mix_b: f32) {
    assert_eq!(voice.resonators.parallel_mix_a.target(), mix_a);
    assert_eq!(voice.resonators.parallel_mix_b.target(), mix_b);
}

fn assert_parallel_mix_is_smoothing(voice: &Voice<'_>) {
    assert!(voice.resonators.parallel_mix_a.is_smoothing());
    assert!(voice.resonators.parallel_mix_b.is_smoothing());
}

fn assert_output_params_are_smoothing(voice: &Voice<'_>) {
    assert!(voice.output.master_gain.is_smoothing());
    assert!(voice.output.saturation_drive.is_smoothing());
    assert!(voice.output.master_pan.is_smoothing());
    assert!(voice.output.filter_cutoff.is_smoothing());
    assert!(voice.output.filter_resonance.is_smoothing());
}

fn assert_resonator_controls_are_smoothing(voice: &Voice<'_>) {
    assert!(voice.modulation.waveguide_loop_gain.is_smoothing());
    assert!(voice.modulation.pitch_bend_semitones.is_smoothing());
}

fn assert_master_gain_is_ramping_down(voice: &mut Voice<'_>) {
    let first_gain = voice.output.master_gain.next_sample();
    assert!(first_gain < output_gain(0.0));
    assert!(first_gain > output_gain(-60.0));
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

fn render_mono_energy(sample_rate: f32, patch: &ResonatorSynthPatch, excitation: &[f32]) -> f32 {
    let left = render_left(sample_rate, patch, excitation);
    rms(&left)
}

fn render_left(sample_rate: f32, patch: &ResonatorSynthPatch, excitation: &[f32]) -> Vec<f32> {
    let mut voice = Voice::new(sample_rate);
    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];
    voice.trigger(VoiceTrigger::new(60, 1.0, excitation, sample_rate, patch));
    voice.render_add(&mut left, &mut right);
    left
}

fn render_silent_retrigger_after_impulse(sample_rate: f32, retrigger_resonators: bool) -> f32 {
    let mut patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    patch.retrigger_resonators = retrigger_resonators;
    patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
        loop_gain: 0.985,
        loop_filter_cutoff: 12_000.0,
        ..WaveguideConfig::default()
    });
    let excitation = impulse(64);
    let mut voice = Voice::new(sample_rate);
    voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));

    for _ in 0..512 {
        voice.process_sample();
    }

    voice.trigger(VoiceTrigger::with_excitations(
        60,
        1.0,
        SelectedExcitations::default(),
        &patch,
    ));

    let mut output = Vec::with_capacity(1024);
    for _ in 0..1024 {
        output.push(voice.process_sample());
    }

    rms(&output)
}

fn impulse(len: usize) -> Vec<f32> {
    let mut excitation = vec![0.0; len];
    excitation[0] = 1.0;
    excitation
}

fn test_patch(routing: ResonatorRouting) -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Modal(ModalConfig {
            mode_count: 24,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.8,
            brightness: 0.6,
            ..ModalConfig::default()
        }),
        resonator_b: ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.94,
            loop_filter_cutoff: 10_000.0,
            ..WaveguideConfig::default()
        }),
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

fn bright_modal_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Modal(ModalConfig {
            mode_count: 64,
            preset: ModalPreset::GenericStrike,
            decay_global: 1.0,
            decay_tilt: 0.0,
            brightness: 1.0,
            ..ModalConfig::default()
        }),
        resonator_b: ResonatorConfig::Modal(ModalConfig {
            mode_count: 1,
            ..ModalConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
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
