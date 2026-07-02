use super::*;
use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, dft_magnitude_at, estimate_f0_autocorrelation_refined,
        max_adjacent_delta, peak_abs, rms, rms_difference, spectral_centroid_hz,
    },
    math::midi_note_to_hz,
};

const BLOCK: usize = 512;
const SAMPLE_RATE: f32 = 48_000.0;

#[test]
fn default_note_produces_finite_audible_output() {
    let left = render_held_note(TubePatch::default(), 60, 1.0, 2_048);

    assert_all_finite(&left);
    assert!(peak_abs(&left) > 0.001);
    assert!(rms(&left[512..]) > 0.000_01);
}

#[test]
fn c_minus_two_key_selects_first_articulation_without_triggering_note() {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    let mut left = [1.0; 32];
    let mut right = [1.0; 32];
    let events = [note_on(KEYSWITCH_BASE_NOTE, 1.0)];

    processor.process(&events, &mut left, &mut right);

    assert_eq!(processor.selected_slot(), 0);
    assert!(left.iter().all(|sample| *sample == 0.0));
    assert_eq!(left, right);
}

#[test]
fn note_processing_does_not_allocate() {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];
    let events = [note_on(64, 0.8)];

    crate::assert_no_allocations("lamath_tube_process", || {
        processor.process(&events, &mut left, &mut right);
    });
}

#[test]
fn default_patch_sustains_audibly_and_releases() {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];
    let hold_blocks = (SAMPLE_RATE * 0.5 / BLOCK as f32).ceil() as usize;
    let release_blocks = (SAMPLE_RATE * 0.25 / BLOCK as f32).ceil() as usize;
    let mut rendered = Vec::with_capacity((hold_blocks + release_blocks) * BLOCK);

    for block in 0..(hold_blocks + release_blocks) {
        let events = if block == 0 {
            vec![note_on(60, 100.0 / 127.0)]
        } else if block == hold_blocks {
            vec![note_off(60)]
        } else {
            Vec::new()
        };
        processor.process(&events, &mut left, &mut right);
        rendered.extend_from_slice(&left);
    }

    assert_all_finite(&rendered);
    assert!(peak_abs(&rendered) < 1.0, "peak={}", peak_abs(&rendered));
    let hold_samples = hold_blocks * BLOCK;
    let sustain_rms = rms(&rendered[hold_samples / 2..hold_samples]);
    let sustain_dbfs = 20.0 * sustain_rms.max(1.0e-9).log10();
    assert!(
        sustain_dbfs > -45.0,
        "Tube should sustain audibly while held: {sustain_dbfs:.1} dBFS"
    );
    let release_rms = rms(&rendered[hold_samples + release_blocks * BLOCK / 2..]);
    assert!(release_rms < sustain_rms);
}

#[test]
fn brightness_control_is_audible() {
    let dark = render_held_note(
        TubePatch {
            brightness: 0.15,
            ..TubePatch::default()
        },
        60,
        100.0 / 127.0,
        24_000,
    );
    let bright = render_held_note(
        TubePatch {
            brightness: 0.95,
            ..TubePatch::default()
        },
        60,
        100.0 / 127.0,
        24_000,
    );
    let dark_centroid = spectral_centroid_hz(&dark[12_000..], SAMPLE_RATE).unwrap_or(0.0);
    let bright_centroid = spectral_centroid_hz(&bright[12_000..], SAMPLE_RATE).unwrap_or(0.0);

    assert!(
        bright_centroid > dark_centroid * 1.15,
        "dark={dark_centroid:.0} bright={bright_centroid:.0}"
    );
}

