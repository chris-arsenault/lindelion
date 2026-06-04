use super::*;
use crate::patch::{BodySelection, DriverSelection, ModelSwitches};
use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, estimate_f0_autocorrelation, max_adjacent_delta, peak_abs, rms,
        rms_difference, spectral_centroid_hz,
    },
    math::midi_note_to_hz,
};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK: usize = 512;

#[test]
fn default_note_produces_finite_audible_output() {
    let left = render_held_note(StringPatch::default(), 60, 1.0, 4_096);

    assert_all_finite(&left);
    assert!(peak_abs(&left) > 0.001);
    assert!(rms(&left[512..]) > 0.000_01);
}

#[test]
fn c_minus_two_key_selects_first_slot_without_audio() {
    let mut processor =
        StringProcessor::new(SAMPLE_RATE, StringPatch::default(), default_sources());
    let mut left = [1.0; 64];
    let mut right = [1.0; 64];
    let events = [note_on(KEYSWITCH_BASE_NOTE, 1.0)];

    processor.process(&events, &mut left, &mut right);

    assert_eq!(processor.selected_slot(), 0);
    assert!(left.iter().all(|sample| *sample == 0.0));
    assert_eq!(left, right);
}

#[test]
fn note_processing_does_not_allocate() {
    let mut processor =
        StringProcessor::new(SAMPLE_RATE, StringPatch::default(), default_sources());
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];
    let events = [note_on(64, 0.8)];

    crate::assert_no_allocations("lamath_stringed_process", || {
        processor.process(&events, &mut left, &mut right);
    });
}

#[test]
fn driver_selection_changes_the_render() {
    let pick = render_held_note(
        StringPatch {
            driver: DriverSelection::Pick,
            ..StringPatch::default()
        },
        60,
        0.9,
        12_000,
    );
    let bow = render_held_note(
        StringPatch {
            driver: DriverSelection::Bow,
            ..StringPatch::default()
        },
        60,
        0.9,
        12_000,
    );
    let none = render_held_note(
        StringPatch {
            driver: DriverSelection::None,
            ..StringPatch::default()
        },
        60,
        0.9,
        12_000,
    );

    assert_all_finite(&pick);
    assert_all_finite(&bow);
    assert_all_finite(&none);
    assert!(rms_difference(&pick[512..], &bow[512..]) > 0.000_01);
    assert!(rms_difference(&pick[512..], &none[512..]) > 0.000_01);
}

