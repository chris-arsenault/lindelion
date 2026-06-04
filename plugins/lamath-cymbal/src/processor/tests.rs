use super::*;
use lindelion_dsp_utils::analysis::{
    assert_all_finite, max_adjacent_delta, peak_abs, rms, rms_difference,
    sampled_high_frequency_ratio, spectral_centroid_hz,
};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK: usize = 512;

#[test]
fn strike_produces_sustained_finite_ring() {
    let left = render_strikes(CymbalPatch::default(), &[(60, 0, 1.0)], 8_192);

    assert_all_finite(&left);
    assert!(
        peak_abs(&left) > 0.03,
        "default cymbal strike should be audible without post-extraction output makeup: peak={}",
        peak_abs(&left)
    );
    assert!(rms(&left[4_096..]) > 0.000_001);
}

#[test]
fn strike_processing_does_not_allocate() {
    let mut processor =
        CymbalProcessor::new(SAMPLE_RATE, CymbalPatch::default(), builtin_sources());
    let mut left = [0.0; 512];
    let mut right = [0.0; 512];

    crate::assert_no_allocations("lamath_cymbal_process", || {
        processor.process(&[note_on(64, 0.8)], &mut left, &mut right);
        processor.process(&[], &mut left, &mut right);
    });
}

#[test]
fn damp_key_chokes_the_body() {
    let mut processor =
        CymbalProcessor::new(SAMPLE_RATE, CymbalPatch::default(), builtin_sources());
    let mut left = vec![0.0; 4_096];
    let mut right = vec![0.0; 4_096];
    processor.process(&[note_on(60, 1.0)], &mut left, &mut right);
    let before = rms(&left);

    for block in 0..8 {
        let events = if block == 0 {
            vec![note_on(4, 1.0)]
        } else {
            Vec::new()
        };
        processor.process(&events, &mut left, &mut right);
    }

    assert!(rms(&left) < before * 0.2);
}

