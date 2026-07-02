//! Spectral guards for the plate kernel: mode placement against the simply-supported
//! analytic anchor and the two-target T60 loss-law calibration (cheap in-ci versions plus
//! the integration-gated full-scale sweep).

use super::tests::{TEST_FORCE, goertzel_magnitude, rel_err, small_plate};
use super::*;

/// Render `samples` of impulse response with the strike/pickup at the grid centre offset
/// slightly so odd-odd modes dominate but the pickup is not exactly on a node line.
fn render_center_impulse(kernel: &mut PlateKernel, samples: usize) -> Vec<f32> {
    let grid = kernel.grid();
    let center = (grid.height / 2) * grid.width + grid.width / 2;
    kernel.set_taps(center, center + grid.width + 1);
    let mut output = Vec::with_capacity(samples);
    for n in 0..samples {
        output.push(kernel.process_sample(if n == 0 { TEST_FORCE } else { 0.0 }));
    }
    output
}

/// Frequency of the strongest spectral line in `[low, high]` Hz (1 Hz scan).
fn dominant_frequency(samples: &[f32], low: f32, high: f32, sample_rate: f32) -> f32 {
    let mut best = (low, 0.0_f32);
    let mut f = low;
    while f <= high {
        let mag = goertzel_magnitude(samples, f, sample_rate);
        if mag > best.1 {
            best = (f, mag);
        }
        f += 1.0;
    }
    best.0
}

/// Narrowband T60 at `frequency_hz` from two Goertzel windows of the decaying ring. The
/// windows are Hann-weighted: a fast-decaying high mode is measured 60+ dB under a
/// still-loud low mode, and a rectangular window's sidelobe leakage from that line would
/// floor the late reading.
fn measure_t60_at(
    output: &[f32],
    frequency_hz: f32,
    sample_rate: f32,
    early_start: usize,
    late_start: usize,
    window: usize,
) -> f32 {
    let hann = |slice: &[f32]| -> Vec<f32> {
        let len = slice.len() as f32;
        slice
            .iter()
            .enumerate()
            .map(|(n, &s)| {
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / len).cos();
                s * w
            })
            .collect()
    };
    let early = goertzel_magnitude(
        &hann(&output[early_start..early_start + window]),
        frequency_hz,
        sample_rate,
    );
    let late = goertzel_magnitude(
        &hann(&output[late_start..late_start + window]),
        frequency_hz,
        sample_rate,
    );
    assert!(early > 0.0 && late > 0.0, "empty band at {frequency_hz} Hz");
    let drop_db = 20.0 * (early / late).log10();
    let dt = (late_start as f32 - early_start as f32) / sample_rate;
    60.0 * dt / drop_db
}

#[test]
fn simply_supported_fundamental_matches_the_analytic_mode_frequency() {
    // The dispersion anchor: the measured (1,1) line must sit on `mode_frequency_hz(1,1)`.
    // A constant error in κ scaling, `h`, or the stencil moves it and fails this.
    let config = small_plate(PlateBoundary::SimplySupported, LN_1000 / 2.0, 0.0);
    let mut kernel = PlateKernel::new(config);
    let grid = kernel.grid();
    let output = render_center_impulse(&mut kernel, 57_600);
    let ring = &output[9_600..57_600];
    let f11 = mode_frequency_hz(1, 1, &config, &grid);
    let measured = dominant_frequency(ring, 0.7 * f11, 1.3 * f11, config.sample_rate);
    assert!(
        rel_err(measured, f11) < 0.03,
        "measured fundamental {measured} Hz, analytic {f11} Hz"
    );
}

