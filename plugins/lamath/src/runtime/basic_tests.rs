#[test]
fn processor_handles_note_events_and_renders_audio() {
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
fn processor_audio_path_does_not_allocate() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    assert_no_allocations("processor process note-on", || {
        processor.process(&events, &mut left, &mut right);
    });
    assert_no_allocations("processor process render-only", || {
        processor.process(&[], &mut left, &mut right);
    });
    let controls = [
        MidiEvent::Control(ControlEvent::PitchBend {
            channel: 0,
            semitones: 1.5,
        }),
        MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.75,
        }),
        MidiEvent::Control(ControlEvent::PolyPressure {
            channel: 0,
            note: 60,
            value: 0.65,
        }),
        MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 1,
            value: 0.5,
        }),
        MidiEvent::Control(ControlEvent::ContinuousController {
            channel: 0,
            controller: 74,
            value: 0.25,
        }),
    ];
    assert_no_allocations("processor process controls", || {
        processor.process(&controls, &mut left, &mut right);
    });

    assert_live_control_path_does_not_allocate(
        "processor process live pressure resonator damping",
        aftertouch_resonator_damping_patch(),
        ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
    assert_live_control_path_does_not_allocate(
        "processor process live mod wheel resonator damping",
        mod_wheel_resonator_damping_patch(),
        ControlEvent::ContinuousController {
            channel: 0,
            controller: 1,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
    assert_live_control_path_does_not_allocate(
        "processor process live brightness resonator damping",
        brightness_resonator_damping_patch(),
        ControlEvent::ContinuousController {
            channel: 0,
            controller: 74,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
    assert_live_control_path_does_not_allocate(
        "processor process live poly pressure resonator damping",
        poly_pressure_resonator_damping_patch(),
        ControlEvent::PolyPressure {
            channel: 0,
            note: 60,
            value: 0.85,
        },
        &events,
        &mut left,
        &mut right,
    );
}

fn assert_live_control_path_does_not_allocate(
    label: &str,
    patch: ResonatorSynthPatch,
    control: ControlEvent,
    note_events: &[MidiEvent],
    left: &mut [f32],
    right: &mut [f32],
) {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
    processor.process(note_events, left, right);
    assert_no_allocations(label, || {
        processor.process(&[MidiEvent::Control(control)], left, right);
    });
}

#[test]
fn held_voice_consumes_expression_stream_updates_each_block() {
    let sample_rate = 48_000.0;
    let mut neutral_processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, expression_filter_patch());
    let mut pressed_processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, expression_filter_patch());
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 256];
    let mut warmup_right = vec![0.0; 256];
    neutral_processor.process(&note_on, &mut warmup_left, &mut warmup_right);
    pressed_processor.process(&note_on, &mut warmup_left, &mut warmup_right);

    let mut neutral_left = vec![0.0; 4096];
    let mut neutral_right = vec![0.0; 4096];
    let mut pressed_left = vec![0.0; 4096];
    let mut pressed_right = vec![0.0; 4096];
    neutral_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 0.0,
        })],
        &mut neutral_left,
        &mut neutral_right,
    );
    pressed_processor.process(
        &[MidiEvent::Control(ControlEvent::ChannelPressure {
            channel: 0,
            value: 1.0,
        })],
        &mut pressed_left,
        &mut pressed_right,
    );

    assert_eq!(neutral_processor.active_voice_count(), 1);
    assert_eq!(pressed_processor.active_voice_count(), 1);
    assert_all_finite(&neutral_left);
    assert_all_finite(&neutral_right);
    assert_all_finite(&pressed_left);
    assert_all_finite(&pressed_right);
    assert!(rms(&neutral_left) > 0.000_001);
    assert!(
        mean_abs_difference(&neutral_left, &pressed_left) > rms(&neutral_left) * 0.05,
        "neutral_rms={}, pressed_rms={}, diff={}",
        rms(&neutral_left),
        rms(&pressed_left),
        mean_abs_difference(&neutral_left, &pressed_left)
    );
}

