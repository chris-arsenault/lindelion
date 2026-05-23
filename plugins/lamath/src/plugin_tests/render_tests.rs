#[test]
fn audio_plugin_process_renders_default_patch() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 512,
        mode: ProcessMode::Realtime,
    };
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    synth.reset(setup);
    synth.process(ProcessContext::new(
        setup,
        AudioBuffer {
            left: &mut left,
            right: &mut right,
        },
        &events,
    ));

    assert_all_finite(&left);
    assert_all_finite(&right);
    assert!(rms(&left) > 0.000_001);
    assert!(rms(&right) > 0.000_001);
}

#[test]
fn default_patch_uses_marimba_modal_as_only_audible_resonator_path() {
    let patch = ResonatorSynthPatch::default();

    assert!(matches!(
        patch.resonator_a,
        ResonatorConfig::Modal(ModalConfig {
            preset: ModalPreset::Marimba,
            ..
        })
    ));
    assert!(matches!(
        patch.routing,
        ResonatorRouting::Parallel { mix_a, mix_b }
            if (mix_a - 1.0).abs() < 0.001 && mix_b.abs() < 0.001
    ));
    assert!(!patch.retrigger_resonators);
    assert_eq!(patch.excitation_slots.len(), 1);
    assert!(patch.excitation_slots[0].sample.is_none());
}

#[test]
fn default_single_note_velocity_100_has_nominal_headroom() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 16_384,
        mode: ProcessMode::Realtime,
    };
    let mut left = vec![0.0; 16_384];
    let mut right = vec![0.0; 16_384];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 100.0 / 127.0,
    })];

    synth.reset(setup);
    synth.process(ProcessContext::new(
        setup,
        AudioBuffer {
            left: &mut left,
            right: &mut right,
        },
        &events,
    ));

    assert_all_finite(&left);
    assert_all_finite(&right);
    let peak = peak_abs(&left).max(peak_abs(&right));
    assert!(
        peak <= db_to_gain(-6.0),
        "default velocity-100 single note peak {peak} exceeded -6 dBFS"
    );
}

#[test]
fn master_gain_is_linear_when_saturation_is_off() {
    let quiet = render_default_note(-11.0, 0.0);
    let loud = render_default_note(-3.0, 0.0);
    let expected_gain = db_to_gain(8.0);
    let mut max_error = 0.0_f32;

    for (quiet_sample, loud_sample) in quiet.iter().copied().zip(loud.iter().copied()) {
        if quiet_sample.abs() < 1.0e-7 && loud_sample.abs() < 1.0e-7 {
            continue;
        }
        max_error = max_error.max((loud_sample - quiet_sample * expected_gain).abs());
    }

    assert!(
        max_error < 1.0e-5,
        "master gain changed waveform shape with saturation off; max error {max_error}"
    );
}

#[test]
fn live_filter_cutoff_changes_do_not_emit_non_finite_audio() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 256,
        mode: ProcessMode::Realtime,
    };
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];

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
            velocity: 100.0 / 127.0,
        })],
    ));

    for normalized in [
        1.0,
        0.75,
        0.5,
        0.25,
        0.0,
        f32::NAN,
        f32::INFINITY,
        0.01,
        1.0,
    ] {
        synth.set_parameter_normalized(ParameterId(3), normalized);
        synth.process(ProcessContext::new(
            setup,
            AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            &[],
        ));
        assert_all_finite(&left);
        assert_all_finite(&right);
        let peak = peak_abs(&left).max(peak_abs(&right));
        assert!(peak < 16.0, "live cutoff change produced peak {peak}");
    }
}

#[test]
fn rapid_exposed_parameter_changes_stay_finite_and_bounded() {
    let rendered = render_automation_stress_clip();
    let settled_start = 2_048;
    let max_delta = max_adjacent_delta(&rendered.left[settled_start..])
        .max(max_adjacent_delta(&rendered.right[settled_start..]));

    assert_all_finite(&rendered.left);
    assert_all_finite(&rendered.right);
    assert!(
        rendered.peak < 8.0,
        "automation stress peak={}",
        rendered.peak
    );
    assert!(
        max_delta < 0.35,
        "rapid automation should not introduce hard sample steps, max_delta={max_delta}"
    );
}