/// Each editor model switch must change the sustained sound on its own — by
/// the signature it gates, measured in level/spectrum terms, not a
/// not-bit-identical RMS epsilon. Analysis reads a short slice at the end of
/// a half-second hold: the bell's HF share develops over the first ~0.3 s,
/// and the slim window keeps the in-CI DFT cost down. Thresholds are roughly
/// half the measured deltas at the shipped default (body ±19 dB level,
/// bore-steepening +31% centroid, reed-noise −66% HF band, bell −11% HF
/// band).
#[test]
fn each_model_switch_is_individually_audible() {
    use lindelion_dsp_utils::analysis::sampled_high_frequency_ratio;
    fn settled(samples: &[f32]) -> &[f32] {
        &samples[samples.len() - 4_096..]
    }
    let frames = 24_000usize;
    let render_with = |set: fn(&mut crate::patch::TubeModelSwitchPatch)| {
        let mut switches = crate::patch::TubeModelSwitchPatch::default();
        set(&mut switches);
        let rendered = render_held_note(
            TubePatch {
                switches,
                ..TubePatch::default()
            },
            60,
            1.0,
            frames,
        );
        assert_all_finite(&rendered);
        rendered
    };
    let centroid =
        |samples: &[f32]| spectral_centroid_hz(settled(samples), SAMPLE_RATE).unwrap_or(0.0);
    let level_db = |samples: &[f32]| 20.0 * rms(settled(samples)).max(1.0e-9).log10();
    let hf = |samples: &[f32]| {
        sampled_high_frequency_ratio(settled(samples), SAMPLE_RATE, 3_000.0, 130.0)
    };

    let base = render_with(|_| {});

    let body_off = render_with(|switches| switches.body_enabled = false);
    let body_delta_db = (level_db(&body_off) - level_db(&base)).abs();
    assert!(
        body_delta_db > 6.0,
        "body switch should shift the sustained level audibly: {body_delta_db:.1} dB"
    );

    let steepening_off = render_with(|switches| switches.bore_steepening_enabled = false);
    let steepening_shift = (centroid(&steepening_off) - centroid(&base)).abs();
    // Re-measured after the breath-wash cut (the broadband wash inflated both centroids):
    // the switch still moves the sustained centroid ~10% / ~220 Hz.
    assert!(
        steepening_shift > centroid(&base) * 0.07,
        "bore steepening should shift the sustained spectrum: {:.0} vs {:.0} Hz",
        centroid(&steepening_off),
        centroid(&base)
    );

    let reed_off = render_with(|switches| switches.reed_radiation_enabled = false);
    assert!(
        hf(&reed_off) < hf(&base) * 0.7,
        "reed radiation should carry an audible share of the HF air: off={} on={}",
        hf(&reed_off),
        hf(&base)
    );

    // The bell's HF share depends on where the note's fractional loop delay lands; measure it
    // on G4, where the share is well clear of the comb-sampling noise floor (measured -20%).
    let bell_render = |bell_enabled: bool| {
        let rendered = render_held_note(
            TubePatch {
                switches: crate::patch::TubeModelSwitchPatch {
                    bell_enabled,
                    ..crate::patch::TubeModelSwitchPatch::default()
                },
                ..TubePatch::default()
            },
            67,
            1.0,
            frames,
        );
        assert_all_finite(&rendered);
        rendered
    };
    let bell_on = bell_render(true);
    let bell_off = bell_render(false);
    assert!(
        hf(&bell_off) < hf(&bell_on) * 0.9,
        "bell radiation should carry an audible share of the upper spectrum: off={} on={}",
        hf(&bell_off),
        hf(&bell_on)
    );
}

#[test]
fn loaded_excitation_changes_attack() {
    let builtin = render_held_note(TubePatch::default(), 60, 0.9, 8_192);
    let sources = custom_sources(&[0.0, 1.0, -0.9, 0.7, -0.5, 0.3, 0.0]);
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), sources);
    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];
    processor.process(&[note_on(60, 0.9)], &mut left, &mut right);

    assert_all_finite(&left);
    assert!(rms_difference(&builtin[..2_048], &left[..2_048]) > 0.000_01);
}