#[test]
fn non_midi_expression_source_drives_pressure_and_brightness_without_midi_events() {
    let sample_rate = 48_000.0;
    let patch = external_expression_filter_patch();
    let mut neutral_processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, patch.clone());
    let mut driven_processor = ResonatorProcessor::with_builtin_excitation(sample_rate, patch);
    let mut neutral_source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
    let mut driven_source = ManualExpressionSource::<MIDI_EXPRESSION_VOICES>::default();
    let neutral_stream = ExpressionStream {
        velocity: 1.0,
        gate: true,
        ..ExpressionStream::default()
    };
    let driven_stream = ExpressionStream {
        pressure: 0.75,
        brightness: 0.85,
        ..neutral_stream
    };
    assert!(neutral_source.set_voice_stream(0, neutral_stream));
    assert!(driven_source.set_voice_stream(0, driven_stream));

    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];
    let mut warmup_left = vec![0.0; 512];
    let mut warmup_right = vec![0.0; 512];
    neutral_processor.process_with_expression_source(
        &mut neutral_source,
        &note_on,
        &mut warmup_left,
        &mut warmup_right,
    );
    driven_processor.process_with_expression_source(
        &mut driven_source,
        &note_on,
        &mut warmup_left,
        &mut warmup_right,
    );

    let mut neutral_left = vec![0.0; 4096];
    let mut neutral_right = vec![0.0; 4096];
    let mut driven_left = vec![0.0; 4096];
    let mut driven_right = vec![0.0; 4096];
    neutral_processor.process_with_expression_source(
        &mut neutral_source,
        &[],
        &mut neutral_left,
        &mut neutral_right,
    );
    driven_processor.process_with_expression_source(
        &mut driven_source,
        &[],
        &mut driven_left,
        &mut driven_right,
    );

    let neutral = expression_for_slot(&neutral_processor, 0, 60);
    let driven = expression_for_slot(&driven_processor, 0, 60);
    assert_eq!(neutral.stream.pressure, 0.0);
    assert_eq!(neutral.stream.brightness, 0.0);
    assert_eq!(driven.stream.pressure, 0.75);
    assert_eq!(driven.stream.brightness, 0.85);
    assert_all_finite(&neutral_left);
    assert_all_finite(&driven_left);
    assert!(
        mean_abs_difference(&neutral_left, &driven_left) > rms(&neutral_left) * 0.05,
        "neutral_rms={}, driven_rms={}, diff={}",
        rms(&neutral_left),
        rms(&driven_left),
        mean_abs_difference(&neutral_left, &driven_left)
    );
}

#[test]
fn audio_analysis_expression_source_drives_lamath_from_shared_phrase_result() {
    let sample_rate = 48_000.0;
    let pitch_hz = midi_note_to_hz(62.0);
    let audio = audio_expression_sine(pitch_hz, 0.3, 2_048);
    let analysis = audio_expression_phrase_result(pitch_hz, 0.2, audio.len());
    let mut source = crate::AudioAnalysisExpressionSource::<MIDI_EXPRESSION_VOICES>::new(
        &audio,
        sample_rate as u32,
        &analysis,
        crate::AudioExpressionMapping {
            pitch_bend_range_semitones: 12.0,
            pressure_floor_rms: 0.0,
            pressure_ceiling_rms: 0.4,
            brightness_floor_hz: 100.0,
            brightness_ceiling_hz: 8_000.0,
        },
    );
    source.set_block(0, 512);

    let mut processor =
        ResonatorProcessor::with_builtin_excitation(sample_rate, external_expression_filter_patch());
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 0.9,
    })];
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];

    processor.process_with_expression_source(&mut source, &note_on, &mut left, &mut right);

    let expression = expression_for_slot(&processor, 0, 60);
    assert!(expression.stream.gate);
    assert!((expression.stream.pitch_bend - 2.0).abs() < 0.05);
    assert_eq!(expression.stream.velocity, 0.9);
    assert!(expression.stream.pressure > 0.4);
    assert!(expression.stream.brightness > 0.0);
    assert_all_finite(&left);
    assert_all_finite(&right);
}

fn audio_expression_phrase_result(pitch_hz: f32, rms: f32, len: usize) -> PhraseAnalysisResult {
    let note = DetectedNote {
        start_sample: 0,
        end_sample: len,
        pitch_hz,
        peak_rms: rms,
        mean_rms: rms,
    };
    PhraseAnalysisResult {
        pitch_contour: PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                audio_expression_pitch_frame(0, 0, pitch_hz, rms),
                audio_expression_pitch_frame(1, 768, pitch_hz, rms),
                audio_expression_pitch_frame(2, 1_536, pitch_hz, rms),
            ],
        },
        markers: Vec::new(),
        segmented_notes: vec![SegmentedNote {
            note,
            inherited_pitch: false,
        }],
        detected_notes: vec![note],
    }
}

fn audio_expression_pitch_frame(
    frame_index: usize,
    source_sample_position: usize,
    pitch_hz: f32,
    rms: f32,
) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / 48_000.0,
        f0_hz: Some(pitch_hz),
        raw_f0_hz: pitch_hz,
        confidence: 0.95,
        voiced: true,
        rms,
    }
}

fn audio_expression_sine(frequency_hz: f32, amplitude: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / 48_000.0;
            phase.sin() * amplitude
        })
        .collect()
}