#[test]
fn structural_parameter_changes_do_not_silence_active_voice() {
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: 256,
        mode: ProcessMode::Realtime,
    };
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];
    let mut after_change = Vec::new();

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
    for _ in 0..8 {
        process_block(&mut synth, setup, &mut left, &mut right, &[]);
    }
    for (id, plain) in [(10, 1.0), (20, 1.0), (40, 0.0), (60, 500.0), (69, 3.0)] {
        set_parameter_plain(&mut synth, id, plain);
    }
    for _ in 0..16 {
        process_block(&mut synth, setup, &mut left, &mut right, &[]);
        after_change.extend_from_slice(&left);
    }

    assert_all_finite(&after_change);
    assert!(
        rms(&after_change) > 0.000_001,
        "structural edits should not rebuild away active voices"
    );
}

#[test]
fn ableton_style_four_bar_clip_renders_inside_expected_bounds() {
    let rendered = render_qa_clip(48_000.0, 128, ProcessMode::Realtime);

    assert_all_finite(&rendered.left);
    assert_all_finite(&rendered.right);
    assert!(
        rendered.rms > 0.000_01,
        "QA clip should render audible output"
    );
    assert!(
        rendered.peak < 8.0,
        "QA clip peak should remain bounded, peak={}",
        rendered.peak
    );
}

#[test]
fn velocity_changes_excitation_level() {
    let quiet = render_single_note_rms(48_000.0, 128, 60, 32.0 / 127.0);
    let medium = render_single_note_rms(48_000.0, 128, 60, 80.0 / 127.0);
    let loud = render_single_note_rms(48_000.0, 128, 60, 127.0 / 127.0);

    assert!(medium > quiet * 1.5, "quiet={quiet}, medium={medium}");
    assert!(loud > medium * 1.2, "medium={medium}, loud={loud}");
}

#[test]
fn pitch_tracks_midi_notes() {
    let sample_rate = 48_000.0_f32;
    let c3 = render_single_note_left(sample_rate, 128, 48, 1.0);
    let c4 = render_single_note_left(sample_rate, 128, 60, 1.0);
    let c3_hz = midi_note_to_hz(48.0);
    let c4_hz = midi_note_to_hz(60.0);

    assert!(
        dft_magnitude_at(&c3[512..], sample_rate, c3_hz)
            > dft_magnitude_at(&c3[512..], sample_rate, c4_hz),
        "C3 render should favor C3 over C4"
    );
    assert!(
        dft_magnitude_at(&c4[512..], sample_rate, c4_hz)
            > dft_magnitude_at(&c4[512..], sample_rate, c3_hz),
        "C4 render should favor C4 over C3"
    );
}

#[test]
fn pitch_bend_moves_held_note_about_two_semitones() {
    let sample_rate = 48_000.0_f32;
    let mut synth = ResonatorSynth::default();
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: 128,
        mode: ProcessMode::Realtime,
    };
    let mut block_left = vec![0.0; 128];
    let mut block_right = vec![0.0; 128];

    synth.reset(setup);
    process_block(
        &mut synth,
        setup,
        &mut block_left,
        &mut block_right,
        &[MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 48,
            velocity: 1.0,
        })],
    );
    for _ in 0..4 {
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
    }
    process_block(
        &mut synth,
        setup,
        &mut block_left,
        &mut block_right,
        &[MidiEvent::Control(ControlEvent::PitchBend {
            channel: 0,
            semitones: 2.0,
        })],
    );

    let mut bent = Vec::new();
    for _ in 0..64 {
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        bent.extend_from_slice(&block_left);
    }

    let original = midi_note_to_hz(48.0);
    let bent_up = midi_note_to_hz(50.0);
    assert!(
        dft_magnitude_at(&bent, sample_rate, bent_up)
            > dft_magnitude_at(&bent, sample_rate, original),
        "pitch bend should move the held voice up about two semitones"
    );
}

#[test]
fn expression_aftertouch_changes_deterministic_render() {
    let neutral = render_expression_damping_clip(
        ModulationSource::Aftertouch,
        1,
        &[note_on(0, 48)],
        &[channel_pressure(0, 0.0)],
    );
    let pressed = render_expression_damping_clip(
        ModulationSource::Aftertouch,
        1,
        &[note_on(0, 48)],
        &[channel_pressure(0, 1.0)],
    );

    assert_expression_render_material_change("aftertouch", &neutral, &pressed);
}

