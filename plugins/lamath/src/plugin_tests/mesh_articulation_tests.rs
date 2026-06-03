// Mesh-as-shared-body-idiophone articulation tests (ADR-0031). The mesh resonator is a
// single persistent re-strikable body, so its musically interesting behavior lives in how
// it is *played*: a melodic line retunes the one body ring-preserving, overlapping strikes
// move it to the latest pitch, rapid restrikes re-excite without runaway, and — unlike the
// per-voice families — it never allocates polyphony. The existing shared-body sweeps
// (`shared_body_sweep_tests.rs`) exercise the Modal body; these cover the **Mesh** body and
// add the pitch-tracking / single-voice assertions those sweeps don't make. Multi-second
// full-synth renders, so gated behind `integration-tests` (run via `make test-integration`).
// Included into `plugin_tests`, so `shared_body_patch`, `render_shared_body`, `strike_event`,
// and the render helpers resolve from the shared module scope.

use lindelion_dsp_utils::analysis::spectral_centroid_hz;

/// One strike every `seconds`, expressed in render blocks (≥ 1).
fn strike_interval_blocks(seconds: f32, sample_rate: f32, block_size: usize) -> usize {
    ((seconds * sample_rate / block_size as f32) as usize).max(1)
}

/// Spectral centroid (Hz) of a `seconds`-long window starting at `start_sample`. The mesh
/// is inharmonic, so the centroid — not an absolute fundamental — is the robust pitch proxy:
/// steering the body higher raises every mode, so the centroid rises with the played note.
fn window_centroid(samples: &[f32], start_sample: usize, seconds: f32, sample_rate: f32) -> f32 {
    let len = (seconds * sample_rate) as usize;
    let end = (start_sample + len).min(samples.len());
    let start = start_sample.min(end);
    spectral_centroid_hz(&samples[start..end], sample_rate).unwrap_or(0.0)
}

/// Articulation 1 — a C-major scale on the single body. Each note retunes the persistent
/// body (ring-preserving), so an ascending line raises the body's pitch: the early-window
/// spectral centroid climbs monotonically from C4 to C5, every note is audible, and the
/// whole line stays finite, bounded, and click-free.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn mesh_idiophone_plays_c_major_scale_with_rising_pitch() {
    let sample_rate = 48_000.0_f32;
    let block_size = 256;
    let scale = [60u8, 62, 64, 65, 67, 69, 71, 72]; // C4 … C5
    let strike_every = strike_interval_blocks(0.28, sample_rate, block_size);
    let total_blocks = strike_every * (scale.len() + 1);

    let rendered = render_shared_body(
        shared_body_patch(true),
        sample_rate,
        block_size,
        total_blocks,
        |block| {
            if block % strike_every == 0 && block / strike_every < scale.len() {
                strike_event(scale[block / strike_every])
            } else {
                Vec::new()
            }
        },
    );

    assert_all_finite(&rendered.left);
    assert!(rendered.peak < 8.0, "scale must stay bounded: peak {}", rendered.peak);
    assert!(
        max_adjacent_delta(&rendered.left) < 1.0,
        "retuning the body per scale step must not click"
    );

    // Measure the early window of C4 (first), G4 (middle), C5 (last) — same elapsed time
    // after each strike, so the decay state is comparable. The centroid must rise.
    let note_start = |index: usize| index * strike_every * block_size;
    let centroid_at = |index: usize| {
        window_centroid(&rendered.left, note_start(index), 0.12, sample_rate)
    };
    let c4 = centroid_at(0);
    let g4 = centroid_at(4);
    let c5 = centroid_at(7);
    assert!(c4 > 0.0 && g4 > 0.0 && c5 > 0.0, "each scale note must be audible: {c4}, {g4}, {c5}");
    assert!(
        c4 < g4 && g4 < c5,
        "ascending scale must raise the body's pitch (centroid): C4 {c4}, G4 {g4}, C5 {c5}"
    );
    // An octave of steering should move the centroid substantially.
    assert!(
        c5 > c4 * 1.4,
        "C4→C5 should clearly raise the centroid: C4 {c4}, C5 {c5}"
    );
}

