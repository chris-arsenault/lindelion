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
fn sympathetic_chamber_adds_a_resonant_tail_through_the_runtime() {
    // End-to-end (M10, ADR-0025): the runtime tunes the shared chamber to the sounding
    // voice's pitch and runs it as a send/return over the mix. A short note excites the
    // chamber, which rings on after the direct sound has decayed — so a late block
    // carries measurably more energy with the sympathetic depth engaged than defeated.
    let make = |sympathetic: f32| {
        let mut patch = test_patch();
        patch.resonator_a = ResonatorConfig::Modal(ModalConfig {
            mode_count: 12,
            preset: ModalPreset::GenericStrike,
            decay_global: 0.4,
            ..ModalConfig::default()
        });
        patch.output = OutputConfig {
            filter_cutoff: 20_000.0,
            master_gain_db: 0.0,
            ..OutputConfig::default()
        };
        // A fast amp release so that, once the note is released, the direct sound dies
        // quickly and the only remaining late energy is the sympathetic ring.
        patch.modulation.amp_envelope.release_ms = 4.0;
        patch.surrounding.sympathetic = sympathetic;
        patch
    };
    let render_post_release_tail = |patch: ResonatorSynthPatch| -> f32 {
        let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, patch);
        let mut left = vec![0.0; 8_192];
        let mut right = vec![0.0; 8_192];
        let note_on = MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 55,
            velocity: 1.0,
        });
        let note_off = MidiEvent::Note(NoteEvent::Off {
            channel: 0,
            note: 55,
            velocity: 0.0,
        });
        // Hold the note for two blocks so the sustained tone rings the chamber up.
        processor.process(&[note_on], &mut left, &mut right);
        processor.process(&[], &mut left, &mut right);
        // Release: the fast amp release kills the direct sound within a few ms.
        processor.process(&[note_off], &mut left, &mut right);
        // This block is entirely post-release: direct gone, only the sympathetic ring.
        processor.process(&[], &mut left, &mut right);
        assert_all_finite(&left);
        assert_all_finite(&right);
        rms(&left) + rms(&right)
    };
    let dry = render_post_release_tail(make(0.0));
    let wet = render_post_release_tail(make(0.9));
    assert!(
        wet > dry * 4.0 && wet > 1.0e-4,
        "sympathetic should add a cross-voice tail through the runtime: dry={dry} wet={wet}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn processor_audio_path_does_not_allocate() {
    let mut processor = ResonatorProcessor::with_builtin_excitation(48_000.0, test_patch());
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let events = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 1.0,
    })];

    assert_runtime_process_does_not_allocate(
        "processor process note-on",
        &mut processor,
        &events,
        &mut left,
        &mut right,
    );
    assert_runtime_process_does_not_allocate(
        "processor process render-only",
        &mut processor,
        &[],
        &mut left,
        &mut right,
    );
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
    assert_runtime_process_does_not_allocate(
        "processor process controls",
        &mut processor,
        &controls,
        &mut left,
        &mut right,
    );

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
