use super::*;
use crate::assert_no_allocations;
use crate::dsp::render_metrics::{RenderExcitation, render_response};
use crate::dsp::waveguide::WaveguideParams;
use crate::{BowConfig, ContactConfig, DriverConfig, PickConfig, ReedConfig, WaveguideStyle};
use lindelion_dsp_utils::analysis::{
    assert_all_finite, audio_window_metrics, gain_fitted_rms_difference, peak_abs, rms,
};

fn equivalence_params(style: WaveguideStyle) -> WaveguideParams {
    WaveguideParams {
        style,
        frequency_hz: 220.0,
        loop_filter_cutoff: 9_000.0,
        loop_filter_resonance: 0.05,
        loop_gain: 0.99,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.35,
        pickup_position: 0.62,
        ..WaveguideParams::default()
    }
}

fn render_base_rate(params: WaveguideParams, samples: usize) -> Vec<f32> {
    let mut waveguide = WaveguideResonator::new(48_000.0, LOWEST_RESONATOR_FREQUENCY_HZ);
    render_response(
        48_000.0,
        params.frequency_hz,
        samples,
        RenderExcitation::SidechainBurst,
        |sample| waveguide.process_sample(sample, params),
    )
}

fn render_oversampled(params: WaveguideParams, samples: usize) -> Vec<f32> {
    // Mirror what ResonatorEngine does: the core built at 2x, driven through
    // the oversampler at the host rate with the same excitation sequence.
    let mut waveguide = WaveguideResonator::new(2.0 * 48_000.0, LOWEST_RESONATOR_FREQUENCY_HZ);
    let mut oversampler = Oversampler2x::new();
    render_response(
        48_000.0,
        params.frequency_hz,
        samples,
        RenderExcitation::SidechainBurst,
        |sample| oversampler.process(sample, |inner| waveguide.process_sample(inner, params)),
    )
}

#[test]
fn oversampled_waveguide_matches_base_rate_within_filter_tolerance() {
    let latency = Oversampler2x::LATENCY_SAMPLES as usize;

    for style in [WaveguideStyle::String, WaveguideStyle::Tube] {
        let params = equivalence_params(style);
        let base = render_base_rate(params, 12_000);
        let over = render_oversampled(params, 12_000 + latency);

        // Waveform matches within filter tolerance once aligned. The cross-rate
        // onset can differ from the reported wrapper PDC by up to a sample, so
        // align by the best lag within +/-2 of LATENCY_SAMPLES before the
        // sample-wise comparison. A broken wrapper (wrong gain/decimation phase)
        // would push this toward 1.0; the linear cores stay well below.
        let window = 1_024;
        let base_window = &base[512..512 + window];
        let mut best_residual = f32::MAX;
        for lag in -2..=2 {
            let start = ((512 + latency) as isize + lag) as usize;
            let over_window = &over[start..start + window];
            let residual =
                gain_fitted_rms_difference(base_window, over_window) / rms(over_window).max(1.0e-6);
            best_residual = best_residual.min(residual);
        }
        assert!(
            best_residual < 0.25,
            "{style:?} aligned normalized difference={best_residual}"
        );

        // Level is preserved (the wrapper neither boosts nor drops the signal).
        let base_early = rms(&base[512..1_536]);
        let over_early = rms(&over[512 + latency..1_536 + latency]);
        assert!(
            (base_early / over_early.max(1.0e-9) - 1.0).abs() < 0.1,
            "{style:?} early rms base={base_early} over={over_early}"
        );
    }
}

#[test]
fn pass_through_driver_engine_render_matches_pre_driver_path() {
    // The default (Sample) driver is the transparent PassThrough, so the new effort
    // input and the M11 P3 note-state drive gate are both inert: the waveguide
    // receives exactly the excitation it did before the M8 driver seam. A render at
    // zero/full effort and at open/released drive gate must therefore be bit-
    // identical — the seam adds nothing to the pre-driver sample/pluck signal path.
    for style in [WaveguideStyle::String, WaveguideStyle::Tube] {
        let config = ResonatorConfig::Waveguide(WaveguideConfig {
            style,
            ..WaveguideConfig::default()
        });
        let render = |effort: f32, drive_gate: f32| {
            let mut engine = ResonatorEngine::new(48_000.0);
            engine.configure(&config, 220.0, true);
            (0..4_096)
                .map(|index| {
                    engine.process_sample((index == 0) as u8 as f32, 0.0, effort, drive_gate)
                })
                .collect::<Vec<_>>()
        };
        let quiet = render(0.0, 1.0);
        let hard = render(1.0, 1.0);
        let released_gate = render(1.0, 0.0);
        assert_all_finite(&quiet);
        assert!(rms(&quiet[..2_048]) > 0.0, "{style:?} produced silence");
        assert_eq!(quiet, hard, "{style:?} pass-through driver leaked effort");
        assert_eq!(
            quiet, released_gate,
            "{style:?} pass-through driver leaked the drive gate into the sample path"
        );
    }
}