#[test]
fn dense_strikes_render_inside_bounds() {
    let strikes = (0..6)
        .map(|index| {
            (
                60 + (index % 5) as u8,
                index * 900,
                0.65 + index as f32 * 0.02,
            )
        })
        .collect::<Vec<_>>();
    let left = render_strikes(CymbalPatch::default(), &strikes, 8_192);

    assert_all_finite(&left);
    assert!(peak_abs(&left) < 1.0, "peak={}", peak_abs(&left));
    assert!(rms(&left) > 0.000_01);
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn ring_sustains_multiple_seconds() {
    let left = render_strikes(
        CymbalPatch {
            damping: 0.12,
            ..CymbalPatch::default()
        },
        &[(60, 0, 1.0)],
        144_000,
    );
    let early = rms(&left[4_800..14_400]);
    let late = rms(&left[96_000..120_000]);

    assert_all_finite(&left);
    assert!(
        late > early * 0.02,
        "long cymbal tail collapsed too quickly: early={early}, late={late}"
    );
}

#[test]
fn dynamics_respond_end_to_end() {
    let soft = render_strikes(CymbalPatch::default(), &[(60, 0, 0.25)], 12_000);
    let loud = render_strikes(CymbalPatch::default(), &[(60, 0, 1.0)], 12_000);

    assert_all_finite(&soft);
    assert_all_finite(&loud);
    assert!(
        peak_abs(&loud) > peak_abs(&soft) * 1.4,
        "soft={} loud={}",
        peak_abs(&soft),
        peak_abs(&loud)
    );
}

#[test]
fn retune_sequence_stays_bounded_and_continuous() {
    let strikes = [
        (48, 0, 0.9),
        (60, 4_800, 0.9),
        (72, 9_600, 0.9),
        (55, 14_400, 0.9),
    ];
    let left = render_strikes(CymbalPatch::default(), &strikes, 16_384);

    assert_all_finite(&left);
    assert!(peak_abs(&left) < 1.0, "peak={}", peak_abs(&left));
    assert!(
        max_adjacent_delta(&left) < 0.2,
        "retune introduced click-like discontinuity: max_delta={}",
        max_adjacent_delta(&left)
    );
}

#[test]
fn loaded_excitation_changes_attack() {
    let builtin = render_strikes(CymbalPatch::default(), &[(60, 0, 0.9)], 8_192);
    let custom = render_with_sources(
        CymbalPatch::default(),
        custom_sources(&[0.0, 1.0, -0.75, 0.45, -0.2, 0.0]),
        &[(60, 0, 0.9)],
        8_192,
    );

    assert_all_finite(&custom);
    assert!(rms_difference(&builtin[..2_048], &custom[..2_048]) > 0.000_01);
}

#[test]
fn striker_keyswitch_selects_slot_without_triggering_audio() {
    let mut processor =
        CymbalProcessor::new(SAMPLE_RATE, CymbalPatch::default(), builtin_sources());
    let mut left = [0.0; BLOCK];
    let mut right = [0.0; BLOCK];

    processor.process(&[note_on(2, 1.0)], &mut left, &mut right);

    assert_eq!(processor.selected_slot(), 2);
    assert_eq!(processor.active_injectors(), 0);
    assert!(left.iter().all(|sample| sample.abs() == 0.0));
}

#[test]
fn builtin_striker_slots_have_distinct_attacks() {
    let hard_stick = render_strikes(CymbalPatch::default(), &[(60, 0, 0.9)], 8_192);
    let jazz_brush = render_strikes(CymbalPatch::default(), &[(2, 0, 1.0), (60, 0, 0.9)], 8_192);

    assert_all_finite(&jazz_brush);
    assert!(rms_difference(&hard_stick[..2_048], &jazz_brush[..2_048]) > 0.000_01);
}

#[test]
fn timbre_controls_are_audible_axes() {
    let sparse = render_strikes(
        CymbalPatch {
            size: 0.08,
            tension: 0.08,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    let dense = render_strikes(
        CymbalPatch {
            size: 0.95,
            tension: 0.9,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    let free = render_strikes(
        CymbalPatch {
            material: 0.1,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    let fixed = render_strikes(
        CymbalPatch {
            material: 0.95,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );

    assert_all_finite(&sparse);
    assert_all_finite(&dense);
    assert_all_finite(&free);
    assert_all_finite(&fixed);
    assert!(rms_difference(&sparse[1_024..], &dense[1_024..]) > 0.000_01);
    assert!(rms_difference(&free[1_024..], &fixed[1_024..]) > 0.000_01);
    let sparse_centroid = spectral_centroid_hz(&sparse[2_048..], SAMPLE_RATE).unwrap_or(0.0);
    let dense_centroid = spectral_centroid_hz(&dense[2_048..], SAMPLE_RATE).unwrap_or(0.0);
    assert_ne!(
        sparse_centroid.round() as i32,
        dense_centroid.round() as i32
    );
}

#[test]
fn crash_voicing_keeps_dense_bloom() {
    let crash = render_strikes(
        CymbalPatch {
            size: 0.95,
            tension: 0.88,
            damping: 0.10,
            material: 0.40,
            strike_position: 0.9,
            output_gain_db: -5.0,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );

    let crash_early = &crash[1_024..5_120];
    let crash_mid = &crash[9_600..20_000];
    let crash_high = sampled_high_frequency_ratio(crash_early, SAMPLE_RATE, 3_000.0, 500.0);

    assert_all_finite(&crash);
    assert!(
        crash_high > 0.015,
        "crash should retain >3 kHz shimmer energy: ratio={crash_high}"
    );
    assert!(
        rms(crash_mid) > rms(crash_early) * 0.4,
        "crash bloom should sustain after the attack: early={} mid={}",
        rms(crash_early),
        rms(crash_mid)
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn hard_contact_ab_moves_dense_plate_toward_cymbal_air() {
    let soft_contact = render_strikes(
        CymbalPatch {
            size: 0.98,
            tension: 0.98,
            damping: 0.18,
            material: 0.25,
            strike_position: 0.9,
            pickup_spread: 0.18,
            output_gain_db: -8.0,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );
    let hard_contact = render_strikes(
        CymbalPatch {
            size: 0.98,
            tension: 0.98,
            damping: 0.18,
            material: 0.95,
            strike_position: 0.9,
            pickup_spread: 0.18,
            output_gain_db: -8.0,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );

    let soft_early = &soft_contact[1_024..5_120];
    let hard_early = &hard_contact[1_024..5_120];
    let soft_high = sampled_high_frequency_ratio(soft_early, SAMPLE_RATE, 3_000.0, 500.0);
    let hard_high = sampled_high_frequency_ratio(hard_early, SAMPLE_RATE, 3_000.0, 500.0);
    let soft_air = sampled_high_frequency_ratio(soft_early, SAMPLE_RATE, 6_000.0, 500.0);
    let hard_air = sampled_high_frequency_ratio(hard_early, SAMPLE_RATE, 6_000.0, 500.0);

    assert_all_finite(&soft_contact);
    assert_all_finite(&hard_contact);
    assert!(
        peak_abs(&hard_contact) < 1.0,
        "peak={}",
        peak_abs(&hard_contact)
    );
    assert!(
        hard_high > soft_high * 1.25,
        "hard contact should lift broadband high energy: soft={soft_high} hard={hard_high}"
    );
    assert!(
        hard_air > soft_air * 1.25,
        "hard contact should lift cymbal air energy: soft={soft_air} hard={hard_air}"
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn kit_ride_tail_decays_faster_than_kit_crash_tail() {
    let ride = render_strikes(
        CymbalPatch {
            size: 0.86,
            tension: 0.84,
            damping: 0.56,
            material: 0.95,
            strike_position: 0.82,
            pickup_spread: 0.24,
            output_gain_db: -5.0,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );
    let crash = render_strikes(
        CymbalPatch {
            size: 0.98,
            tension: 0.98,
            damping: 0.14,
            material: 0.95,
            strike_position: 0.9,
            pickup_spread: 0.18,
            output_gain_db: -8.0,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );

    let ride_tail_ratio = rms(&ride[14_400..24_000]) / rms(&ride[1_024..7_200]).max(1.0e-9);
    let crash_tail_ratio = rms(&crash[14_400..24_000]) / rms(&crash[1_024..7_200]).max(1.0e-9);

    assert_all_finite(&ride);
    assert_all_finite(&crash);
    assert!(
        ride_tail_ratio < 0.35,
        "kit ride should not carry a crash-length tail: ratio={ride_tail_ratio}"
    );
    assert!(
        ride_tail_ratio < crash_tail_ratio * 0.55,
        "kit ride tail should decay much faster than kit crash: ride={ride_tail_ratio} crash={crash_tail_ratio}"
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn full_timbre_sweep_stays_finite_bounded_and_audible() {
    for patch in [
        CymbalPatch {
            size: 0.05,
            tension: 0.1,
            damping: 0.75,
            material: 0.2,
            ..CymbalPatch::default()
        },
        CymbalPatch {
            size: 0.5,
            tension: 0.5,
            damping: 0.3,
            material: 0.6,
            ..CymbalPatch::default()
        },
        CymbalPatch {
            size: 0.97,
            tension: 0.92,
            damping: 0.05,
            material: 0.9,
            ..CymbalPatch::default()
        },
    ] {
        let left = render_strikes(patch, &[(60, 0, 1.0)], 48_000);
        assert_all_finite(&left);
        assert!(peak_abs(&left) < 1.0, "peak={}", peak_abs(&left));
        assert!(rms(&left) > 0.000_01);
    }
}

fn render_strikes(patch: CymbalPatch, strikes: &[(u8, usize, f32)], frames: usize) -> Vec<f32> {
    render_with_sources(patch, builtin_sources(), strikes, frames)
}

fn render_with_sources(
    patch: CymbalPatch,
    sources: [ExcitationSource<'_>; STRIKER_SLOT_COUNT],
    strikes: &[(u8, usize, f32)],
    frames: usize,
) -> Vec<f32> {
    let mut processor = CymbalProcessor::new(SAMPLE_RATE, patch, sources);
    let total_blocks = frames.div_ceil(BLOCK);
    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];
    let mut rendered = Vec::with_capacity(total_blocks * BLOCK);
    for block in 0..total_blocks {
        let block_start = block * BLOCK;
        let block_end = block_start + BLOCK;
        let events = strikes
            .iter()
            .filter(|(_, sample, _)| (block_start..block_end).contains(sample))
            .map(|(note, _, velocity)| note_on(*note, *velocity))
            .collect::<Vec<_>>();
        processor.process(&events, &mut left, &mut right);
        rendered.extend_from_slice(&left);
    }
    rendered.truncate(frames);
    rendered
}

fn builtin_sources<'a>() -> [ExcitationSource<'a>; STRIKER_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
}

fn custom_sources<'a>(samples: &'a [f32]) -> [ExcitationSource<'a>; STRIKER_SLOT_COUNT] {
    std::array::from_fn(|slot| {
        if slot == 0 {
            ExcitationSource::from_samples(samples, SAMPLE_RATE, slot)
        } else {
            ExcitationSource::builtin(slot)
        }
    })
}

fn note_on(note: u8, velocity: f32) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })
}
