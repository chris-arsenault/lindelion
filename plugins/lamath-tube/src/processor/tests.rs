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

#[test]
fn model_switches_stay_finite_and_material() {
    let on = render_held_note(TubePatch::default(), 60, 1.0, 12_000);
    let off = render_held_note(
        TubePatch {
            switches: crate::patch::TubeModelSwitchPatch {
                bell_enabled: false,
                bore_steepening_enabled: false,
                body_enabled: false,
                reed_radiation_enabled: false,
            },
            ..TubePatch::default()
        },
        60,
        1.0,
        12_000,
    );

    assert_all_finite(&on);
    assert_all_finite(&off);
    assert!(rms_difference(&on[2_048..], &off[2_048..]) > 0.000_01);
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

#[test]
fn articulation_key_switches_change_phrasing() {
    let tongue = render_with_selected_slot(0, 60, 0.9, 16_000);
    let sforzando = render_with_selected_slot(1, 60, 0.9, 16_000);
    let legato = render_with_selected_slot(2, 60, 0.9, 16_000);
    let n = tongue.len().min(sforzando.len()).min(legato.len());

    assert_all_finite(&tongue);
    assert_all_finite(&sforzando);
    assert_all_finite(&legato);
    assert!(rms_difference(&tongue[..n], &sforzando[..n]) > 0.000_01);
    assert!(rms_difference(&tongue[..n], &legato[..n]) > 0.000_01);
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

fn default_sources<'a>() -> [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
}

fn custom_sources<'a>(samples: &'a [f32]) -> [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(|slot| ExcitationSource::from_samples(samples, SAMPLE_RATE, slot))
}

fn render_held_note(patch: TubePatch, note: u8, velocity: f32, frames: usize) -> Vec<f32> {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, patch, default_sources());
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    processor.process(&[note_on(note, velocity)], &mut left, &mut right);
    left
}

fn render_with_selected_slot(slot: usize, note: u8, velocity: f32, frames: usize) -> Vec<f32> {
    let mut processor = TubeProcessor::new(SAMPLE_RATE, TubePatch::default(), default_sources());
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    processor.process(
        &[note_on(KEYSWITCH_BASE_NOTE + slot as u8, 1.0)],
        &mut left,
        &mut right,
    );
    processor.process(&[note_on(note, velocity)], &mut left, &mut right);
    left
}

fn render_phrase(notes: &[(u8, f32, f32, f32)]) -> Vec<f32> {
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

fn note_on(note: u8, velocity: f32) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })
}

fn note_off(note: u8) -> MidiEvent {
    MidiEvent::Note(NoteEvent::Off {
        channel: 0,
        note,
        velocity: 0.0,
    })
}
