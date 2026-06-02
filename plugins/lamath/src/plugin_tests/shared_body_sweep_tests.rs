// Heavy shared-body fidelity/stability sweeps (ADR-0031, M8). Each renders multi-second
// clips through the full ResonatorSynth with the shared-body idiophone mode enabled, and
// asserts the objective bounds (finite, bounded, audible, sustaining, monotonic dynamics,
// continuity, damp). Gated behind the `integration-tests` feature (run via
// `make test-integration`) so they stay out of the fast `make ci` unit path. Included into
// the `plugin_tests` module, so the render helpers resolve from the shared module scope.

fn shared_body_patch(mesh: bool) -> ResonatorSynthPatch {
    let mut patch = if mesh {
        ResonatorSynthPatch {
            resonator_a: ResonatorConfig::Mesh(MeshConfig::default()),
            ..ResonatorSynthPatch::default()
        }
    } else {
        ResonatorSynthPatch::default()
    };
    // `shared_body` / `surrounding` are nested fields (no `field_reassign_with_default`).
    patch.shared_body.enabled = true;
    // Isolate the body from the cross-voice sympathetic chamber for these measurements.
    patch.surrounding.sympathetic = 0.0;
    patch
}

fn strike_event(note: u8) -> Vec<MidiEvent> {
    vec![MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity: 100.0 / 127.0,
    })]
}

fn render_shared_body(
    patch: ResonatorSynthPatch,
    sample_rate: f32,
    block_size: usize,
    total_blocks: usize,
    mut events_for_block: impl FnMut(usize) -> Vec<MidiEvent>,
) -> RenderedClip {
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_for_test(patch);

    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity(total_blocks * block_size);
    let mut right = Vec::with_capacity(total_blocks * block_size);

    for block in 0..total_blocks {
        let events = events_for_block(block);
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &events);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    let rms = rms(&left).max(rms(&right));
    let peak = peak_abs(&left).max(peak_abs(&right));
    RenderedClip {
        left,
        right,
        rms,
        peak,
    }
}

/// M8: stability under dense overlapping strikes — across sample rates and block sizes the
/// struck body stays finite, bounded, and audible (no runaway).
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn shared_body_dense_strikes_render_inside_bounds() {
    let notes = [48u8, 55, 60, 64, 67, 72];
    for sample_rate in [44_100.0_f32, 48_000.0, 96_000.0] {
        for block_size in [32usize, 128, 512] {
            let total_blocks = ((sample_rate * 4.0).ceil() as usize).div_ceil(block_size);
            let strike_every = (0.02 * sample_rate / block_size as f32).max(1.0) as usize;
            let rendered = render_shared_body(
                shared_body_patch(false),
                sample_rate,
                block_size,
                total_blocks,
                |block| {
                    if block % strike_every == 0 {
                        strike_event(notes[(block / strike_every) % notes.len()])
                    } else {
                        Vec::new()
                    }
                },
            );
            assert_all_finite(&rendered.left);
            assert_all_finite(&rendered.right);
            assert!(
                rendered.rms > 0.000_001,
                "dense strikes should be audible at sr={sample_rate} block={block_size}",
            );
            assert!(
                rendered.peak < 8.0,
                "dense strikes must stay bounded at sr={sample_rate} block={block_size}, peak={}",
                rendered.peak,
            );
        }
    }
}

/// M8: multi-second ring length — a single strike still rings well into a later window.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn shared_body_ring_sustains_multiple_seconds() {
    let sample_rate = 48_000.0_f32;
    let block_size = 512;
    let total_blocks = ((sample_rate * 3.0).ceil() as usize).div_ceil(block_size);
    let rendered = render_shared_body(
        shared_body_patch(false),
        sample_rate,
        block_size,
        total_blocks,
        |block| {
            if block == 0 {
                strike_event(60)
            } else {
                Vec::new()
            }
        },
    );

    assert_all_finite(&rendered.left);
    let tail_start = rendered
        .left
        .len()
        .saturating_sub((sample_rate * 0.5) as usize);
    assert!(
        peak_abs(&rendered.left[tail_start..]) > 0.0,
        "the body ring must sustain into a later window",
    );
}

/// M8: dynamics respond monotonically end-to-end — a harder strike is louder.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn shared_body_dynamics_respond_end_to_end() {
    let render_rms = |velocity: f32| {
        let sample_rate = 48_000.0_f32;
        let block_size = 512;
        let total_blocks = ((sample_rate * 1.5).ceil() as usize).div_ceil(block_size);
        render_shared_body(
            shared_body_patch(false),
            sample_rate,
            block_size,
            total_blocks,
            move |block| {
                if block == 0 {
                    vec![MidiEvent::Note(NoteEvent::On {
                        channel: 0,
                        note: 60,
                        velocity,
                    })]
                } else {
                    Vec::new()
                }
            },
        )
        .rms
    };

    let soft = render_rms(0.2);
    let hard = render_rms(1.0);
    assert!(soft > 0.0, "a soft strike must still ring");
    assert!(
        hard > soft,
        "a harder strike must be louder end-to-end: hard {hard} soft {soft}",
    );
}

/// M8: retune-while-ringing continuity — a melodic strike sequence stays finite and
/// bounded, and retuning the live body injects no click (no large sample-to-sample jump).
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn shared_body_retune_sequence_stays_bounded_and_continuous() {
    let sample_rate = 48_000.0_f32;
    let block_size = 256;
    let total_blocks = ((sample_rate * 3.0).ceil() as usize).div_ceil(block_size);
    let notes = [60u8, 62, 64, 65, 67, 69, 71, 72];
    let strike_every = (0.15 * sample_rate / block_size as f32).max(1.0) as usize;
    let rendered = render_shared_body(
        shared_body_patch(false),
        sample_rate,
        block_size,
        total_blocks,
        |block| {
            if block % strike_every == 0 {
                strike_event(notes[(block / strike_every) % notes.len()])
            } else {
                Vec::new()
            }
        },
    );

    assert_all_finite(&rendered.left);
    assert!(
        rendered.peak < 8.0,
        "retune sequence must stay bounded: peak {}",
        rendered.peak,
    );
    let max_delta = max_adjacent_delta(&rendered.left);
    assert!(
        max_delta < 1.0,
        "retuning a ringing body must not inject a click: max adjacent delta {max_delta}",
    );
}

/// M8: damp-ramp through the full processor — a key-switch damp note silences the body
/// within the ramp, leaving a silent tail.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn shared_body_damp_silences_through_the_processor() {
    let sample_rate = 48_000.0_f32;
    let block_size = 256;
    let total_blocks = ((sample_rate * 1.5).ceil() as usize).div_ceil(block_size);
    let damp_block = (0.5 * sample_rate / block_size as f32) as usize;
    let rendered = render_shared_body(
        shared_body_patch(false),
        sample_rate,
        block_size,
        total_blocks,
        |block| {
            if block == 0 {
                strike_event(60) // out of the default damp range [0, 11] -> strikes
            } else if block == damp_block {
                strike_event(5) // inside the damp range -> damps
            } else {
                Vec::new()
            }
        },
    );

    assert_all_finite(&rendered.left);
    let tail_start = rendered
        .left
        .len()
        .saturating_sub((sample_rate * 0.2) as usize);
    assert_eq!(
        peak_abs(&rendered.left[tail_start..]),
        0.0,
        "the damp key must silence the body within the ramp through the full processor",
    );
}
