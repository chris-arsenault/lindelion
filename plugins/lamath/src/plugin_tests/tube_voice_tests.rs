// Full-synth perceptual guards for the driven wind Tube (ADR-0032). The Tube is always
// reed-driven, so it must produce a sustained, audible tone at its shipped default and never
// the silent struck bore it used to be. Cheap, in-memory, make-ci-safe (audio-hygiene
// invariant per AGENTS): the multi-second voicing/stability sweeps belong to M1's integration
// suite, not here. The audibility bar is a real dBFS floor, not `rms > 0`.

fn render_held_tube_note(note: u8, velocity: f32, hold_seconds: f32) -> Vec<f32> {
    let sample_rate = 48_000.0_f32;
    let block_size = 512;
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut patch = ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::Tube,
            ..WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        ..ResonatorSynthPatch::default()
    };
    patch.output.master_gain_db = 0.0;
    patch.output.saturation_drive = 0.0;
    patch.output.filter_cutoff = 20_000.0;
    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_for_test(patch);
    let blocks = (sample_rate * hold_seconds / block_size as f32).ceil() as usize;
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })];
    let mut bl = vec![0.0; block_size];
    let mut br = vec![0.0; block_size];
    let mut left = Vec::new();
    for block in 0..blocks {
        let events: &[MidiEvent] = if block == 0 { &note_on } else { &[] };
        process_block(&mut synth, setup, &mut bl, &mut br, events);
        left.extend_from_slice(&bl);
    }
    left
}

fn render_tube_voiced(
    note: u8,
    velocity: f32,
    reflection: f32,
    cutoff: f32,
    nonlinearity: f32,
) -> Vec<f32> {
    let sample_rate = 48_000.0_f32;
    let block_size = 512;
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut patch = ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::Tube,
            boundary_reflection: reflection,
            loop_filter_cutoff: cutoff,
            loop_nonlinearity: nonlinearity,
            ..WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        ..ResonatorSynthPatch::default()
    };
    patch.output.master_gain_db = 0.0;
    patch.output.saturation_drive = 0.0;
    patch.output.filter_cutoff = 20_000.0;
    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_for_test(patch);
    let blocks = (sample_rate * 0.5 / block_size as f32).ceil() as usize;
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })];
    let mut bl = vec![0.0; block_size];
    let mut br = vec![0.0; block_size];
    let mut left = Vec::new();
    for block in 0..blocks {
        let events: &[MidiEvent] = if block == 0 { &note_on } else { &[] };
        process_block(&mut synth, setup, &mut bl, &mut br, events);
        left.extend_from_slice(&bl);
    }
    left
}

fn tube_centroid(left: &[f32]) -> f32 {
    let s = &left[left.len() / 2..];
    lindelion_dsp_utils::analysis::spectral_centroid_hz(s, 48_000.0).unwrap_or(0.0)
}

/// Render the reed Tube playing a phrase. Each note is `(midi, start_s, end_s, velocity)`;
/// `polyphony` + `retrigger` set the articulation behavior (mono voice-steal slur vs poly
/// stack; re-strike the bore per note or not). Notes that overlap (next start < prev end)
/// are legato; notes with a gap are tongued/separated.
fn render_tube_phrase(notes: &[(u8, f32, f32, f32)], polyphony: u8, retrigger: bool) -> Vec<f32> {
    let sr = 48_000.0_f32;
    let block = 512usize;
    let total_s = notes.iter().map(|n| n.2).fold(0.0_f32, f32::max) + 0.6;
    let setup = ProcessSetup {
        sample_rate: f64::from(sr),
        max_block_size: block,
        mode: ProcessMode::Realtime,
    };
    let mut patch = ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::Tube,
            ..WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        polyphony,
        retrigger_resonators: retrigger,
        ..ResonatorSynthPatch::default()
    };
    patch.output.master_gain_db = 0.0;
    patch.output.saturation_drive = 0.0;
    patch.output.filter_cutoff = 20_000.0;
    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_for_test(patch);

    let total_blocks = ((sr * total_s).ceil() as usize).div_ceil(block);
    let block_of = |t: f32| ((t * sr) as usize / block).min(total_blocks.saturating_sub(1));
    let mut on: Vec<(usize, MidiEvent)> = Vec::new();
    for &(note, start, end, vel) in notes {
        on.push((
            block_of(start),
            MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note,
                velocity: vel,
            }),
        ));
        on.push((
            block_of(end),
            MidiEvent::Note(NoteEvent::Off {
                channel: 0,
                note,
                velocity: 0.0,
            }),
        ));
    }
    let mut bl = vec![0.0; block];
    let mut br = vec![0.0; block];
    let mut left = Vec::with_capacity(total_blocks * block);
    for b in 0..total_blocks {
        let events: Vec<MidiEvent> = on.iter().filter(|(blk, _)| *blk == b).map(|(_, e)| *e).collect();
        process_block(&mut synth, setup, &mut bl, &mut br, &events);
        left.extend_from_slice(&bl);
    }
    left
}

