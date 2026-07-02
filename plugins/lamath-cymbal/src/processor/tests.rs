use super::*;
use lindelion_dsp_utils::analysis::{
    assert_all_finite, max_adjacent_delta, peak_abs, rms, rms_difference,
    sampled_high_frequency_ratio, spectral_centroid_hz,
};

pub(crate) const SAMPLE_RATE: f32 = 48_000.0;
pub(crate) const BLOCK: usize = 512;

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
    // Plate decay law at damping 0.12: low-band T60 ≈ 3.0 s puts the 2.0–2.5 s window
    // ≈ −41 dB under the early window (the plate's highs die first by design, unlike the
    // membrane whose barely-damped high band inflated the late RMS). Guard at half the
    // analytic ratio: a collapsed tail reads orders of magnitude below this.
    assert!(
        late > early * 0.005,
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
    // Continuity is asserted at musical velocities: at stacked full-velocity torture
    // levels the output legitimately works the soft limit, where adjacency deltas
    // saturate toward the rail and stop measuring continuity. Runaway itself is guarded
    // scale-free by `stacked_strikes_ring_decays`.
    let strikes = [
        (48, 0, 0.6),
        (60, 4_800, 0.6),
        (72, 9_600, 0.6),
        (55, 14_400, 0.6),
    ];
    let left = render_strikes(CymbalPatch::default(), &strikes, 16_384);

    assert_all_finite(&left);
    assert!(peak_abs(&left) < 1.0, "peak={}", peak_abs(&left));
    assert!(
        max_adjacent_delta(&left) < 0.8,
        "retune introduced catastrophic discontinuity: max_delta={}",
        max_adjacent_delta(&left)
    );
}