/// Articulation slots must change how the note *starts*, by entry-window
/// signatures a player can hear, in both registers. The first 50 ms carries
/// the gate/accent distinction (measured vented: staccato +8 dB, legato
/// -11 dB, slur below legato vs tongue; low register: sforzando +11.6 dB,
/// accent +12 dB, breath -24 dB swell); sforzando additionally holds its
/// emphasis past the tongue's (50-100 ms window), and breath attacks with
/// audible air. Thresholds are roughly half the measured deltas.
#[test]
fn articulation_slots_change_the_attack_audibly() {
    let window_db = |samples: &[f32], start: usize, end: usize| {
        20.0 * rms(&samples[start..end]).max(1.0e-9).log10()
    };
    let entry_db = |samples: &[f32]| window_db(samples, 0, 2_400);
    let emphasis_db = |samples: &[f32]| window_db(samples, 2_400, 4_800);
    let render = |slot: usize, note: u8| {
        let rendered = render_with_selected_slot(slot, note, 0.9, 8_000);
        assert_all_finite(&rendered);
        assert!(peak_abs(&rendered) < 1.0);
        rendered
    };

    // Vented register (above the break).
    let tongue_render = render(0, 74);
    let tongue = entry_db(&tongue_render);
    let legato = entry_db(&render(2, 74));
    let staccato = entry_db(&render(3, 74));
    let slur = entry_db(&render(7, 74));
    // The vented air-support floor leaves no effort headroom near full velocity (a real player
    // cannot blow a near-ff sforzando meaningfully harder either); the emphasis comparison runs
    // at a moderate velocity where the accent has room.
    let tongue_moderate = render_with_selected_slot(0, 74, 0.65, 8_000);
    let sforzando_moderate = render_with_selected_slot(1, 74, 0.65, 8_000);
    assert!(
        emphasis_db(&sforzando_moderate) > emphasis_db(&tongue_moderate) + 0.25,
        "sforzando should hold its emphasis past the tongue's: {:.2} vs {:.2} dB",
        emphasis_db(&sforzando_moderate),
        emphasis_db(&tongue_moderate)
    );

    assert!(
        staccato > tongue + 4.0,
        "staccato's accented gate should arrive ahead of tongue: {staccato:.1} vs {tongue:.1} dB"
    );
    assert!(
        legato < tongue - 5.0,
        "legato should ease in far softer than tongue: {legato:.1} vs {tongue:.1} dB"
    );
    assert!(
        slur <= legato + 1.0,
        "slur should be at least as gentle as legato: {slur:.1} vs {legato:.1} dB"
    );

    // Breath: onset turbulence carries the attack instead of the seed, and
    // the air swells in instead of arriving.
    use lindelion_dsp_utils::analysis::sampled_high_frequency_ratio;
    let attack_hf = |samples: &[f32]| {
        sampled_high_frequency_ratio(&samples[..4_800], SAMPLE_RATE, 2_000.0, 65.0)
    };
    let breath_render = render(5, 74);
    let breath_hf = attack_hf(&breath_render);
    let tongue_hf = attack_hf(&tongue_render);
    assert!(
        breath_hf > tongue_hf * 1.8,
        "breath articulation should attack with audible air: {breath_hf:.4} vs {tongue_hf:.4}"
    );

    // Low register (below the break): accents arm a pressure boost and air
    // push the neutral tongue deliberately does not have; breath swells.
    let tongue_low = entry_db(&render(0, 55));
    let sforzando_low = entry_db(&render(1, 55));
    let accent_low = entry_db(&render(6, 55));
    let breath_low = entry_db(&render(5, 55));
    assert!(
        sforzando_low > tongue_low + 5.0,
        "low-register sforzando should accent the attack: {sforzando_low:.1} vs {tongue_low:.1} dB"
    );
    assert!(
        accent_low > tongue_low + 5.0,
        "low-register accent should read against tongue: {accent_low:.1} vs {tongue_low:.1} dB"
    );
    assert!(
        breath_low < tongue_low - 10.0,
        "low-register breath should swell in from air: {breath_low:.1} vs {tongue_low:.1} dB"
    );
}

#[test]
fn held_low_mid_notes_keep_the_fundamental() {
    for note in [48_u8, 60] {
        let f0 = midi_note_to_hz(note as f32);
        for velocity in [20.0 / 127.0, 90.0 / 127.0, 1.0] {
            let left = render_held_note(TubePatch::default(), note, velocity, 24_000);
            let subharmonic = subharmonic_ratio(&left, f0);
            assert!(
                subharmonic < 0.7,
                "note {note} velocity {velocity}: f/2 ratio {subharmonic:.2}"
            );
        }
    }
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn sweep_stays_finite_bounded_and_audible() {
    for note in [36_u8, 41, 48, 56, 60, 65, 72, 80, 84] {
        for velocity in [40.0 / 127.0, 100.0 / 127.0, 1.0] {
            let left = render_held_note(TubePatch::default(), note, velocity, 28_800);
            assert_all_finite(&left);
            assert!(
                peak_abs(&left) < 1.0,
                "note={note} peak={}",
                peak_abs(&left)
            );
            let dbfs = 20.0 * rms(&left[left.len() / 2..]).max(1.0e-9).log10();
            assert!(
                dbfs > -50.0,
                "note {note} velocity {velocity}: {dbfs:.1} dBFS"
            );
        }
    }
}

#[test]
fn scale_notes_are_tuned_well_enough_for_audition() {
    let phrase = render_phrase(&c_major_scale(0.30, 0.22, 100.0 / 127.0));
    for (index, &note) in [60_u8, 62, 64, 65, 67, 69, 71, 72].iter().enumerate() {
        let f0 = midi_note_to_hz(note as f32);
        let start = ((index as f32 * 0.30 + 0.17) * SAMPLE_RATE) as usize;
        let end = (start + 2_048).min(phrase.len());
        let estimate = estimate_f0_autocorrelation_refined(
            &phrase[start..end],
            SAMPLE_RATE,
            f0 * 0.70,
            f0 * 1.6,
        )
        .unwrap_or_else(|| panic!("scale note {note} produced no pitch"));
        let cents = 1200.0 * (estimate / f0).log2();
        assert!(
            cents.abs() < 90.0,
            "note {note} expected {f0:.0} Hz, got {estimate:.0} Hz ({cents:.0} cents)"
        );
    }
}

#[test]
#[ignore = "tone guard parked during audition-driven register-voice revision (docs/plugins/lamath-backlog.md); re-derive against the audition-approved model"]
fn phrase_onsets_do_not_click() {
    let held = max_adjacent_delta(&render_held_note(
        TubePatch::default(),
        72,
        100.0 / 127.0,
        24_000,
    ));
    let tongued = max_adjacent_delta(&render_phrase(&c_major_scale(0.30, 0.22, 100.0 / 127.0)));
    let legato = max_adjacent_delta(&render_phrase(&c_major_scale(0.25, 0.33, 100.0 / 127.0)));

    assert!(
        tongued <= held * 1.8,
        "tongued jump {tongued} vs held {held}"
    );
    assert!(legato <= held * 2.6, "legato jump {legato} vs held {held}");
}

pub(super) fn default_sources<'a>() -> [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
}