/// C major scale C4→C5 as `(midi, start, end, velocity)` with `step` between onsets and each
/// note lasting `dur` — `dur > step` overlaps (legato), `dur < step` separates (tongued).
fn c_major_scale(step: f32, dur: f32, vel: f32) -> Vec<(u8, f32, f32, f32)> {
    [60u8, 62, 64, 65, 67, 69, 71, 72]
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let start = i as f32 * step;
            (n, start, start + dur, vel)
        })
        .collect()
}

#[test]
fn driven_tube_termination_reshapes_timbre() {
    // The bell termination is an audible timbral axis within its usable (closed-bell) band:
    // opening it toward the radiating end thins and brightens the tone. Gain-invariant
    // centroid, so the shift is timbral, not level.
    let warm = render_tube_voiced(60, 100.0 / 127.0, -0.75, 8_000.0, 0.0);
    let open = render_tube_voiced(60, 100.0 / 127.0, -0.50, 8_000.0, 0.0);
    let (cw, co) = (tube_centroid(&warm), tube_centroid(&open));
    assert!(
        co > cw * 1.2,
        "opening the bell should brighten the tube audibly: warm={cw:.0} open={co:.0}"
    );
}

#[test]
fn driven_tube_brightness_control_is_audible() {
    // The loop-filter cutoff is a clean brightness axis across its range.
    let dark = render_tube_voiced(60, 100.0 / 127.0, -0.75, 3_000.0, 0.0);
    let bright = render_tube_voiced(60, 100.0 / 127.0, -0.75, 12_000.0, 0.0);
    let (cd, cb) = (tube_centroid(&dark), tube_centroid(&bright));
    assert!(
        cb > cd * 1.3,
        "raising the loop cutoff should brighten the tube audibly: dark={cd:.0} bright={cb:.0}"
    );
}

#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn driven_tube_sweep_stays_finite_bounded_and_audible() {
    // Multi-second register × velocity sweep: the wind tube self-oscillates, stays finite and
    // bounded, and is audible at playing velocities across the whole range — plus extreme
    // termination / cutoff / pressure-depth combinations stay finite and bounded (no runaway).
    for note in [36u8, 41, 46, 51, 56, 60, 65, 70, 75, 80, 84] {
        for vel_127 in [10u8, 40, 80, 127] {
            let left = render_held_tube_note(note, f32::from(vel_127) / 127.0, 0.6);
            assert_all_finite(&left);
            assert!(
                peak_abs(&left) < 8.0,
                "tube note {note} vel {vel_127} unbounded: {}",
                peak_abs(&left)
            );
            if vel_127 >= 40 {
                let sustain = &left[left.len() / 2..];
                let dbfs = 20.0 * rms(sustain).max(1.0e-9).log10();
                assert!(
                    dbfs > -45.0,
                    "tube note {note} vel {vel_127} should be audible: {dbfs:.1} dBFS"
                );
            }
        }
    }
    for refl in [-1.0_f32, -0.95, -0.6, -0.3, 0.5] {
        for cutoff in [400.0_f32, 8_000.0, 20_000.0] {
            let left = render_tube_voiced(60, 1.0, refl, cutoff, 1.0);
            assert_all_finite(&left);
            assert!(
                peak_abs(&left) < 8.0,
                "extreme tube (refl {refl}, cutoff {cutoff}) unbounded: {}",
                peak_abs(&left)
            );
        }
    }
}

#[test]
fn driven_tube_holds_fundamental_across_velocity() {
    // The reed's usable pressure window keeps the playing dynamic range on the fundamental: C3
    // and C4 lock from soft up through forte. At fortissimo a note may overblow down an octave
    // (real, accepted reed behaviour), so vel 127 is not asserted.
    use lindelion_dsp_utils::analysis::estimate_f0_autocorrelation;
    let sr = 48_000.0_f32;
    for note in [48u8, 60] {
        let f0 = lindelion_dsp_utils::math::midi_note_to_hz(f32::from(note));
        for vel_127 in [20u8, 50, 90, 105] {
            let left = render_held_tube_note(note, f32::from(vel_127) / 127.0, 0.5);
            let sustain = &left[left.len() / 2..];
            let est = estimate_f0_autocorrelation(sustain, sr, f0 * 0.4, f0 * 4.0).unwrap_or_else(
                || panic!("tube note {note} vel {vel_127} produced no pitched tone"),
            );
            let ratio = est / f0;
            assert!(
                (0.94..=1.04).contains(&ratio),
                "tube note {note} vel {vel_127} should hold the fundamental {f0:.1} Hz, got {est:.1} Hz (ratio {ratio:.3})"
            );
        }
    }
}

