use super::*;
use crate::assert_no_allocations;
use crate::dsp::render_metrics::{RenderExcitation, render_response};
use crate::dsp::waveguide::WaveguideParams;
use crate::{DriverConfig, PickConfig, ReedConfig, WaveguideStyle};
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
    // input is inert: the waveguide receives exactly the excitation it did before
    // the M8 driver seam. A render at zero effort and at full effort must therefore
    // be bit-identical — the seam adds nothing to the pre-driver signal path.
    for style in [WaveguideStyle::String, WaveguideStyle::Tube] {
        let config = ResonatorConfig::Waveguide(WaveguideConfig {
            style,
            ..WaveguideConfig::default()
        });
        let render = |effort: f32| {
            let mut engine = ResonatorEngine::new(48_000.0);
            engine.configure(&config, 220.0, true);
            (0..4_096)
                .map(|index| engine.process_sample((index == 0) as u8 as f32, 0.0, effort))
                .collect::<Vec<_>>()
        };
        let quiet = render(0.0);
        let hard = render(1.0);
        assert_all_finite(&quiet);
        assert!(rms(&quiet[..2_048]) > 0.0, "{style:?} produced silence");
        assert_eq!(quiet, hard, "{style:?} pass-through driver leaked effort");
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
            |sample| engine.process_sample(sample, 0.0, effort),
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
            |sample| engine.process_sample(sample, 0.0, effort),
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
            engine.process_sample((index == 0) as u8 as f32, 0.0, 0.0);
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
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0);
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
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0);
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
            engine.process_sample((index == 0) as u8 as f32, energy, 0.0);
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