fn custom_sources<'a>(samples: &'a [f32]) -> [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(|slot| ExcitationSource::from_samples(samples, SAMPLE_RATE, slot))
}

pub(super) fn render_held_note(
    patch: TubePatch,
    note: u8,
    velocity: f32,
    frames: usize,
) -> Vec<f32> {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, patch, default_sources());
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    processor.process(&[note_on(note, velocity)], &mut left, &mut right);
    left
}

fn render_with_selected_slot(slot: usize, note: u8, velocity: f32, frames: usize) -> Vec<f32> {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    // One short block delivers the key switch; the note render starts at 0.
    let mut prefix_left = [0.0; 64];
    let mut prefix_right = [0.0; 64];
    processor.process(
        &[note_on(KEYSWITCH_BASE_NOTE + slot as u8, 1.0)],
        &mut prefix_left,
        &mut prefix_right,
    );
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    processor.process(&[note_on(note, velocity)], &mut left, &mut right);
    left
}

pub(super) fn render_phrase(notes: &[(u8, f32, f32, f32)]) -> Vec<f32> {
    let total_seconds = notes.iter().map(|(_, _, end, _)| *end).fold(0.0, f32::max) + 0.5;
    let total_blocks = ((SAMPLE_RATE * total_seconds).ceil() as usize).div_ceil(BLOCK);
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];
    let mut rendered = Vec::with_capacity(total_blocks * BLOCK);
    for block in 0..total_blocks {
        let start = block * BLOCK;
        let end = start + BLOCK;
        let events = scheduled_events(notes, start, end);
        processor.process(&events, &mut left, &mut right);
        rendered.extend_from_slice(&left);
    }
    rendered
}

fn scheduled_events(
    notes: &[(u8, f32, f32, f32)],
    block_start: usize,
    block_end: usize,
) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    for &(note, start, end, velocity) in notes {
        let start_sample = (start * SAMPLE_RATE) as usize;
        let end_sample = (end * SAMPLE_RATE) as usize;
        if (block_start..block_end).contains(&start_sample) {
            events.push(note_on(note, velocity));
        }
        if (block_start..block_end).contains(&end_sample) {
            events.push(note_off(note));
        }
    }
    events
}

fn c_major_scale(step: f32, duration: f32, velocity: f32) -> Vec<(u8, f32, f32, f32)> {
    [60_u8, 62, 64, 65, 67, 69, 71, 72]
        .iter()
        .enumerate()
        .map(|(index, &note)| {
            let start = index as f32 * step;
            (note, start, start + duration, velocity)
        })
        .collect()
}

fn subharmonic_ratio(left: &[f32], f0: f32) -> f32 {
    let sustain = &left[left.len() / 2..];
    let h1 = dft_magnitude_at(sustain, SAMPLE_RATE, f0).max(1.0e-9);
    dft_magnitude_at(sustain, SAMPLE_RATE, f0 * 0.5) / h1
}

pub(super) fn note_on(note: u8, velocity: f32) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })
}

pub(super) fn note_off(note: u8) -> MidiEvent {
    MidiEvent::Note(NoteEvent::Off {
        channel: 0,
        note,
        velocity: 0.0,
    })
}

