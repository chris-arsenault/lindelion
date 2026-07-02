use super::*;
use crate::{ReedDriver, ReedParams};
use lindelion_dsp_utils::{
    analysis::{
        assert_all_finite, audio_window_metrics, dft_magnitude_at,
        estimate_f0_autocorrelation_refined, max_adjacent_delta, peak_abs, rms, rms_difference,
    },
    math::{cents_between, midi_note_to_hz},
};

#[test]
fn reed_driven_tube_renders_finite_audible_audio() {
    let output = render_reed_tube(48_000.0, ReedTubeParams::default(), 0.8, 4_096);

    assert_all_finite(&output);
    assert!(peak_abs(&output) > 0.001);
    assert!(rms(&output[1_024..]) > 0.000_01);
}

#[test]
fn reed_driven_tube_decays_after_drive_is_removed() {
    let sample_rate = 48_000.0;
    let mut reed = ReedDriver::new(ReedParams::default(), sample_rate);
    let mut tube = ReedTube::new(sample_rate);
    let params = ReedTubeParams {
        frequency_hz: 220.0,
        ..ReedTubeParams::default()
    };
    let output = (0..24_000)
        .map(|index| {
            let gate = if index < 12_000 { 1.0 } else { 0.0 };
            let excitation = if index == 0 { 0.4 } else { 0.0 };
            tube.set_brightness_effort(0.8 * gate);
            let mouth = reed.process(excitation, 0.8, tube.driven_feedback(), gate);
            tube.process_wind(mouth, params)
        })
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(rms(&output[18_000..]) < rms(&output[6_000..12_000]));
}

#[test]
fn tuning_tracks_low_mid_register() {
    let sample_rate = 48_000.0;
    for frequency_hz in [110.0, 220.0, 440.0] {
        let output = render_reed_tube(
            sample_rate,
            ReedTubeParams {
                frequency_hz,
                loop_filter_cutoff_hz: 12_000.0,
                loop_gain: 0.992,
                ..ReedTubeParams::default()
            },
            0.9,
            32_000,
        );
        let estimate = estimate_f0_autocorrelation_refined(
            &output[8_000..],
            sample_rate,
            frequency_hz * 0.70,
            frequency_hz * 1.6,
        )
        .unwrap_or_else(|| panic!("no pitch estimate for {frequency_hz} Hz"));
        let cents = cents_between(frequency_hz, estimate);

        assert!(
            cents < 70.0,
            "frequency={frequency_hz} estimate={estimate} cents={cents}"
        );
    }
}

#[test]
fn boundary_reflection_materially_changes_bore_response() {
    let sample_rate = 48_000.0;
    let base = ReedTubeParams {
        frequency_hz: 220.0,
        loop_filter_cutoff_hz: 8_000.0,
        loop_gain: 0.97,
        ..ReedTubeParams::default()
    };
    let closed = render_reed_tube(
        sample_rate,
        ReedTubeParams {
            boundary_reflection: -0.85,
            ..base
        },
        0.8,
        16_000,
    );
    let open = render_reed_tube(
        sample_rate,
        ReedTubeParams {
            boundary_reflection: -0.45,
            ..base
        },
        0.8,
        16_000,
    );

    assert_all_finite(&closed);
    assert_all_finite(&open);
    assert!(rms_difference(&closed[2_048..], &open[2_048..]) > 0.000_01);
}

#[test]
fn reset_clears_bore_state() {
    let sample_rate = 48_000.0;
    let mut reed = ReedDriver::new(ReedParams::default(), sample_rate);
    let mut tube = ReedTube::new(sample_rate);
    let params = ReedTubeParams::default();
    for index in 0..4_096 {
        let excitation = if index == 0 { 0.4 } else { 0.0 };
        let mouth = reed.process(excitation, 0.8, tube.driven_feedback(), 1.0);
        tube.process_wind(mouth, params);
    }
    tube.reset();

    let output = (0..512)
        .map(|_| tube.process_wind(0.0, params))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(audio_window_metrics(&output, sample_rate).peak_abs < 0.000_001);
}

#[test]
fn bore_model_derived_once_for_constant_params() {
    let mut tube = ReedTube::new(48_000.0);
    let params = ReedTubeParams {
        frequency_hz: 196.0,
        loop_filter_cutoff_hz: 7_500.0,
        loop_filter_resonance: 0.15,
        loop_gain: 0.95,
        loop_nonlinearity: 0.05,
        pickup_position: 0.72,
        boundary_reflection: -0.82,
        ..ReedTubeParams::default()
    };

    for index in 0..2_048 {
        tube.process_wind((index == 0) as u8 as f32 * 0.3, params);
    }

    assert_eq!(
        tube.recompute_count, 1,
        "bore operators recomputed per sample"
    );
}

