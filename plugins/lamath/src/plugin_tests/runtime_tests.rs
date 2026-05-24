#[test]
fn repeated_clip_loop_note_offs_remain_bounded_and_decay_after_stop() {
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
    let mut start_tail = Vec::new();
    let mut end_tail = Vec::new();

    synth.reset(setup);
    for loop_index in 0..4 {
        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &[MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note: 48 + loop_index,
                velocity: 100.0 / 127.0,
            })],
        );
        assert!(peak_abs(&block_left).max(peak_abs(&block_right)) < 8.0);

        for _ in 0..24 {
            process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
            assert_all_finite(&block_left);
            assert_all_finite(&block_right);
        }

        process_block(
            &mut synth,
            setup,
            &mut block_left,
            &mut block_right,
            &[MidiEvent::Note(NoteEvent::Off {
                channel: 0,
                note: 48 + loop_index,
                velocity: 0.0,
            })],
        );
    }

    for tail_block in 0..800 {
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &[]);
        assert_all_finite(&block_left);
        assert_all_finite(&block_right);
        if tail_block < 64 {
            start_tail.extend_from_slice(&block_left);
        } else if tail_block >= 736 {
            end_tail.extend_from_slice(&block_left);
        }
    }

    assert!(
        rms(&end_tail) < rms(&start_tail) * 0.1,
        "output should decay after repeated note-offs"
    );
}

#[test]
fn audio_plugin_process_does_not_allocate() {
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
    assert_audio_plugin_process_block_does_not_allocate(
        "audio plugin process",
        &mut synth,
        setup,
        &mut left,
        &mut right,
        &events,
        AudioInputBuffer::empty(),
    );
}

fn realtime_process_setup(block_size: usize) -> ProcessSetup {
    ProcessSetup {
        sample_rate: 48_000.0,
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    }
}

#[allow(clippy::too_many_arguments)]
fn assert_audio_plugin_process_block_does_not_allocate(
    label: &str,
    synth: &mut ResonatorSynth,
    setup: ProcessSetup,
    left: &mut [f32],
    right: &mut [f32],
    events: &[MidiEvent],
    input: AudioInputBuffer<'_>,
) {
    assert_no_allocations(label, || {
        synth.process(
            ProcessContext::new(setup, AudioBuffer { left, right }, events).with_input(input),
        );
    });
    assert_all_finite(left);
    assert_all_finite(right);
}