#[test]
fn expression_mod_wheel_changes_deterministic_render() {
    let neutral = render_expression_damping_clip(
        ModulationSource::ModWheel,
        1,
        &[note_on(0, 48)],
        &[cc(0, 1, 0.0)],
    );
    let pushed = render_expression_damping_clip(
        ModulationSource::ModWheel,
        1,
        &[note_on(0, 48)],
        &[cc(0, 1, 1.0)],
    );

    assert_expression_render_material_change("mod wheel", &neutral, &pushed);
}

#[test]
fn expression_brightness_cc74_changes_deterministic_render() {
    let neutral = render_expression_damping_clip(
        ModulationSource::Brightness,
        1,
        &[note_on(0, 48)],
        &[cc(0, 74, 0.0)],
    );
    let bright = render_expression_damping_clip(
        ModulationSource::Brightness,
        1,
        &[note_on(0, 48)],
        &[cc(0, 74, 1.0)],
    );

    assert_expression_render_material_change("brightness", &neutral, &bright);
}

#[test]
fn expression_poly_pressure_targets_note_in_deterministic_render() {
    let notes = [note_on(0, 48), note_on(0, 60)];
    let neutral = render_expression_damping_clip(
        ModulationSource::Aftertouch,
        2,
        &notes,
        &[poly_pressure(0, 48, 0.0)],
    );
    let wrong_note = render_expression_damping_clip(
        ModulationSource::Aftertouch,
        2,
        &notes,
        &[poly_pressure(0, 72, 1.0)],
    );
    let target_note = render_expression_damping_clip(
        ModulationSource::Aftertouch,
        2,
        &notes,
        &[poly_pressure(0, 48, 1.0)],
    );

    assert_expression_render_no_material_change("wrong-note poly pressure", &neutral, &wrong_note);
    assert_expression_render_material_change("poly pressure", &neutral, &target_note);
}

#[test]
fn expression_member_channel_pitch_bend_retunes_deterministic_render() {
    let neutral = render_pitch_tracking_expression_clip(1, &[]);
    let wrong_channel = render_pitch_tracking_expression_clip(1, &[pitch_bend(2, 2.0)]);
    let bent = render_pitch_tracking_expression_clip(1, &[pitch_bend(1, 2.0)]);

    assert_expression_render_no_material_change(
        "wrong-channel pitch bend",
        &neutral,
        &wrong_channel,
    );
    assert_frequency_dominates(&neutral.left[512..], 48_000.0, 60.0, 62.0);
    assert_frequency_dominates(&bent.left[512..], 48_000.0, 62.0, 60.0);
    assert_expression_render_material_change("member-channel pitch bend", &neutral, &bent);
}

#[test]
fn exposed_parameters_materially_change_rendered_audio() {
    let master_quiet = render_single_note_with_params(&[(1, -24.0)], 48_000.0, 128);
    let master_loud = render_single_note_with_params(&[(1, 0.0)], 48_000.0, 128);
    assert!(
        master_loud.rms > master_quiet.rms * 8.0,
        "master gain should materially change level"
    );

    let loop_low = render_loop_tail_rms(0.1);
    let loop_high = render_loop_tail_rms(0.99);
    assert!(
        loop_high > loop_low * 1.5,
        "loop gain should materially lengthen the tail, low={loop_low}, high={loop_high}"
    );

    let dark = render_single_note_with_params(&[(3, 250.0)], 48_000.0, 128);
    let open = render_single_note_with_params(&[(3, 20_000.0)], 48_000.0, 128);
    assert!(
        open.rms > dark.rms * 1.25,
        "filter cutoff should materially change output, dark={}, open={}",
        dark.rms,
        open.rms
    );
}

#[test]
fn loop_resonance_parameters_are_per_slot_controls() {
    let mut synth = ResonatorSynth::default();

    set_parameter_plain(&mut synth, 20, 1.0);
    set_parameter_plain(&mut synth, 31, 0.72);
    set_parameter_plain(&mut synth, 51, 0.63);

    assert_resonator_a_loop_resonance(synth.patch(), 0.72);
    assert_resonator_b_loop_resonance(synth.patch(), 0.63);
}