#[test]
fn bore_model_recomputes_when_params_move() {
    let mut tube = ReedTube::new(48_000.0);
    let base = ReedTubeParams {
        frequency_hz: 196.0,
        loop_filter_cutoff_hz: 7_500.0,
        loop_gain: 0.95,
        boundary_reflection: -0.82,
        ..ReedTubeParams::default()
    };
    let moved = ReedTubeParams {
        boundary_reflection: -0.5,
        ..base
    };

    for _ in 0..16 {
        tube.process_wind(0.0, base);
    }
    let settled = tube.recompute_count;
    for _ in 0..8_000 {
        tube.process_wind(0.0, moved);
    }

    assert_eq!(settled, 1, "constant base params should derive once");
    assert!(
        tube.recompute_count > settled,
        "moving params should invalidate the cache"
    );
}

#[test]
#[ignore = "tone guard parked during audition-driven register-voice revision (docs/plugins/lamath-backlog.md); re-derive against the audition-approved model"]
fn bell_radiation_does_not_depend_on_effort() {
    let ratio_at = |effort: f32| {
        let render = |bell_enabled: bool| {
            render_reed_tube(
                48_000.0,
                ReedTubeParams {
                    switches: ReedTubeSwitches {
                        bell_enabled,
                        ..ReedTubeSwitches::default()
                    },
                    ..ReedTubeParams::default()
                },
                effort,
                8_192,
            )
        };
        let on = render(true);
        let off = render(false);
        assert_all_finite(&on);
        peak_abs(&on[2_048..]) / peak_abs(&off[2_048..]).max(1.0e-6)
    };
    let soft = ratio_at(0.45);
    let hard = ratio_at(1.0);

    assert!(
        (soft / hard).max(hard / soft) < 1.3,
        "bell contribution is effort-dependent: soft ratio {soft}, hard ratio {hard}"
    );
}

#[test]
fn effort_changes_level_while_preserving_tuning() {
    // Efforts sit above this generic 48 kHz configuration's speaking threshold (~0.6; the
    // shipped voice always runs the model at 2x the host rate, where soft efforts speak), and
    // both windows must actually speak — comparing the pitch of a non-oscillating bore's noise
    // ring is meaningless.
    let sample_rate = 48_000.0;
    let params = ReedTubeParams {
        frequency_hz: 196.0,
        loop_filter_cutoff_hz: 12_000.0,
        loop_filter_resonance: 0.1,
        loop_gain: 0.985,
        ..ReedTubeParams::default()
    };
    let quiet = render_reed_tube(sample_rate, params, 0.65, 24_000);
    let loud = render_reed_tube(sample_rate, params, 1.0, 24_000);
    let quiet_window = &quiet[12_000..20_000];
    let loud_window = &loud[12_000..20_000];

    assert_all_finite(&quiet);
    assert_all_finite(&loud);
    let dbfs = |samples: &[f32]| 20.0 * rms(samples).max(1.0e-9).log10();
    assert!(
        dbfs(quiet_window) > -40.0 && dbfs(loud_window) > -40.0,
        "both efforts must speak: quiet {} dB loud {} dB",
        dbfs(quiet_window),
        dbfs(loud_window)
    );
    assert!(
        rms(loud_window) > rms(quiet_window) * 1.05,
        "quiet_rms={} loud_rms={}",
        rms(quiet_window),
        rms(loud_window)
    );

    let estimate =
        |samples: &[f32]| estimate_f0_autocorrelation_refined(samples, sample_rate, 100.0, 260.0);
    let quiet_f0 = estimate(quiet_window).expect("quiet pitch");
    let loud_f0 = estimate(loud_window).expect("loud pitch");
    assert!(
        cents_between(quiet_f0, loud_f0).abs() < 40.0,
        "quiet_f0={quiet_f0} loud_f0={loud_f0}"
    );
}