#[test]
fn calibrated_losses_hit_both_t60_targets_in_render() {
    // End-to-end loss law: calibrate (σ₀, σ₁) for two targets at the (1,1) and (3,1)
    // modes, render, and measure both narrowband decays.
    let base = small_plate(PlateBoundary::SimplySupported, 0.0, 0.0);
    let grid = PlateGrid::for_config(&base);
    let f11 = mode_frequency_hz(1, 1, &base, &grid);
    let f31 = mode_frequency_hz(3, 1, &base, &grid);
    let low = DecayTarget {
        frequency_hz: f11,
        t60_s: 1.2,
    };
    let high = DecayTarget {
        frequency_hz: f31,
        t60_s: 0.4,
    };
    let (sigma0, sigma1) = losses_for(low, high, base.kappa, base.tension_speed);
    let mut config = base;
    config.sigma0 = sigma0;
    config.sigma1 = sigma1;
    let mut kernel = PlateKernel::new(config);
    let output = render_center_impulse(&mut kernel, 43_200);
    for target in [low, high] {
        let measured = measure_t60_at(
            &output,
            target.frequency_hz,
            48_000.0,
            4_800,
            28_800,
            12_000,
        );
        assert!(
            rel_err(measured, target.t60_s) < 0.15,
            "{} Hz: measured T60={measured} target={}",
            target.frequency_hz,
            target.t60_s
        );
    }
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "multi-second full-scale decay sweep; run via make test-integration"
)]
#[test]
fn full_scale_free_plate_decay_follows_the_loss_law() {
    // Crash-scale free bronze plate: find the strongest real modes near two probe bands and
    // check their measured decay against the calibrated σ(ω) law (free plates have no
    // closed-form modes, so the law — not a fixed target — is the reference).
    let low = DecayTarget {
        frequency_hz: 250.0,
        t60_s: 3.0,
    };
    let high = DecayTarget {
        frequency_hz: 4_000.0,
        t60_s: 0.8,
    };
    let base = PlateConfig::default();
    let (sigma0, sigma1) = losses_for(low, high, base.kappa, base.tension_speed);
    let config = PlateConfig {
        sigma0,
        sigma1,
        ..base
    };
    let mut kernel = PlateKernel::new(config);
    let grid = kernel.grid();
    let strike = (grid.height / 5) * grid.width + grid.width / 3;
    let pickup = (grid.height / 3) * grid.width + (7 * grid.width) / 10;
    kernel.set_taps(strike, pickup);
    let samples = 4 * 48_000;
    let mut displacement = Vec::with_capacity(samples);
    for n in 0..samples {
        displacement.push(kernel.process_sample(if n == 0 { TEST_FORCE } else { 0.0 }));
    }
    // Analyze the differenced (velocity) signal: the free plate's rigid displacement
    // offset and its settling transient leak a 1/f skirt across the low band and corrupt
    // both the line scan and the late-window decay reading; differencing removes DC
    // exactly and leaves the modal lines (scaled by omega, irrelevant to decay rates).
    let output: Vec<f32> = displacement
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    // The low probe band is wide: the free plate's strong low lines move with the edge
    // discretization, and a narrow band can land between lines and read the f32 noise
    // floor as a bogus long decay (every mode of this system decays at >= sigma0 by
    // construction, so T60 readings far above ln(1000)/sigma0 are measurement artifacts).
    for (band_low, band_high) in [(60.0, 400.0), (3_500.0, 4_500.0)] {
        let early_ring = &output[4_800..52_800];
        let mode = dominant_frequency(early_ring, band_low, band_high, config.sample_rate);
        let analytic = LN_1000
            / decay_rate_for_omega(
                std::f32::consts::TAU * mode,
                sigma0,
                sigma1,
                config.kappa,
                config.tension_speed,
            );
        // Multi-window regression on band energy: a point pickup on a nearly-square free
        // plate sees near-degenerate mode pairs beat, so two-instant single-line readings
        // fluctuate wildly; fitting log band energy across several windows rides through
        // the beats. Windows span ~28 dB of expected decay, inside the f32 floor of a
        // state carrying the rigid DC offset.
        let window = 12_000usize;
        let count = ((analytic * (28.0 / 60.0) * 48_000.0) as usize / window).clamp(3, 7);
        let band_energy = |start: usize| -> f32 {
            let hann: Vec<f32> = output[start..start + window]
                .iter()
                .enumerate()
                .map(|(n, &v)| {
                    let w = 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / window as f32).cos();
                    v * w
                })
                .collect();
            (-3..=3)
                .map(|b| {
                    let mag = goertzel_magnitude(&hann, mode + b as f32 * 8.0, config.sample_rate);
                    mag * mag
                })
                .sum::<f32>()
        };
        let log_energies: Vec<f32> = (0..count)
            .map(|w| band_energy(4_800 + w * window).max(1.0e-30).ln())
            .collect();
        // Least-squares slope of ln(energy) per window -> decay rate sigma = -slope/(2*dt).
        let n = count as f32;
        let mean_x = (n - 1.0) / 2.0;
        let mean_y = log_energies.iter().sum::<f32>() / n;
        let (mut num, mut den) = (0.0_f32, 0.0_f32);
        for (i, y) in log_energies.iter().enumerate() {
            let dx = i as f32 - mean_x;
            num += dx * (y - mean_y);
            den += dx * dx;
        }
        let slope = num / den.max(f32::EPSILON);
        let dt = window as f32 / config.sample_rate;
        let measured = LN_1000 / (-slope / (2.0 * dt)).max(1.0e-6);
        assert!(
            rel_err(measured, analytic) < 0.30,
            "mode {mode} Hz: measured T60={measured} law={analytic} log_energies={log_energies:?}"
        );
    }
}
