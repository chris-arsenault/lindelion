#[test]
fn audio_plugin_sidechain_note_on_latency_is_bounded_with_small_host_blocks() {
    const BLOCK_SIZE: usize = 128;
    const SILENCE_BLOCKS: usize = 24;
    const TONE_BLOCKS: usize = 24;
    const ONSET_SAMPLE: usize = BLOCK_SIZE * SILENCE_BLOCKS;
    const MAX_LATENCY_SAMPLES: usize = 512;

    let setup = realtime_process_setup(BLOCK_SIZE);
    let mut synth = ResonatorSynth::default();
    let mut patch = waveguide_tail_test_patch();
    patch.audio_input.mode = AudioInputMode::AudioCreatesNotes;
    let sidechain =
        sidechain_sine_note_after_silence(84.0, 0.8, ONSET_SAMPLE, BLOCK_SIZE * TONE_BLOCKS);
    let mut rendered = Vec::with_capacity(sidechain.len());
    let mut left = vec![0.0; BLOCK_SIZE];
    let mut right = vec![0.0; BLOCK_SIZE];

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    for block in sidechain.chunks_exact(BLOCK_SIZE) {
        synth.process(
            ProcessContext::new(setup, AudioBuffer { left: &mut left, right: &mut right }, &[])
                .with_input(AudioInputBuffer::mono(block)),
        );
        rendered.extend_from_slice(&left);
    }

    let latency_samples = first_sample_above(&rendered[ONSET_SAMPLE..], 0.000_001)
        .expect("audio-created note should become audible after projected sidechain input");
    assert!(
        latency_samples <= MAX_LATENCY_SAMPLES,
        "plugin sidechain note-on latency should stay within {MAX_LATENCY_SAMPLES} samples, got {latency_samples}"
    );
    assert!(synth.telemetry().sidechain.note_detected);
    assert_all_finite(&rendered);
}

#[test]
#[ignore = "host-dependent performance probe; run in release mode when updating docs"]
fn audio_plugin_v2_all_enabled_cpu_probe() {
    const BLOCK_SIZE: usize = 128;
    const RENDER_SECONDS: usize = 10;
    const SILENCE_BLOCKS: usize = 24;
    const ONSET_SAMPLE: usize = BLOCK_SIZE * SILENCE_BLOCKS;

    let sample_rate = 48_000.0;
    let setup = realtime_process_setup(BLOCK_SIZE);
    let total_samples = sample_rate as usize * RENDER_SECONDS;
    let tone_samples = total_samples - ONSET_SAMPLE;
    let sidechain = sidechain_sine_note_after_silence(84.0, 0.8, ONSET_SAMPLE, tone_samples);
    let mut synth = ResonatorSynth::default();
    let mut patch = waveguide_tail_test_patch();
    patch.polyphony = 4;
    patch.audio_input.mode = AudioInputMode::MidiPlusAudioCreatesNotes;
    patch.audio_expression.enabled = true;
    patch.audio_expression.mapping.pressure_floor_rms = 0.0;
    patch.audio_expression.mapping.pressure_ceiling_rms = 0.5;
    patch.live_excitation.mode = LiveExcitationMode::ContinuousAndNoteLatched;
    patch.live_excitation.gain_db = -6.0;
    let midi_note = [MidiEvent::Note(NoteEvent::On {
        channel: 1,
        note: 60,
        velocity: 0.8,
    })];
    let midi_note_block = SILENCE_BLOCKS + 40;
    let mut left = vec![0.0; BLOCK_SIZE];
    let mut right = vec![0.0; BLOCK_SIZE];
    let mut first_audible_sample = None;

    synth.reset(setup);
    synth.set_patch_for_test(patch);
    let start = std::time::Instant::now();
    for (block_index, block) in sidechain.chunks_exact(BLOCK_SIZE).enumerate() {
        let events = if block_index == midi_note_block {
            &midi_note[..]
        } else {
            &[]
        };
        synth.process(
            ProcessContext::new(setup, AudioBuffer { left: &mut left, right: &mut right }, events)
                .with_input(AudioInputBuffer::mono(block)),
        );
        if first_audible_sample.is_none() {
            first_audible_sample = first_sample_above(&left, 0.000_001)
                .or_else(|| first_sample_above(&right, 0.000_001))
                .map(|offset| block_index * BLOCK_SIZE + offset);
        }
        std::hint::black_box(&left);
        std::hint::black_box(&right);
    }
    let elapsed = start.elapsed();
    let rendered_seconds = sidechain.len() as f64 / sample_rate;
    let realtime_ratio = elapsed.as_secs_f64() / rendered_seconds;
    let note_on_latency_samples = first_audible_sample
        .expect("sidechain note should become audible during the performance probe")
        .saturating_sub(ONSET_SAMPLE);
    let note_on_latency_ms = note_on_latency_samples as f64 * 1_000.0 / sample_rate;

    println!(
        "lamath_v2_all_enabled_cpu_probe sample_rate={sample_rate} block_size={BLOCK_SIZE} sidechain_note_latency_samples={note_on_latency_samples} sidechain_note_latency_ms={note_on_latency_ms:.3} rendered_seconds={rendered_seconds:.3} elapsed_ms={:.3} realtime_ratio={realtime_ratio:.5}",
        elapsed.as_secs_f64() * 1_000.0
    );
    assert!(synth.telemetry().active_voices >= 2);
    assert!(synth.telemetry().sidechain.note_detected);
    assert_all_finite(&left);
    assert_all_finite(&right);
}