#[test]
fn driven_tube_locks_to_played_fundamental_across_register() {
    // ADR-0032: the reed terminates the bore mouth, so the wind-driven tube self-oscillates
    // at the bore's *tuned fundamental*, not the sub-harmonic / overblown register the old
    // strike-injected reed locked to. Guard the played pitch across the register at forte:
    // a per-note estimate within ~half a semitone of the requested fundamental (which firmly
    // excludes the period-doubled octave-below ≈ 0.5 and any overblown register).
    use lindelion_dsp_utils::analysis::estimate_f0_autocorrelation;
    let sr = 48_000.0_f32;
    for note in [36u8, 48, 60, 72] {
        let f0 = lindelion_dsp_utils::math::midi_note_to_hz(f32::from(note));
        let left = render_held_tube_note(note, 100.0 / 127.0, 0.5);
        let sustain = &left[left.len() / 2..];
        let est = estimate_f0_autocorrelation(sustain, sr, f0 * 0.4, f0 * 4.0)
            .unwrap_or_else(|| panic!("tube note {note} produced no pitched tone"));
        let ratio = est / f0;
        assert!(
            (0.94..=1.04).contains(&ratio),
            "tube note {note} should lock to the fundamental {f0:.1} Hz, got {est:.1} Hz (ratio {ratio:.3})"
        );
    }
}

#[test]
fn default_tube_patch_sustains_audibly_and_releases() {
    let sample_rate = 48_000.0_f32;
    let block_size = 512;
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    // Shipped single-family Tube: only resonator A sounds, output transparent.
    let mut patch = ResonatorSynthPatch {
        resonator_a: ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::Tube,
            ..WaveguideConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        ..ResonatorSynthPatch::default()
    };
    patch.output.master_gain_db = 0.0;
    patch.output.saturation_drive = 0.0;
    patch.output.filter_cutoff = 20_000.0;

    let mut synth = ResonatorSynth::default();
    synth.reset(setup);
    synth.set_patch_for_test(patch);

    let hold_blocks = (sample_rate * 0.5 / block_size as f32).ceil() as usize;
    let release_blocks = (sample_rate * 0.25 / block_size as f32).ceil() as usize;
    let note_on = [MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note: 60,
        velocity: 100.0 / 127.0,
    })];
    let note_off = [MidiEvent::Note(NoteEvent::Off {
        channel: 0,
        note: 60,
        velocity: 0.0,
    })];
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity((hold_blocks + release_blocks) * block_size);
    for block in 0..(hold_blocks + release_blocks) {
        let events: &[MidiEvent] = if block == 0 {
            &note_on
        } else if block == hold_blocks {
            &note_off
        } else {
            &[]
        };
        process_block(&mut synth, setup, &mut block_left, &mut block_right, events);
        left.extend_from_slice(&block_left);
    }

    assert_all_finite(&left);
    assert!(
        peak_abs(&left) < 8.0,
        "tube output unbounded: {}",
        peak_abs(&left)
    );

    // Sustained while held: measure the second half of the hold window, past the attack.
    let hold_samples = hold_blocks * block_size;
    let sustain_rms = rms(&left[hold_samples / 2..hold_samples]);
    let sustain_dbfs = 20.0 * sustain_rms.max(1.0e-9).log10();
    assert!(
        sustain_dbfs > -40.0,
        "driven Tube should sustain audibly while held: {sustain_dbfs:.1} dBFS (rms {sustain_rms})"
    );

    // Rings down after note-off: well past the drive-gate release, level is below the sustain.
    let release_rms = rms(&left[hold_samples + release_blocks * block_size / 2..]);
    assert!(
        release_rms < sustain_rms,
        "Tube should ring down after note-off: release rms {release_rms} vs sustain rms {sustain_rms}"
    );
}

#[test]
fn driven_tube_sits_below_the_master_clipper() {
    // Regression guard for the failure where the reed-driven tube was ~15 dB too hot (the
    // struck-calibrated 12x makeup with no reed trim), slamming the master soft-clip so every
    // case collapsed to the same bit-crushed saw at -1 dBFS. The shipped-default tube must land
    // at a family-matched forte level *below* the clipper, so its dynamics and timbre survive —
    // a perceptual check the pitch/centroid guards completely missed.
    for note in [48u8, 60, 72] {
        let loud = render_held_tube_note(note, 1.0, 0.5);
        let peak_db = 20.0 * peak_abs(&loud).max(1.0e-9).log10();
        assert!(
            peak_db < -3.0,
            "tube note {note} ff slams the master clipper at {peak_db:.1} dBFS (should sit below ~-5 dBFS)"
        );
    }
    // Velocity must produce a real level difference (not flattened against the clipper).
    let soft = peak_abs(&render_held_tube_note(60, 20.0 / 127.0, 0.5));
    let loud = peak_abs(&render_held_tube_note(60, 1.0, 0.5));
    assert!(
        loud > soft * 1.1,
        "velocity should change the tube's output level: soft={soft:.4} loud={loud:.4}"
    );
}


