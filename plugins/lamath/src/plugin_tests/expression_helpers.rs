fn render_expression_damping_clip(
    source: ModulationSource,
    polyphony: u8,
    start_events: &[MidiEvent],
    control_events: &[MidiEvent],
) -> RenderedClip {
    render_expression_clip(
        expression_damping_test_patch(source, polyphony),
        start_events,
        control_events,
    )
}

fn render_pitch_tracking_expression_clip(
    channel: u8,
    control_events: &[MidiEvent],
) -> RenderedClip {
    render_expression_clip(
        pitch_tracking_expression_patch(),
        &[note_on(channel, 60)],
        control_events,
    )
}

fn render_expression_clip(
    patch: ResonatorSynthPatch,
    start_events: &[MidiEvent],
    control_events: &[MidiEvent],
) -> RenderedClip {
    let sample_rate = 48_000.0;
    let block_size = 128;
    let setup = ProcessSetup {
        sample_rate,
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity(block_size * 65);
    let mut right = Vec::with_capacity(block_size * 65);

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    process_block(
        &mut synth,
        setup,
        &mut block_left,
        &mut block_right,
        start_events,
    );
    for _ in 0..4 {
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
    }
    process_block(
        &mut synth,
        setup,
        &mut block_left,
        &mut block_right,
        control_events,
    );
    left.extend_from_slice(&block_left);
    right.extend_from_slice(&block_right);

    for _ in 0..64 {
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    rendered_clip(left, right)
}

fn rendered_clip(left: Vec<f32>, right: Vec<f32>) -> RenderedClip {
    let rms = rms(&left).max(rms(&right));
    let peak = peak_abs(&left).max(peak_abs(&right));
    RenderedClip {
        left,
        right,
        rms,
        peak,
    }
}

fn assert_expression_render_material_change(
    label: &str,
    neutral: &RenderedClip,
    changed: &RenderedClip,
) {
    assert_rendered_clip_is_finite_and_bounded(neutral);
    assert_rendered_clip_is_finite_and_bounded(changed);
    let diff = rms_difference(&neutral.left[512..], &changed.left[512..]);
    assert!(
        diff > neutral.rms.max(0.000_001) * 0.05,
        "{label} should materially change render, neutral_rms={}, changed_rms={}, diff={diff}",
        neutral.rms,
        changed.rms
    );
}

fn assert_expression_render_no_material_change(
    label: &str,
    neutral: &RenderedClip,
    changed: &RenderedClip,
) {
    assert_rendered_clip_is_finite_and_bounded(neutral);
    assert_rendered_clip_is_finite_and_bounded(changed);
    let diff = rms_difference(&neutral.left[512..], &changed.left[512..]);
    assert!(
        diff < 1.0e-7,
        "{label} should not materially change render, diff={diff}"
    );
}

fn render_loop_tail_rms(loop_gain: f32) -> f32 {
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 128,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; 128];
    let mut block_right = vec![0.0; 128];
    let mut tail = Vec::new();

    synth.reset(setup);
    synth.set_patch_for_test(waveguide_tail_test_patch());
    set_parameter_plain(&mut synth, 32, loop_gain);
    for block in 0..256 {
        match block {
            0 => process_block(
                &mut synth,
                setup,
                &mut block_left,
                &mut block_right,
                &[MidiEvent::Note(NoteEvent::On {
                    channel: 0,
                    note: 60,
                    velocity: 1.0,
                })],
            ),
            24 => process_block(
                &mut synth,
                setup,
                &mut block_left,
                &mut block_right,
                &[MidiEvent::Note(NoteEvent::Off {
                    channel: 0,
                    note: 60,
                    velocity: 0.0,
                })],
            ),
            _ => process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]),
        }
        if block >= 96 {
            tail.extend_from_slice(&block_left);
        }
    }

    rms(&tail)
}

fn waveguide_tail_test_patch() -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        polyphony: 1,
        resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
            loop_gain: 0.92,
            loop_filter_cutoff: 18_000.0,
            ..WaveguideConfig::default()
        }),
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