#[test]
fn pick_driver_brightness_rises_with_strike_force() {
    // Pick/hammer contact: a harder strike (higher effort) passes more high
    // frequencies, so the radiated spectral centroid rises with playing force — a
    // timbral change, not just level (cross-cutting "objective audio tests").
    let render = |effort: f32| {
        let mut engine = ResonatorEngine::new(48_000.0);
        engine.configure(
            &ResonatorConfig::Waveguide(WaveguideConfig::default()),
            220.0,
            true,
        );
        engine.set_driver(DriverConfig::Pick(PickConfig {
            hardness: 0.9,
            contact_time: 0.2,
        }));
        render_response(
            48_000.0,
            220.0,
            4_096,
            RenderExcitation::Impulse,
            |sample| engine.process_sample(sample, 0.0, effort, 1.0),
        )
    };
    let soft = render(0.1);
    let hard = render(0.95);
    assert_all_finite(&soft);
    assert_all_finite(&hard);
    let soft_centroid = audio_window_metrics(&soft[..2_048], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let hard_centroid = audio_window_metrics(&hard[..2_048], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    assert!(
        hard_centroid > soft_centroid * 1.1,
        "harder strike should be brighter: soft={soft_centroid} hard={hard_centroid}"
    );
}

#[test]
fn reed_driver_self_oscillates_above_a_pressure_threshold() {
    // Reed valve: below a mouth-pressure (effort) threshold the bore decays to near
    // silence; above it the bore self-oscillates and sustains. This is a regime
    // change driven by force, not a gain change.
    let render = |effort: f32| {
        let mut engine = ResonatorEngine::new(48_000.0);
        engine.configure(
            &ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::Tube,
                ..WaveguideConfig::default()
            }),
            196.0,
            true,
        );
        engine.set_driver(DriverConfig::Reed(ReedConfig::default()));
        render_response(
            48_000.0,
            196.0,
            24_000,
            RenderExcitation::Impulse,
            |sample| engine.process_sample(sample, 0.0, effort, 1.0),
        )
    };
    let quiet = render(0.05);
    let blown = render(0.9);
    assert_all_finite(&quiet);
    assert_all_finite(&blown);
    let late_quiet = rms(&quiet[16_000..]);
    let late_blown = rms(&blown[16_000..]);
    assert!(
        late_blown > 0.01 && late_blown > late_quiet * 5.0,
        "reed should self-oscillate above threshold: quiet={late_quiet} blown={late_blown}"
    );
    // The reed-table nonlinearity bounds the oscillation to a limit cycle (the active
    // element must never run away — it injects energy but stays bounded).
    assert!(
        peak_abs(&blown) < 8.0,
        "reed oscillation must stay bounded: peak={}",
        peak_abs(&blown)
    );
}