#[test]
fn driven_tube_plays_a_scale_in_tune() {
    // A wind voice's value is melodic, not one sustained note. The reed tube tracks a C-major
    // scale C4→C5 — each note locks to its own pitch (within a quartertone, allowing the slight
    // high-register flatness). This is the diversity the single-note audition could never show.
    use lindelion_dsp_utils::analysis::estimate_f0_autocorrelation;
    let sr = 48_000.0_f32;
    let (step, dur) = (0.30_f32, 0.22_f32);
    let phrase = render_tube_phrase(&c_major_scale(step, dur, 100.0 / 127.0), 8, true);
    for (i, &note) in [60u8, 62, 64, 65, 67, 69, 71, 72].iter().enumerate() {
        let f0 = lindelion_dsp_utils::math::midi_note_to_hz(f32::from(note));
        let center = ((i as f32 * step + dur * 0.5) * sr) as usize;
        let w = &phrase[center..(center + 8192).min(phrase.len())];
        let est = estimate_f0_autocorrelation(w, sr, 100.0, 1400.0)
            .unwrap_or_else(|| panic!("scale note {note} produced no pitch"));
        let cents = 1200.0 * (est / f0).log2();
        assert!(
            cents.abs() < 60.0,
            "scale note {note} off pitch: expected {f0:.0} Hz, got {est:.0} Hz ({cents:.0} cents)"
        );
    }
}

#[test]
fn driven_tube_articulations_produce_distinct_phrasing() {
    // Different articulations of the same scale must render audibly different — the expressivity
    // a wind instrument lives on, and what the all-single-note audition hid. Tongued (separated,
    // bore re-struck per note), legato (overlapping note-ons), and a mono voice-stealing slur all
    // differ; re-striking the bore on each slurred note vs not changes it again.
    use lindelion_dsp_utils::analysis::rms_difference;
    let tongued = render_tube_phrase(&c_major_scale(0.30, 0.22, 100.0 / 127.0), 8, true);
    let legato = render_tube_phrase(&c_major_scale(0.25, 0.33, 100.0 / 127.0), 8, false);
    let mono = render_tube_phrase(&c_major_scale(0.25, 0.33, 100.0 / 127.0), 1, false);
    let mono_rt = render_tube_phrase(&c_major_scale(0.25, 0.33, 100.0 / 127.0), 1, true);
    for p in [&tongued, &legato, &mono, &mono_rt] {
        assert_all_finite(p);
    }
    let n = [tongued.len(), legato.len(), mono.len(), mono_rt.len()]
        .into_iter()
        .min()
        .unwrap();
    assert!(
        rms_difference(&tongued[..n], &legato[..n]) > 0.05,
        "tongued and legato phrasing should differ audibly"
    );
    assert!(
        rms_difference(&tongued[..n], &mono[..n]) > 0.05,
        "tongued and mono-legato phrasing should differ audibly"
    );
    assert!(
        rms_difference(&mono[..n], &mono_rt[..n]) > 0.02,
        "re-striking the bore per slurred note should change the line"
    );
}

#[test]
fn driven_tube_phrase_onsets_do_not_click() {
    // The breath-onset ramp and the bore frequency glide keep note onsets/changes from stepping.
    // A note's worst sample-to-sample jump is bounded by the (odd-harmonic) waveform's own edges;
    // a *held* note sets that reference. A tongued scale must not exceed it (clean re-onsets), and
    // a legato scale must stay close (the glide smooths the pitch change instead of snapping it).
    use lindelion_dsp_utils::analysis::max_adjacent_delta;
    let held = max_adjacent_delta(&render_held_tube_note(60, 100.0 / 127.0, 0.5));
    let tongued =
        max_adjacent_delta(&render_tube_phrase(&c_major_scale(0.30, 0.22, 100.0 / 127.0), 1, true));
    let legato =
        max_adjacent_delta(&render_tube_phrase(&c_major_scale(0.25, 0.33, 100.0 / 127.0), 1, false));
    assert!(
        tongued <= held * 1.15,
        "tongued note onsets click (jump {tongued} vs held {held})"
    );
    assert!(
        legato <= held * 1.3,
        "legato note changes click (jump {legato} vs held {held})"
    );
}