fn expression_damping_test_patch(source: ModulationSource, polyphony: u8) -> ResonatorSynthPatch {
    let mut patch = waveguide_tail_test_patch();
    patch.polyphony = polyphony;
    patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
        loop_gain: 0.62,
        loop_filter_cutoff: 12_000.0,
        ..WaveguideConfig::default()
    });
    patch.modulation.slots[0] = ModulationSlot {
        enabled: true,
        source,
        destination: ModulationDestination::ResonatorADamping,
        amount: 1.0,
    };
    patch
}

fn pitch_tracking_expression_patch() -> ResonatorSynthPatch {
    let mut patch = waveguide_tail_test_patch();
    patch.polyphony = 1;
    patch.resonator_a = ResonatorConfig::Waveguide(WaveguideConfig {
        loop_gain: 0.99,
        loop_filter_cutoff: 12_000.0,
        ..WaveguideConfig::default()
    });
    patch
}

fn note_on(channel: u8, note: u8) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel,
        note,
        velocity: 1.0,
    })
}

fn channel_pressure(channel: u8, value: f32) -> MidiEvent {
    MidiEvent::Control(ControlEvent::ChannelPressure { channel, value })
}

fn poly_pressure(channel: u8, note: u8, value: f32) -> MidiEvent {
    MidiEvent::Control(ControlEvent::PolyPressure {
        channel,
        note,
        value,
    })
}

fn cc(channel: u8, controller: u8, value: f32) -> MidiEvent {
    MidiEvent::Control(ControlEvent::ContinuousController {
        channel,
        controller,
        value,
    })
}

fn pitch_bend(channel: u8, semitones: f32) -> MidiEvent {
    MidiEvent::Control(ControlEvent::PitchBend { channel, semitones })
}

fn assert_frequency_dominates(samples: &[f32], sample_rate: f32, high_note: f32, low_note: f32) {
    let high = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(high_note));
    let low = dft_magnitude_at(samples, sample_rate, midi_note_to_hz(low_note));
    assert!(
        high > low,
        "note {high_note} magnitude {high} should exceed note {low_note} magnitude {low}"
    );
}

fn set_parameter_plain(synth: &mut ResonatorSynth, id: u32, plain: f32) {
    let Some(parameter) = PARAMETERS
        .iter()
        .find(|parameter| parameter.id == ParameterId(id))
    else {
        return;
    };
    synth.set_parameter_normalized(ParameterId(id), parameter.range.normalize(plain));
}

fn assert_resonator_b_loop_gain(patch: &ResonatorSynthPatch, expected: f32) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
        panic!("expected resonator B to be waveguide");
    };
    assert!((config.loop_gain - expected).abs() < 0.001);
}

fn assert_resonator_a_loop_resonance(patch: &ResonatorSynthPatch, expected: f32) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_a else {
        panic!("expected resonator A to be waveguide");
    };
    assert!((config.loop_filter_resonance - expected).abs() < 0.001);
}

fn assert_resonator_b_loop_resonance(patch: &ResonatorSynthPatch, expected: f32) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
        panic!("expected resonator B to be waveguide");
    };
    assert!((config.loop_filter_resonance - expected).abs() < 0.001);
}

fn assert_resonator_a_waveguide_style(patch: &ResonatorSynthPatch, expected: WaveguideStyle) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_a else {
        panic!("expected resonator A to be waveguide");
    };
    assert_eq!(config.style, expected);
}

fn assert_resonator_b_waveguide_style(patch: &ResonatorSynthPatch, expected: WaveguideStyle) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
        panic!("expected resonator B to be waveguide");
    };
    assert_eq!(config.style, expected);
}

fn assert_resonator_a_boundary_reflection(patch: &ResonatorSynthPatch, expected: f32) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_a else {
        panic!("expected resonator A to be waveguide");
    };
    assert!((config.boundary_reflection - expected).abs() < 0.001);
}

fn assert_resonator_b_boundary_reflection(patch: &ResonatorSynthPatch, expected: f32) {
    let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
        panic!("expected resonator B to be waveguide");
    };
    assert!((config.boundary_reflection - expected).abs() < 0.001);
}

fn rms_difference(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len()).max(1);
    let sum = a
        .iter()
        .copied()
        .zip(b.iter().copied())
        .map(|(a, b)| {
            let diff = a - b;
            diff * diff
        })
        .sum::<f32>();
    (sum / len as f32).sqrt()
}

fn max_adjacent_delta(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|window| (window[1] - window[0]).abs())
        .fold(0.0, f32::max)
}