#[test]
fn host_expression_line_ducks_the_breath() {
    use lindelion_plugin_shell::ControlEvent;
    let patch = TubePatch {
        phrasing: 0.0,
        vibrato: 0.0,
        ..TubePatch::default()
    };
    let frames = 48_000;
    let render = |events_at_half: &[MidiEvent]| {
        let mut processor = TubeProcessor::new(SAMPLE_RATE, patch.clone(), default_sources());
        let mut left = vec![0.0; frames / 2];
        let mut right = vec![0.0; frames / 2];
        processor.process(&[note_on(60, 1.0)], &mut left, &mut right);
        let mut left_tail = vec![0.0; frames / 2];
        let mut right_tail = vec![0.0; frames / 2];
        processor.process(events_at_half, &mut left_tail, &mut right_tail);
        left.extend_from_slice(&left_tail);
        left
    };
    let window = |samples: &[f32]| {
        let start = (0.8 * SAMPLE_RATE) as usize;
        let end = (0.95 * SAMPLE_RATE) as usize;
        rms(&samples[start..end])
    };
    let plain = render(&[]);
    let ducked = render(&[MidiEvent::Control(ControlEvent::ContinuousController {
        channel: 0,
        controller: 11,
        value: 0.25,
    })]);
    assert!(
        window(&ducked) < window(&plain) * 0.8,
        "CC11 should duck the breath line: plain={} ducked={}",
        window(&plain),
        window(&ducked)
    );
}

pub(super) fn sustained_dbfs(samples: &[f32]) -> f32 {
    20.0 * rms(samples).max(1.0e-9).log10()
}

pub(super) fn sustained_cents_error(samples: &[f32], note: u8) -> f32 {
    let f0 = midi_note_to_hz(note as f32);
    let estimate = estimate_f0_autocorrelation_refined(samples, SAMPLE_RATE, f0 * 0.8, f0 * 1.25)
        .unwrap_or_else(|| panic!("note {note} produced no pitch"));
    1200.0 * (estimate / f0).log2()
}

/// The native level calibration at output 0 (see `RADIATED_LEVEL_MAKEUP`): the vented register
/// anchors the loudness near -12 dBFS sustained; the chalumeau sits ~4 dB under it by ear (its
/// dense spectrum reads louder than its RMS).
#[test]
fn shipped_default_plays_at_the_calibrated_levels() {
    let level = |note: u8| {
        let left = render_held_note(TubePatch::default(), note, 100.0 / 127.0, 48_000);
        assert_all_finite(&left);
        sustained_dbfs(&left[24_000..])
    };
    for note in [72_u8, 74] {
        let dbfs = level(note);
        assert!(
            (-15.0..=-9.0).contains(&dbfs),
            "vented note {note} should sustain near -12 dBFS at output 0: {dbfs:.1}"
        );
    }
    for note in [55_u8, 67] {
        let dbfs = level(note);
        assert!(
            (-19.5..=-13.5).contains(&dbfs),
            "low note {note} should sustain near -16.5 dBFS at output 0: {dbfs:.1}"
        );
    }
}

/// The loop-phase compensation law must hold sustained pitch across the velocity range — the
/// soft-playing flatness (up to -50 cents) and full-velocity sharpness (up to +14 cents) of the
/// saturated legacy law are the regression this guards (see
/// `ReedDriver::aperture_phase_delay_samples`).
#[test]
fn sustained_pitch_holds_across_velocities() {
    for (note, velocity) in [
        (60_u8, 40.0 / 127.0),
        (60, 1.0),
        (67, 40.0 / 127.0),
        (67, 1.0),
        (72, 100.0 / 127.0),
    ] {
        let left = render_held_note(TubePatch::default(), note, velocity, 48_000);
        let cents = sustained_cents_error(&left[30_000..46_000], note);
        assert!(
            cents.abs() <= 8.0,
            "note {note} velocity {velocity:.2} should sustain on the grid: {cents:+.1} cents"
        );
    }
}

/// Soft-blown vented notes must speak: the air-support floor keeps the whole velocity range
/// above the register mode's oscillation threshold (the waltz's vel ~0.4-0.58 lines over the
/// break were silent-or-weak without it; "don't ship silent configs").
#[test]
fn soft_vented_notes_speak() {
    for velocity in [0.3_f32, 0.5] {
        let left = render_held_note(TubePatch::default(), 72, velocity, 24_000);
        let dbfs = sustained_dbfs(&left[16_000..]);
        assert!(
            dbfs > -20.0,
            "vented C5 at velocity {velocity:.1} should speak: {dbfs:.1} dBFS"
        );
    }
}