/// Articulation 2 — note overlap (legato). A high strike landing while a low note still rings
/// retunes the live body up to the latest pitch (ring-preserving, no choke), continuously:
/// the post-overlap centroid is higher than the low-note-only centroid, with no click.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn mesh_idiophone_overlapping_strike_retunes_to_latest_pitch() {
    let sample_rate = 48_000.0_f32;
    let block_size = 256;
    let overlap_block = strike_interval_blocks(0.12, sample_rate, block_size);
    let total_blocks = strike_interval_blocks(1.4, sample_rate, block_size);

    let rendered = render_shared_body(
        shared_body_patch(true),
        sample_rate,
        block_size,
        total_blocks,
        |block| {
            if block == 0 {
                strike_event(48) // C3 — low note
            } else if block == overlap_block {
                strike_event(72) // C5 — overlaps while C3 still rings
            } else {
                Vec::new()
            }
        },
    );

    assert_all_finite(&rendered.left);
    assert!(rendered.peak < 8.0, "overlap must stay bounded: peak {}", rendered.peak);
    assert!(
        max_adjacent_delta(&rendered.left) < 1.0,
        "retuning a ringing body on overlap must not click"
    );

    // Low-note-only window (before the overlap) vs the window right after the overlap.
    let low_only = window_centroid(&rendered.left, 0, 0.10, sample_rate);
    let after_overlap =
        window_centroid(&rendered.left, overlap_block * block_size, 0.10, sample_rate);
    assert!(low_only > 0.0, "the low note must ring before the overlap: {low_only}");
    assert!(
        after_overlap > low_only * 1.4,
        "overlapping a higher note must pull the body up: low-only {low_only}, after {after_overlap}"
    );
}

/// Articulation 3 — rapid restrike of one note. Each restrike re-excites the persistent body,
/// but the energy must reach a bounded steady state (not accumulate into a runaway), and once
/// the strikes stop the body decays rather than self-oscillating.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn mesh_idiophone_rapid_restrike_is_bounded_and_decays() {
    let sample_rate = 48_000.0_f32;
    let block_size = 256;
    let restrike_every = strike_interval_blocks(0.03, sample_rate, block_size); // ~33 strikes/s
    let burst_blocks = strike_interval_blocks(1.5, sample_rate, block_size);
    let total_blocks = strike_interval_blocks(2.8, sample_rate, block_size); // burst + tail

    let rendered = render_shared_body(
        shared_body_patch(true),
        sample_rate,
        block_size,
        total_blocks,
        |block| {
            if block < burst_blocks && block % restrike_every == 0 {
                strike_event(60)
            } else {
                Vec::new()
            }
        },
    );

    assert_all_finite(&rendered.left);
    assert!(
        rendered.peak < 8.0,
        "rapid restrike must stay bounded, not run away: peak {}",
        rendered.peak
    );

    let window = |from: f32, to: f32| {
        let a = (from * sample_rate) as usize;
        let b = ((to * sample_rate) as usize).min(rendered.left.len());
        rms(&rendered.left[a.min(b)..b])
    };
    let early_burst = window(0.2, 0.4);
    let late_burst = window(1.2, 1.4);
    let tail = window(2.4, 2.7); // well after the last strike

    assert!(early_burst > 0.000_1, "the restrike burst must be audible: {early_burst}");
    // Steady state, not runaway: late energy stays within a bounded multiple of early.
    assert!(
        late_burst < early_burst * 4.0,
        "restrike energy must reach a bounded steady state, not accumulate: early {early_burst}, late {late_burst}"
    );
    // After the strikes stop the body decays (it is struck, not self-oscillating).
    assert!(
        tail < late_burst * 0.5,
        "the body must decay once restrikes stop: late {late_burst}, tail {tail}"
    );
}

/// Articulation 4 — single voice, not polyphony. In shared-body mode the mesh is one
/// persistent body: striking a dense, chord-like burst of distinct notes allocates **zero**
/// per-voice voices (the note-ons strike the shared body instead) while still producing
/// audible output. This is the defining single-voice contrast with the per-voice families.
#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn mesh_idiophone_is_a_single_voice_not_polyphony() {
    let sample_rate = 48_000.0_f32;
    let block_size = 256;
    let total_blocks = strike_interval_blocks(1.5, sample_rate, block_size);
    let strike_every = strike_interval_blocks(0.05, sample_rate, block_size);
    let notes = [60u8, 64, 67, 72, 55, 48]; // a dense chord-like spread

    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_for_test(shared_body_patch(true));

    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut all_left = Vec::with_capacity(total_blocks * block_size);
    let mut max_active_voices = 0usize;

    for block in 0..total_blocks {
        let events = if block % strike_every == 0 && block / strike_every < notes.len() {
            strike_event(notes[block / strike_every])
        } else {
            Vec::new()
        };
        process_block(&mut synth, setup, &mut block_left, &mut block_right, &events);
        max_active_voices = max_active_voices.max(synth.telemetry().active_voices);
        all_left.extend_from_slice(&block_left);
    }

    assert_all_finite(&all_left);
    assert_eq!(
        max_active_voices, 0,
        "the shared-body mesh must allocate no per-voice polyphony (it is one persistent body)"
    );
    assert!(
        rms(&all_left) > 0.000_1,
        "the single shared body must still sound when struck: rms {}",
        rms(&all_left)
    );
}