#[test]
fn loop_resonance_materially_changes_exposed_waveguide_render() {
    let dry = render_single_note_with_params(
        &[
            (20, 1.0),
            (11, 1.0),
            (12, 0.0),
            (30, 1_700.0),
            (31, 0.0),
            (32, 0.965),
        ],
        48_000.0,
        128,
    );
    let resonant = render_single_note_with_params(
        &[
            (20, 1.0),
            (11, 1.0),
            (12, 0.0),
            (30, 1_700.0),
            (31, 0.9),
            (32, 0.965),
        ],
        48_000.0,
        128,
    );

    assert_all_finite(&dry.left);
    assert_all_finite(&resonant.left);
    assert!(rms_difference(&dry.left[512..], &resonant.left[512..]) > 0.000_001);
}

#[test]
fn parallel_mix_a_b_materially_changes_parallel_render() {
    let a_only = render_single_note_with_params(&[(10, 0.0), (11, 1.0), (12, 0.0)], 48_000.0, 128);
    let b_only = render_single_note_with_params(&[(10, 0.0), (11, 0.0), (12, 1.0)], 48_000.0, 128);

    assert!(a_only.rms > 0.000_001);
    assert!(b_only.rms > 0.000_001);
    assert!(rms_difference(&a_only.left[512..], &b_only.left[512..]) > 0.000_001);
}

#[test]
fn output_filter_modes_produce_distinct_bounded_output() {
    let lowpass = render_filter_mode(0.0);
    let bandpass = render_filter_mode(1.0);
    let highpass = render_filter_mode(2.0);

    assert_rendered_clip_is_finite_and_bounded(&lowpass);
    assert_rendered_clip_is_finite_and_bounded(&bandpass);
    assert_rendered_clip_is_finite_and_bounded(&highpass);
    assert!(rms_difference(&lowpass.left[512..], &bandpass.left[512..]) > 0.000_001);
    assert!(rms_difference(&lowpass.left[512..], &highpass.left[512..]) > 0.000_001);
    assert!(lowpass.rms > highpass.rms * 1.1);
}

#[test]
fn saturation_drive_materially_changes_tone_without_large_level_jump() {
    let dry = render_default_note(-3.0, 0.0);
    let driven = render_default_note(-3.0, 1.0);
    let dry_rms = rms(&dry);
    let driven_rms = rms(&driven);

    assert_all_finite(&dry);
    assert_all_finite(&driven);
    assert!(rms_difference(&dry, &driven) > 0.000_001);
    assert!(
        driven_rms > dry_rms * 0.35,
        "driven_rms={driven_rms}, dry_rms={dry_rms}"
    );
    assert!(
        driven_rms < dry_rms * 2.5,
        "driven_rms={driven_rms}, dry_rms={dry_rms}"
    );
}

#[test]
fn render_clip_is_stable_across_buffer_sizes_and_sample_rates() {
    for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
        for block_size in [32, 64, 128, 512] {
            let rendered = render_qa_clip(sample_rate, block_size, ProcessMode::Realtime);

            assert_all_finite(&rendered.left);
            assert_all_finite(&rendered.right);
            assert!(
                rendered.rms > 0.000_001,
                "clip should not be silent at sample_rate={sample_rate}, block_size={block_size}"
            );
            assert!(
                rendered.peak < 8.0,
                "clip peak should stay bounded at sample_rate={sample_rate}, block_size={block_size}, peak={}",
                rendered.peak
            );
        }
    }
}

#[test]
fn offline_render_matches_realtime_for_fixed_clip() {
    let realtime = render_qa_clip(48_000.0, 128, ProcessMode::Realtime);
    let offline = render_qa_clip(48_000.0, 128, ProcessMode::Offline);
    let mut max_diff = 0.0_f32;

    for (realtime_sample, offline_sample) in realtime
        .left
        .iter()
        .copied()
        .zip(offline.left.iter().copied())
    {
        max_diff = max_diff.max((realtime_sample - offline_sample).abs());
    }

    assert!(
        max_diff < 1.0e-6,
        "offline render should match realtime render, max diff {max_diff}"
    );
}

