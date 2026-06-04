use super::*;
use lindelion_dsp_utils::analysis::{
    assert_all_finite, max_adjacent_delta, peak_abs, rms, rms_difference, spectral_centroid_hz,
};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK: usize = 512;

#[test]
fn strike_produces_sustained_finite_ring() {
    let left = render_strikes(CymbalPatch::default(), &[(60, 0, 1.0)], 8_192);

    assert_all_finite(&left);
    assert!(peak_abs(&left) > 0.001);
    assert!(rms(&left[4_096..]) > 0.000_001);
}

#[test]
fn strike_processing_does_not_allocate() {
    let mut processor = CymbalProcessor::new(
        SAMPLE_RATE,
        CymbalPatch::default(),
        ExcitationSource::builtin(),
    );
    let mut left = [0.0; 512];
    let mut right = [0.0; 512];

    crate::assert_no_allocations("lamath_cymbal_process", || {
        processor.process(&[note_on(64, 0.8)], &mut left, &mut right);
        processor.process(&[], &mut left, &mut right);
    });
}

#[test]
fn damp_key_chokes_the_body() {
    let mut processor = CymbalProcessor::new(
        SAMPLE_RATE,
        CymbalPatch::default(),
        ExcitationSource::builtin(),
    );
    let mut left = vec![0.0; 4_096];
    let mut right = vec![0.0; 4_096];
    processor.process(&[note_on(60, 1.0)], &mut left, &mut right);
    let before = rms(&left);

    for block in 0..8 {
        let events = if block == 0 {
            vec![note_on(1, 1.0)]
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
        max_adjacent_delta(&left) < 0.8,
        "retune introduced click-like discontinuity"
    );
}

#[test]
fn loaded_excitation_changes_attack() {
    let builtin = render_strikes(CymbalPatch::default(), &[(60, 0, 0.9)], 8_192);
    let custom = render_with_excitation(
        CymbalPatch::default(),
        ExcitationSource::from_samples(&[0.0, 1.0, -0.75, 0.45, -0.2, 0.0], SAMPLE_RATE),
        &[(60, 0, 0.9)],
        8_192,
    );

    assert_all_finite(&custom);
    assert!(rms_difference(&builtin[..2_048], &custom[..2_048]) > 0.000_01);
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
    render_with_excitation(patch, ExcitationSource::builtin(), strikes, frames)
}

fn render_with_excitation(
    patch: CymbalPatch,
    excitation: ExcitationSource<'_>,
    strikes: &[(u8, usize, f32)],
    frames: usize,
) -> Vec<f32> {
    let mut processor = CymbalProcessor::new(SAMPLE_RATE, patch, excitation);
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

fn note_on(note: u8, velocity: f32) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })
}