#[test]
fn model_switches_do_not_make_output_non_finite() {
    let output = render_reed_tube(
        48_000.0,
        ReedTubeParams {
            switches: ReedTubeSwitches {
                bell_enabled: false,
                bore_steepening_enabled: false,
                body_enabled: false,
                ..ReedTubeSwitches::default()
            },
            ..ReedTubeParams::default()
        },
        0.7,
        512,
    );

    assert_all_finite(&output);
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn tube_stays_finite_bounded_and_audible_across_register() {
    for note in [36_u8, 48, 60, 72, 84] {
        let frequency_hz = midi_note_to_hz(note as f32);
        let output = render_reed_tube(
            48_000.0,
            ReedTubeParams {
                frequency_hz,
                ..ReedTubeParams::default()
            },
            0.85,
            28_800,
        );
        assert_all_finite(&output);
        assert!(
            peak_abs(&output) < 8.0,
            "note {note} peak={}",
            peak_abs(&output)
        );
        let sustain_dbfs = 20.0 * rms(&output[14_400..]).max(1.0e-9).log10();
        assert!(
            sustain_dbfs > -45.0,
            "note {note} sustain {sustain_dbfs:.1} dBFS"
        );
    }
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn steepening_stays_finite_and_bounded_under_extreme_drive() {
    for &(frequency_hz, cutoff, boundary_reflection) in &[
        (55.0, 3_000.0, -0.9),
        (196.0, 12_000.0, -0.8),
        (660.0, 8_000.0, -0.6),
    ] {
        let params = ReedTubeParams {
            frequency_hz,
            loop_filter_cutoff_hz: cutoff,
            loop_filter_resonance: 0.3,
            loop_gain: 0.985,
            loop_nonlinearity: 0.4,
            boundary_reflection,
            ..ReedTubeParams::default()
        };
        let mut reed = ReedDriver::new(ReedParams::default(), 48_000.0);
        let mut tube = ReedTube::new(48_000.0);
        let output = (0..24_000)
            .map(|index| {
                let drive = match index % 7 {
                    0 => 1_000.0,
                    1 => f32::NAN,
                    2 => f32::INFINITY,
                    3 => -5.0,
                    _ => (index as f32 * 0.013).sin() * 50.0,
                };
                tube.set_brightness_effort(drive);
                let phase = std::f32::consts::TAU * frequency_hz * index as f32 / 48_000.0;
                let excitation = 0.4 * phase.sin();
                let mouth = reed.process(excitation, 0.9, tube.driven_feedback(), 1.0);
                tube.process_wind(mouth, params)
            })
            .collect::<Vec<_>>();

        assert_all_finite(&output);
        assert!(
            audio_window_metrics(&output, 48_000.0).peak_abs < 8.0,
            "frequency_hz={frequency_hz} peak too high"
        );
    }
}

#[test]
#[ignore = "tone guard parked during audition-driven register-voice revision (docs/plugins/lamath-backlog.md); re-derive against the audition-approved model"]
fn driven_onsets_are_continuous_against_held_reference() {
    // Re-articulate (fresh excitation impulse) at index 12_000 in both renders, driving the
    // shipped reed-phase compensation. The reference holds 440 Hz across the re-articulation;
    // the output retunes 220 -> 440 at the same instant. Both see the same impulse-onto-a-
    // ringing-tone transient, so the ratio isolates any step the *retune* itself adds — a
    // fair baseline, unlike onset-from-silence which the breath ramp smooths.
    let render = |retune: bool| {
        let mut reed = ReedDriver::new(ReedParams::default(), 48_000.0);
        let mut tube = ReedTube::new(48_000.0);
        (0..24_000)
            .map(|index| {
                let note = if retune && index < 12_000 {
                    220.0
                } else {
                    440.0
                };
                let excitation = if index == 0 || index == 12_000 {
                    0.4
                } else {
                    0.0
                };
                tube.set_brightness_effort(0.8);
                let reed_phase_delay_samples = reed.aperture_phase_delay_samples(note, 0.8);
                let mouth = reed.process(excitation, 0.8, tube.driven_feedback(), 1.0);
                tube.process_wind(
                    mouth,
                    ReedTubeParams {
                        frequency_hz: note,
                        reed_phase_delay_samples,
                        ..ReedTubeParams::default()
                    },
                )
            })
            .collect::<Vec<_>>()
    };
    let held = render(false);
    let output = render(true);

    // The output's largest slew is the octave pitch jump at index 12_000 (the bore delay
    // halving), not the excitation impulse — a constant-pitch re-articulation barely registers.
    // That retune transient runs ~3x the reference note's breath-ramped from-silence onset
    // (which has no such jump); the bar guards against a true step discontinuity, which would
    // be many times larger.
    assert!(
        max_adjacent_delta(&output) <= max_adjacent_delta(&held) * 3.5,
        "retuned tube onset has a step-like discontinuity"
    );
}

fn render_reed_tube(
    sample_rate: f32,
    params: ReedTubeParams,
    effort: f32,
    sample_count: usize,
) -> Vec<f32> {
    let mut reed = ReedDriver::new(ReedParams::default(), sample_rate);
    let mut tube = ReedTube::new(sample_rate);
    // Wire the reed's loop-phase compensation exactly as the processor does, so the helper
    // reflects the shipped reed+bore coupling.
    let params = ReedTubeParams {
        reed_phase_delay_samples: reed.aperture_phase_delay_samples(params.frequency_hz, effort),
        ..params
    };
    (0..sample_count)
        .map(|index| {
            let excitation = if index == 0 { 0.4 } else { 0.0 };
            tube.set_brightness_effort(effort);
            let mouth = reed.process(excitation, effort, tube.driven_feedback(), 1.0);
            tube.process_wind(mouth, params)
        })
        .collect()
}

#[allow(dead_code)]
fn harmonic_richness(samples: &[f32], sample_rate: f32, f0: f32) -> f32 {
    let h1 = dft_magnitude_at(samples, sample_rate, f0).max(1.0e-9);
    [3.0_f32, 5.0, 7.0, 9.0, 11.0]
        .iter()
        .map(|partial| dft_magnitude_at(samples, sample_rate, f0 * *partial))
        .sum::<f32>()
        / h1
}