/// M11 P3 step 1: the bow driver continuously excites the String via friction, so
/// a held note (constant effort) sustains a non-decaying limit cycle, where a pluck
/// (Sample) rings down. The friction nonlinearity bounds the oscillation.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn bow_driver_sustains_string_while_held() {
    let sample_rate = 48_000.0;
    let f0 = 196.0;
    let render = |effort: f32, driver: DriverConfig| {
        let mut engine = ResonatorEngine::new(sample_rate);
        engine.configure(
            &ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::String,
                ..WaveguideConfig::default()
            }),
            f0,
            true,
        );
        engine.set_driver(driver);
        render_response(
            sample_rate,
            f0,
            144_000,
            RenderExcitation::Impulse,
            |sample| engine.process_sample(sample, 0.0, effort, 1.0),
        )
    };

    let bowed = render(0.85, DriverConfig::Bow(BowConfig::default()));
    let plucked = render(0.85, DriverConfig::Sample);
    assert_all_finite(&bowed);

    // The bow injects continuously: the tail sustains (non-decaying), audible, bounded.
    let bow_mid = rms(&bowed[24_000..48_000]); // 0.5–1.0 s
    let bow_late = rms(&bowed[120_000..144_000]); // 2.5–3.0 s
    assert!(
        bow_late > bow_mid * 0.7,
        "bow should sustain (non-decaying): mid={bow_mid}, late={bow_late}"
    );
    assert!(bow_late > 0.01, "bow tail should be audible: {bow_late}");
    assert!(
        peak_abs(&bowed) < 8.0,
        "bow oscillation must stay bounded: peak={}",
        peak_abs(&bowed)
    );
    // The tail is a genuine oscillation, not a DC friction offset: its AC energy
    // dominates its mean.
    let tail = &bowed[120_000..144_000];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    let ac_rms = (tail.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / tail.len() as f32).sqrt();
    assert!(
        ac_rms > mean.abs() * 2.0,
        "bow tail should oscillate, not sit at a DC offset: ac_rms={ac_rms}, dc={mean}"
    );

    // ...where the free pluck has rung well down over the same span.
    let pluck_mid = rms(&plucked[24_000..48_000]);
    let pluck_late = rms(&plucked[120_000..144_000]);
    assert!(
        pluck_late < pluck_mid * 0.6,
        "pluck should decay: mid={pluck_mid}, late={pluck_late}"
    );
}

/// M11 P3 step 2: the note-state drive gate makes a self-sustaining driver let go
/// on note-off so the resonator rings out, instead of being held by continued
/// driving. A held drive (gate=1) sustains; a released drive (gate ramped 1→0 like
/// the modulation drive-gate envelope) rings down. Covers both the reed (Tube) and
/// the bow (String).
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn drive_gate_release_rings_out_reed_and_bow() {
    let sample_rate = 48_000.0;
    // Mirror the modulation drive-gate release rate (DRIVE_GATE_RELEASE_SECONDS).
    let release_step = 1.0 / (0.03 * sample_rate);

    let render =
        |driver: DriverConfig, style: WaveguideStyle, f0: f32, release_at: Option<usize>| {
            let mut engine = ResonatorEngine::new(sample_rate);
            engine.configure(
                &ResonatorConfig::Waveguide(WaveguideConfig {
                    style,
                    ..WaveguideConfig::default()
                }),
                f0,
                true,
            );
            engine.set_driver(driver);
            let mut gate = 1.0_f32;
            (0..144_000)
                .map(|index| {
                    if release_at.is_some_and(|start| index >= start) {
                        gate = (gate - release_step).max(0.0);
                    }
                    engine.process_sample((index == 0) as u8 as f32, 0.0, 0.9, gate)
                })
                .collect::<Vec<_>>()
        };

    // Release at 1.0 s; measure the tail at 2.5–3.0 s, long after the gate is closed.
    for (driver, style, f0) in [
        (
            DriverConfig::Reed(ReedConfig::default()),
            WaveguideStyle::Tube,
            196.0,
        ),
        (
            DriverConfig::Bow(BowConfig::default()),
            WaveguideStyle::String,
            196.0,
        ),
    ] {
        let held = render(driver, style, f0, None);
        let released = render(driver, style, f0, Some(48_000));
        assert_all_finite(&released);
        let held_tail = rms(&held[120_000..144_000]);
        let released_tail = rms(&released[120_000..144_000]);
        assert!(
            released_tail < held_tail * 0.5,
            "{style:?} should ring out after note-off: held={held_tail}, released={released_tail}"
        );
    }
}

#[test]
fn oversampled_waveguide_engine_render_does_not_allocate() {
    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(
        &ResonatorConfig::Waveguide(WaveguideConfig::default()),
        220.0,
        true,
    );

    assert_no_allocations("waveguide_engine_render", || {
        for index in 0..512 {
            engine.process_sample((index == 0) as u8 as f32, 0.0, 0.0, 1.0);
        }
    });
}