// Scale-free runaway regression (M3): four stacked full-velocity strikes must leave a
// ring whose raw envelope decays — the deep-cascade parametric runaway railed here
// before the energy-conserving (divergence-form) tension landed.
#[test]
fn stacked_strikes_ring_decays() {
    let strikes = [
        (48, 0, 1.0),
        (60, 4_800, 1.0),
        (72, 9_600, 1.0),
        (55, 14_400, 1.0),
    ];
    let left = render_strikes(CymbalPatch::default(), &strikes, 144_000);
    assert_all_finite(&left);
    let block_peak = |b: usize| {
        left[24_000 + b * 12_000..24_000 + (b + 1) * 12_000]
            .iter()
            .fold(0.0_f32, |a, &v| a.max(v.abs()))
    };
    let mut previous = block_peak(0);
    assert!(previous > 0.0, "no ring at all");
    for b in 1..9 {
        let peak = block_peak(b);
        assert!(
            peak < (previous * 0.97).max(1.0e-4),
            "ring failed to decay at block {b}: {peak} after {previous}"
        );
        previous = peak;
    }
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

// M1 exit condition: *every* control is an audible axis. Damping orders the tail; strike
// position, pickup spread, and the played note each change the render above the same
// audibility floor as the size/tension/material axes.
#[test]
fn damping_strike_pickup_and_note_are_audible_axes() {
    let ringy = render_strikes(
        CymbalPatch {
            damping: 0.1,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );
    let choked = render_strikes(
        CymbalPatch {
            damping: 0.7,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        24_000,
    );
    let tail_ratio = |out: &[f32]| rms(&out[16_000..24_000]) / rms(&out[1_024..6_000]).max(1.0e-9);
    assert!(
        tail_ratio(&ringy) > tail_ratio(&choked) * 1.5,
        "damping must shorten the tail: ringy={} choked={}",
        tail_ratio(&ringy),
        tail_ratio(&choked)
    );

    let strike_near = render_strikes(
        CymbalPatch {
            strike_position: 0.2,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    let strike_far = render_strikes(
        CymbalPatch {
            strike_position: 0.8,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    assert!(rms_difference(&strike_near[1_024..], &strike_far[1_024..]) > 0.000_01);

    let spread_tight = render_strikes(
        CymbalPatch {
            pickup_spread: 0.1,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    let spread_wide = render_strikes(
        CymbalPatch {
            pickup_spread: 0.8,
            ..CymbalPatch::default()
        },
        &[(60, 0, 0.9)],
        8_192,
    );
    assert!(rms_difference(&spread_tight[1_024..], &spread_wide[1_024..]) > 0.000_01);

    let low_note = render_strikes(CymbalPatch::default(), &[(48, 0, 0.9)], 8_192);
    let high_note = render_strikes(CymbalPatch::default(), &[(72, 0, 0.9)], 8_192);
    assert!(rms_difference(&low_note[1_024..], &high_note[1_024..]) > 0.000_01);
}

// Linear-plate crash character guard (ADR-0050): the nonlinear bloom is intentionally
// absent until the M3 cascade lands, so this guards broadband >3 kHz strike energy and a
// sustained body. Threshold pinned ~2× under the value measured with the M1 force-pulse
// strikers (0.027). M3 restores a stronger bloom-specific guard.
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
    eprintln!("crash_high={crash_high}");
    // The bloom guard (M3): the cascade multiplies the linear chain's >3 kHz shimmer
    // (linear ≈ 0.00065; with the cascade at shipped depth ≈ 0.0013, measured after the
    // stability hardening — σ₁ floor and σ₀-scaled span budget). Pinned ~2× under.
    // History: the M1-era 0.027 was tanh-distortion harmonics at pre-M2 levels.
    assert!(
        crash_high > 0.000_6,
        "crash should retain >3 kHz shimmer energy: ratio={crash_high}"
    );
    assert!(
        rms(crash_mid) > rms(crash_early) * 0.4,
        "crash bloom should sustain after the attack: early={} mid={}",
        rms(crash_early),
        rms(crash_mid)
    );
}

#[test]
fn every_shipped_preset_is_finite_bounded_and_audible() {
    // Audibility invariant guard (ADR-0032, "don't ship silent configs"): each Basic-tab voice must
    // produce a sustained, in-bounds signal from a single mid-velocity strike. Stays in `make ci`.
    for preset in crate::presets::CYMBAL_PRESETS {
        let mut patch = CymbalPatch::default();
        preset.apply_to(&mut patch);
        let rendered = render_strikes(patch, &[(60, 0, 0.9)], 12_000);

        assert_all_finite(&rendered);
        assert!(
            peak_abs(&rendered) < 1.0,
            "preset '{}' should stay in bounds: peak={}",
            preset.name,
            peak_abs(&rendered)
        );
        assert!(
            peak_abs(&rendered) > 0.03,
            "preset '{}' should have an audible onset: peak={}",
            preset.name,
            peak_abs(&rendered)
        );
        assert!(
            rms(&rendered[2_048..]) > 0.000_01,
            "preset '{}' should sustain past the attack: rms={}",
            preset.name,
            rms(&rendered[2_048..])
        );
    }
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn bloom_generates_cymbal_air_with_velocity() {
    // M3's air claim: the cascade generates 6 kHz "air" as the strike gets harder. The
    // pre-M3 form of this test compared hard vs soft *contact* through a single pickup
    // position at 3 kHz; that observable proved grid-layout fragile (a few percent change
    // in cell spacing moves the taps relative to nodal lines and flips the reading).
    // Contact-hardness physics is guarded robustly at the source instead: the striker
    // unipolarity/centroid-ordering test and the contact-width mapping test. What this
    // test owns is the velocity→air axis, measured within one voicing (same grid, same
    // taps) so the modal lottery cancels.
    let render_at = |velocity: f32| {
        render_strikes(
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
            &[(60, 0, velocity)],
            24_000,
        )
    };
    let quiet = render_at(0.15);
    let loud = render_at(0.9);
    let air =
        |out: &[f32]| sampled_high_frequency_ratio(&out[1_024..5_120], SAMPLE_RATE, 6_000.0, 250.0);
    let high =
        |out: &[f32]| sampled_high_frequency_ratio(&out[1_024..5_120], SAMPLE_RATE, 3_000.0, 250.0);
    assert_all_finite(&loud);
    assert!(peak_abs(&loud) < 1.0, "peak={}", peak_abs(&loud));
    let (quiet_air, loud_air) = (air(&quiet), air(&loud));
    // Note: a >3 kHz *fraction* comparison is intentionally absent — the cascade pumps
    // the 1–3 kHz wash (the fraction's denominator) even harder than the treble, so the
    // fraction dips at loud strikes while absolute treble grows. The 6 kHz fraction is
    // clean because the linear plate has nothing there at all (measured 5.1× at M3).
    let _ = high(&quiet);
    assert!(
        loud_air > quiet_air * 2.5,
        "the cascade must generate 6 kHz air with velocity: quiet={quiet_air} loud={loud_air}"
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

pub(crate) fn render_strikes(
    patch: CymbalPatch,
    strikes: &[(u8, usize, f32)],
    frames: usize,
) -> Vec<f32> {
    render_with_sources(patch, builtin_sources(), strikes, frames)
}

pub(crate) fn render_with_sources(
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

pub(crate) fn builtin_sources<'a>() -> [ExcitationSource<'a>; STRIKER_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
}

pub(crate) fn custom_sources<'a>(samples: &'a [f32]) -> [ExcitationSource<'a>; STRIKER_SLOT_COUNT] {
    std::array::from_fn(|slot| {
        if slot == 0 {
            ExcitationSource::from_samples(samples, SAMPLE_RATE, slot)
        } else {
            ExcitationSource::builtin(slot)
        }
    })
}

pub(crate) fn note_on(note: u8, velocity: f32) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel: 0,
        note,
        velocity,
    })
}
