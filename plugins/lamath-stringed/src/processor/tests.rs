use super::*;

mod bow;
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
            humanize: 0.0,
            phrasing: 0.0,
            vibrato: 0.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let bright = render_held_note(
        StringPatch {
            brightness: 0.95,
            humanize: 0.0,
            phrasing: 0.0,
            vibrato: 0.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let pickup = render_held_note(
        StringPatch {
            body_balance: 0.0,
            humanize: 0.0,
            phrasing: 0.0,
            vibrato: 0.0,
            ..StringPatch::default()
        },
        60,
        0.9,
        16_000,
    );
    let body = render_held_note(
        StringPatch {
            body_balance: 1.0,
            humanize: 0.0,
            phrasing: 0.0,
            vibrato: 0.0,
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
fn repeated_plucks_make_body_balance_material_between_strikes() {
    let notes = repeated_c4_plucks(5, 0.28, 0.18, 1.0);
    let pickup = render_phrase_with_patch(
        StringPatch {
            body_balance: 0.0,
            ..StringPatch::default()
        },
        &notes,
    );
    let body = render_phrase_with_patch(
        StringPatch {
            body_balance: 1.0,
            ..StringPatch::default()
        },
        &notes,
    );
    let n = pickup.len().min(body.len());
    let pickup_tail = rms_window(&pickup, 0.22, 0.04);
    let body_tail = rms_window(&body, 0.22, 0.04);
    let pickup_late_tail = rms_window(&pickup, 1.34, 0.08);
    let body_late_tail = rms_window(&body, 1.34, 0.08);
    let difference = rms_difference(&pickup[..n], &body[..n]);

    assert_all_finite(&pickup);
    assert_all_finite(&body);
    assert!(
        difference > 0.000_08,
        "repeated pluck difference too small: {difference}"
    );
    let first_tail_delta = relative_delta(pickup_tail, body_tail);
    let late_tail_delta = relative_delta(pickup_late_tail, body_late_tail);
    assert!(
        first_tail_delta > 0.30,
        "first tail change too small: pickup={pickup_tail} body={body_tail} delta={first_tail_delta}"
    );
    // Balance is a listening-point control: since tension modulation moved to
    // the string's own stored energy, the two balance settings render the same
    // string physics and differ only in the pickup/body mix, so the late-tail
    // contrast is the genuine body-memory level difference (the old, larger
    // delta included balance-dependent tension detune through the output-energy
    // bus, a physics-by-mix artifact).
    assert!(
        late_tail_delta > 0.15,
        "late tail change too small: pickup={pickup_late_tail} body={body_late_tail} delta={late_tail_delta}"
    );
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
    render_phrase_with_patch(StringPatch::default(), notes)
}

fn render_phrase_with_patch(patch: StringPatch, notes: &[(u8, f32, f32, f32)]) -> Vec<f32> {
    let total_seconds = notes.iter().map(|(_, _, end, _)| *end).fold(0.0, f32::max) + 0.5;
    let total_blocks = ((SAMPLE_RATE * total_seconds).ceil() as usize).div_ceil(BLOCK);
    let mut processor = StringProcessor::new(SAMPLE_RATE, patch, default_sources());
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

fn rms_window(output: &[f32], start_seconds: f32, duration_seconds: f32) -> f32 {
    let start = (start_seconds * SAMPLE_RATE) as usize;
    let len = (duration_seconds * SAMPLE_RATE) as usize;
    let end = start.saturating_add(len).min(output.len());
    rms(&output[start..end])
}

fn relative_delta(a: f32, b: f32) -> f32 {
    (a - b).abs() / a.abs().max(b.abs()).max(1.0e-9)
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

fn repeated_c4_plucks(
    count: usize,
    step: f32,
    duration: f32,
    velocity: f32,
) -> Vec<(u8, f32, f32, f32)> {
    (0..count)
        .map(|index| {
            let start = index as f32 * step;
            (60, start, start + duration, velocity)
        })
        .collect()
}

// Humanize/phrasing pinned to zero: the variance walks are instance-seeded
// from a global counter, so renders with them active depend on test execution
// order. Regime/contact tests need deterministic physics; the dedicated
// humanize/phrasing tests opt back in with seed-robust loose bounds.
fn bow_smooth_patch() -> StringPatch {
    StringPatch {
        driver: DriverSelection::Bow,
        body: BodySelection::Violin,
        brightness: 0.58,
        damping: 0.40,
        stiffness: 0.42,
        bow_position: 0.16,
        bow_pressure: 0.50,
        bow_speed: 0.38,
        bow_friction: 0.34,
        humanize: 0.0,
        phrasing: 0.0,
        vibrato: 0.0,
        ..StringPatch::default()
    }
}

// Scratch = bowing at the chaos boundary (see the BowScratch render recipe
// for the measured pressure scan): the note stays clearly pitched
// (autocorrelation ≈ 0.7 at the fundamental lag) while the envelope churns
// at several times the smooth bow's roughness. Well past the boundary the
// note disappears into unpitched crunch — distinct, but unusable as an
// articulation.
fn bow_scratch_patch() -> StringPatch {
    StringPatch {
        driver: DriverSelection::Bow,
        body: BodySelection::Violin,
        brightness: 0.58,
        damping: 0.40,
        stiffness: 0.42,
        bow_position: 0.20,
        // Deterministic over-boundary point (knobs pinned, no walk nudges):
        // the shipped recipe sits lower because the default humanize walks
        // help trigger the raucous regime.
        bow_pressure: 0.56,
        bow_speed: 0.32,
        bow_friction: 0.70,
        humanize: 0.0,
        phrasing: 0.0,
        vibrato: 0.0,
        ..StringPatch::default()
    }
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