#[test]
fn body_selection_changes_the_render_and_disabled_stays_finite() {
    let guitar = render_held_note(
        StringPatch {
            body: BodySelection::Guitar,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let violin = render_held_note(
        StringPatch {
            body: BodySelection::Violin,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let disabled = render_held_note(
        StringPatch {
            body: BodySelection::Disabled,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );

    assert_all_finite(&guitar);
    assert_all_finite(&violin);
    assert_all_finite(&disabled);
    assert!(rms_difference(&guitar[1_024..], &violin[1_024..]) > 0.000_01);
    assert!(rms(&disabled[512..]) > 0.000_001);
}

#[test]
fn model_switches_stay_finite_and_material() {
    let all_on = render_held_note(StringPatch::default(), 60, 0.9, 16_000);
    let contact_off = render_held_note(
        StringPatch {
            switches: ModelSwitches {
                body_contact: false,
                ..ModelSwitches::default()
            },
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let tension_off = render_held_note(
        StringPatch {
            switches: ModelSwitches {
                tension: false,
                ..ModelSwitches::default()
            },
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );

    assert_all_finite(&all_on);
    assert_all_finite(&contact_off);
    assert_all_finite(&tension_off);
    assert!(rms_difference(&all_on[1_024..], &contact_off[1_024..]) > 0.000_01);
    assert!(rms_difference(&all_on[1_024..], &tension_off[1_024..]) > 0.000_001);
}

#[test]
fn brightness_and_body_balance_are_audible_axes() {
    let dark = render_held_note(
        StringPatch {
            brightness: 0.15,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let bright = render_held_note(
        StringPatch {
            brightness: 0.95,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let pickup = render_held_note(
        StringPatch {
            body_balance: 0.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let body = render_held_note(
        StringPatch {
            body_balance: 1.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );

    let dark_centroid = spectral_centroid_hz(&dark[4_096..], SAMPLE_RATE).unwrap_or(0.0);
    let bright_centroid = spectral_centroid_hz(&bright[4_096..], SAMPLE_RATE).unwrap_or(0.0);
    let centroid_delta = (bright_centroid - dark_centroid).abs();
    assert!(
        centroid_delta > 100.0,
        "dark={dark_centroid:.0} bright={bright_centroid:.0}"
    );
    assert!(rms_difference(&dark[1_024..], &bright[1_024..]) > 0.000_01);
    assert!(rms_difference(&pickup[1_024..], &body[1_024..]) > 0.000_01);
}

#[test]
fn articulation_slots_produce_distinct_attacks() {
    let pick = render_with_selected_slot(0, 60, 0.9, 16_000);
    let sforzando = render_with_selected_slot(1, 60, 0.9, 16_000);
    let legato = render_with_selected_slot(2, 60, 0.9, 16_000);
    let n = pick.len().min(sforzando.len()).min(legato.len());

    assert_all_finite(&pick);
    assert_all_finite(&sforzando);
    assert_all_finite(&legato);
    assert!(rms_difference(&pick[..n], &sforzando[..n]) > 0.000_01);
    assert!(rms_difference(&pick[..n], &legato[..n]) > 0.000_01);
}

#[test]
fn loaded_excitation_changes_attack() {
    let builtin = render_held_note(StringPatch::default(), 60, 0.9, 8_192);
    let sources = custom_sources(&[0.0, 1.0, -0.8, 0.55, -0.25, 0.12, 0.0]);
    let mut processor = StringProcessor::new(SAMPLE_RATE, StringPatch::default(), sources);
    let mut left = vec![0.0; 8_192];
    let mut right = vec![0.0; 8_192];
    processor.process(&[note_on(60, 0.9)], &mut left, &mut right);

    assert_all_finite(&left);
    assert!(rms_difference(&builtin[..2_048], &left[..2_048]) > 0.000_01);
}

#[test]
fn scale_notes_are_tuned_well_enough_for_audition() {
    let patch = StringPatch {
        body: BodySelection::Disabled,
        ..StringPatch::default()
    };
    for note in [60_u8, 62, 64, 65, 67, 69, 71, 72] {
        let f0 = midi_note_to_hz(note as f32);
        let left = render_held_note(patch.clone(), note, 100.0 / 127.0, 18_000);
        let estimate =
            estimate_f0_autocorrelation(&left[2_048..14_000], SAMPLE_RATE, f0 * 0.75, f0 * 1.35)
                .unwrap_or_else(|| panic!("scale note {note} produced no pitch"));
        let cents = 1200.0 * (estimate / f0).log2();
        assert!(
            cents.abs() < 100.0,
            "note {note} expected {f0:.0} Hz, got {estimate:.0} Hz ({cents:.0} cents)"
        );
    }
}

#[test]
fn phrase_onsets_do_not_click() {
    let held = max_adjacent_delta(&render_held_note(
        StringPatch::default(),
        72,
        100.0 / 127.0,
        24_000,
    ));
    let tongued = max_adjacent_delta(&render_phrase(&c_major_scale(0.30, 0.22, 100.0 / 127.0)));
    let legato = max_adjacent_delta(&render_phrase(&c_major_scale(0.25, 0.33, 100.0 / 127.0)));

    assert!(tongued <= 0.05, "tongued jump {tongued} vs held {held}");
    assert!(legato <= 0.05, "legato jump {legato} vs held {held}");
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn register_sweep_stays_finite_bounded_and_audible() {
    for note in [36_u8, 48, 60, 72, 84] {
        for velocity in [40.0 / 127.0, 100.0 / 127.0, 1.0] {
            let left = render_held_note(StringPatch::default(), note, velocity, 24_000);
            assert_all_finite(&left);
            assert!(
                peak_abs(&left) < 1.0,
                "note={note} peak={}",
                peak_abs(&left)
            );
            let dbfs = 20.0 * rms(&left[4_096..]).max(1.0e-9).log10();
            let floor = register_sustain_floor_dbfs(note, velocity);
            assert!(
                dbfs > floor,
                "note {note} velocity {velocity}: {dbfs:.1} dBFS below {floor:.1} dBFS"
            );
        }
    }
}

fn register_sustain_floor_dbfs(note: u8, velocity: f32) -> f32 {
    let velocity_db = 20.0 * velocity.clamp(0.001, 1.0).log10();
    let high_register_db = ((note as f32 - 60.0) / 12.0).max(0.0) * 12.0;
    -55.0 + velocity_db - high_register_db
}

fn default_sources<'a>() -> [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
}

fn custom_sources<'a>(samples: &'a [f32]) -> [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(|slot| ExcitationSource::from_samples(samples, SAMPLE_RATE, slot))
}

fn render_held_note(patch: StringPatch, note: u8, velocity: f32, frames: usize) -> Vec<f32> {
    let mut processor = StringProcessor::new(SAMPLE_RATE, patch, default_sources());
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    processor.process(&[note_on(note, velocity)], &mut left, &mut right);
    left
}

fn render_with_selected_slot(slot: usize, note: u8, velocity: f32, frames: usize) -> Vec<f32> {
    let mut processor =
        StringProcessor::new(SAMPLE_RATE, StringPatch::default(), default_sources());
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
    let mut processor =
        StringProcessor::new(SAMPLE_RATE, StringPatch::default(), default_sources());
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