#[test]
fn tension_modulated_engine_render_does_not_allocate() {
    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(
        &ResonatorConfig::Waveguide(WaveguideConfig::default()),
        220.0,
        true,
    );

    // A non-zero, varying measured energy exercises the per-sample tension
    // modulation path (M4) under the no-allocation contract (ADR-0001).
    assert_no_allocations("tension_engine_render", || {
        for index in 0..512 {
            let energy = 0.1 + 0.05 * (index as f32 * 0.05).sin();
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0, 1.0);
        }
    });
}

#[test]
fn steepening_modulated_tube_engine_render_does_not_allocate() {
    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(
        &ResonatorConfig::Waveguide(WaveguideConfig {
            style: WaveguideStyle::Tube,
            ..WaveguideConfig::default()
        }),
        196.0,
        true,
    );

    // A non-zero, varying measured energy exercises the Tube steepening +
    // bell-radiation path (M5) under the no-allocation contract (ADR-0001).
    assert_no_allocations("steepening_engine_render", || {
        for index in 0..512 {
            let energy = 0.1 + 0.05 * (index as f32 * 0.05).sin();
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0, 1.0);
        }
    });
}

#[test]
fn geometric_modulated_mesh_engine_render_does_not_allocate() {
    use crate::MeshConfig;

    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(&ResonatorConfig::Mesh(MeshConfig::default()), 220.0, true);

    // A non-zero, varying measured energy exercises the mesh geometric (von Kármán)
    // coupling path (M6) under the no-allocation contract (ADR-0001).
    assert_no_allocations("geometric_engine_render", || {
        for index in 0..512 {
            let energy = 0.1 + 0.05 * (index as f32 * 0.05).sin();
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0, 1.0);
        }
    });
}

fn render_string_with_spread(spread: f32, samples: usize) -> Vec<f32> {
    // Mirror the production path: the String core at 2x, driven through the
    // oversampler, with the M9 strike-position spread set on the params.
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: 220.0,
        loop_filter_cutoff: 11_000.0,
        loop_filter_resonance: 0.0,
        loop_gain: 0.99,
        loop_nonlinearity: 0.0,
        position_of_strike: 0.28,
        excitation_spread: spread,
        ..WaveguideParams::default()
    };
    let mut waveguide = WaveguideResonator::new(2.0 * 48_000.0, LOWEST_RESONATOR_FREQUENCY_HZ);
    let mut oversampler = Oversampler2x::new();
    render_response(
        48_000.0,
        220.0,
        samples,
        RenderExcitation::Impulse,
        |sample| oversampler.process(sample, |inner| waveguide.process_sample(inner, params)),
    )
}

#[test]
fn strike_position_spread_changes_timbre_picked_vs_strummed() {
    // A wide strum spreads the contact across the string, averaging out the
    // strike-position comb and emphasising different partials than a tight pick. The
    // two are a distinct, measurable timbre — the spectral centroid (a gain-invariant
    // measure) shifts substantially, so the difference cannot be explained by level.
    let picked = render_string_with_spread(0.0, 8_192);
    let strummed = render_string_with_spread(0.85, 8_192);
    assert_all_finite(&picked);
    assert_all_finite(&strummed);
    assert!(
        rms(&picked[..4_096]) > 0.0 && rms(&strummed[..4_096]) > 0.0,
        "produced silence"
    );

    let picked_centroid = audio_window_metrics(&picked[..4_096], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let strummed_centroid = audio_window_metrics(&strummed[..4_096], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let relative_shift = (strummed_centroid - picked_centroid).abs() / picked_centroid.max(1.0e-3);
    assert!(
        relative_shift > 0.15,
        "spread should produce a distinct timbre (gain-invariant centroid shift): \
         picked={picked_centroid} strummed={strummed_centroid} shift={relative_shift}"
    );
}

#[test]
fn contact_time_mellows_the_onset_on_the_sample_driver() {
    // The contact stage applies even with the Sample (pass-through) driver, where the
    // M8 PickConfig contact-time does nothing: a longer contact time spreads the
    // momentum transfer in time, low-passing the onset so the attack is measurably
    // darker. Gain-invariant centroid => timbral, not a level change.
    let render = |contact_time: f32| {
        let mut engine = ResonatorEngine::new(48_000.0);
        engine.configure(
            &ResonatorConfig::Waveguide(WaveguideConfig::default()),
            220.0,
            true,
        );
        engine.set_contact(ContactConfig {
            spread: 0.0,
            contact_time,
        });
        render_response(
            48_000.0,
            220.0,
            4_096,
            RenderExcitation::Impulse,
            |sample| engine.process_sample(sample, 0.0, 0.5, 1.0),
        )
    };
    let sharp = render(0.0);
    let mellow = render(0.9);
    assert_all_finite(&sharp);
    assert_all_finite(&mellow);
    let sharp_centroid = audio_window_metrics(&sharp[..1_024], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    let mellow_centroid = audio_window_metrics(&mellow[..1_024], 48_000.0)
        .spectral_centroid_hz
        .unwrap();
    assert!(
        mellow_centroid < sharp_centroid * 0.9,
        "longer contact should mellow the onset: sharp={sharp_centroid} mellow={mellow_centroid}"
    );
    // The contact stage shapes the onset, it does not mute the string.
    assert!(
        rms(&mellow[..2_048]) > 0.0,
        "contact stage produced silence"
    );
}

#[test]
fn contact_stage_render_does_not_allocate() {
    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(
        &ResonatorConfig::Waveguide(WaveguideConfig::default()),
        220.0,
        true,
    );
    // A non-default contact stage exercises both the contact-time low-pass and the
    // effort-widened spread path under the no-allocation contract (ADR-0001).
    engine.set_contact(ContactConfig {
        spread: 0.6,
        contact_time: 0.7,
    });
    assert_no_allocations("contact_stage_render", || {
        for index in 0..512 {
            engine.process_sample((index == 0) as u8 as f32, 0.0, 0.8, 1.0);
        }
    });
}

#[test]
fn strummed_string_render_does_not_allocate() {
    let params = WaveguideParams {
        style: WaveguideStyle::String,
        frequency_hz: 220.0,
        loop_gain: 0.99,
        position_of_strike: 0.28,
        excitation_spread: 0.7,
        ..WaveguideParams::default()
    };
    let mut waveguide = WaveguideResonator::new(2.0 * 48_000.0, LOWEST_RESONATOR_FREQUENCY_HZ);
    let mut oversampler = Oversampler2x::new();
    // Prime the caches and the wave buffer outside the asserted region.
    for index in 0..256 {
        oversampler.process((index == 0) as u8 as f32, |inner| {
            waveguide.process_sample(inner, params)
        });
    }
    // The widened-tap branch rebuilds a fixed `[PositionTap; 3]` on the stack each
    // sample; the prepared model is cached, so the spread path must not allocate.
    assert_no_allocations("strummed_string_render", || {
        for _ in 0..512 {
            oversampler.process(0.0, |inner| waveguide.process_sample(inner, params));
        }
    });
}

#[test]
fn source_body_balance_string_render_does_not_allocate() {
    let mut engine = ResonatorEngine::new(48_000.0);
    engine.configure(
        &ResonatorConfig::Waveguide(WaveguideConfig {
            source_body_balance: 0.7,
            ..WaveguideConfig::default()
        }),
        220.0,
        true,
    );
    // A non-zero, varying measured energy exercises the per-sample equal-power
    // source-body crossfade (M9) under the no-allocation contract (ADR-0001).
    assert_no_allocations("source_body_balance_render", || {
        for index in 0..512 {
            let energy = 0.1 + 0.05 * (index as f32 * 0.05).sin();
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0, 1.0);
        }
    });
}

#[test]
fn series_conditioner_recovers_from_non_finite_state_and_input() {
    let mut conditioner = SeriesConditioner::new(48_000.0);
    conditioner.fast_env = f32::NAN;
    conditioner.slow_env = f32::INFINITY;

    assert_eq!(conditioner.process_sample(f32::NAN), 0.0);
    assert!(conditioner.process_sample(0.25).is_finite());
}

#[test]
fn body_color_exciter_recovers_from_non_finite_state_and_input() {
    let mut exciter = BodyColorExciter::new(48_000.0);
    exciter.window_env = f32::NAN;
    exciter.trigger_peak = f32::INFINITY;

    assert_eq!(exciter.process_sample(f32::NAN, f32::NAN), 0.0);
    assert!(exciter.process_sample(0.5, 0.25).is_finite());
}
