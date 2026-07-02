use super::*;

pub(crate) fn rel_err(measured: f32, expected: f32) -> f32 {
    (measured - expected).abs() / expected.abs().max(f32::EPSILON)
}

#[test]
fn pure_plate_spacing_matches_closed_form() {
    // h = 2√(κk) when tension and σ₁ are zero.
    let h = min_stable_spacing(1.07, 0.0, 0.0, 48_000.0);
    let expected = 2.0 * (1.07_f32 / 48_000.0).sqrt() * STABILITY_MARGIN;
    assert!(rel_err(h, expected) < 1.0e-3, "h={h} expected={expected}");
}

#[test]
fn bronze_crash_grid_matches_hand_calc() {
    // Bronze-class 16" plate at 48 kHz (ADR-0050 anchor): h ≈ 9.4–9.5 mm, 43×43 active
    // cells, inside the allocation budget.
    let config = PlateConfig::default();
    let grid = PlateGrid::for_config(&config);
    assert!(
        (9.3e-3..9.6e-3).contains(&grid.spacing_m),
        "spacing={}",
        grid.spacing_m
    );
    assert_eq!((grid.width, grid.height), (43, 43));
    assert!(grid.cells() <= PLATE_MAX_CELLS);
}

#[test]
fn loss_calibration_round_trips_both_t60_targets() {
    let (kappa, c) = (1.07, 0.0);
    let low = DecayTarget {
        frequency_hz: 200.0,
        t60_s: 3.0,
    };
    let high = DecayTarget {
        frequency_hz: 4_000.0,
        t60_s: 0.8,
    };
    let (sigma0, sigma1) = losses_for(low, high, kappa, c);
    assert!(sigma0 > 0.0 && sigma1 > 0.0, "σ0={sigma0} σ1={sigma1}");
    for target in [low, high] {
        let omega = std::f32::consts::TAU * target.frequency_hz;
        let t60 = LN_1000 / decay_rate_for_omega(omega, sigma0, sigma1, kappa, c);
        assert!(
            rel_err(t60, target.t60_s) < 1.0e-3,
            "target {} Hz: t60={} expected={}",
            target.frequency_hz,
            t60,
            target.t60_s
        );
    }
}

#[test]
fn out_ringing_high_target_clamps_sigma1_to_zero() {
    // A high band asked to ring longer than the low band is unphysical for σ₀+σ₁ξ losses;
    // σ₁ clamps to zero and the low target governs.
    let low = DecayTarget {
        frequency_hz: 200.0,
        t60_s: 3.0,
    };
    let high = DecayTarget {
        frequency_hz: 4_000.0,
        t60_s: 5.0,
    };
    let (sigma0, sigma1) = losses_for(low, high, 1.07, 0.0);
    assert_eq!(sigma1, 0.0);
    assert!(rel_err(LN_1000 / sigma0, low.t60_s) < 1.0e-3, "σ0={sigma0}");
}

pub(crate) fn small_plate(boundary: PlateBoundary, sigma0: f32, sigma1: f32) -> PlateConfig {
    PlateConfig {
        length_x_m: 0.15,
        length_y_m: 0.15,
        kappa: 1.07,
        tension_speed: 0.0,
        tension_headroom_speed: 0.0,
        sigma0,
        sigma1,
        sample_rate: 48_000.0,
        boundary,
    }
}

/// Physical-scale strike force density (≈30 N stick force over one cell of a 1 mm bronze
/// plate): keeps the kernel's displacement state in a realistic magnitude band.
pub(crate) const TEST_FORCE: f32 = 5.0e4;

pub(crate) fn interior_taps(kernel: &mut PlateKernel) {
    let grid = kernel.grid();
    let strike = (grid.height / 3) * grid.width + grid.width / 3;
    let pickup = (2 * grid.height / 3) * grid.width + 2 * grid.width / 3;
    kernel.set_taps(strike, pickup);
}

fn xorshift(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[test]
fn impulse_on_simply_supported_plate_rings_finite_and_nonzero() {
    let mut kernel = PlateKernel::new(small_plate(PlateBoundary::SimplySupported, 1.0, 0.005));
    interior_taps(&mut kernel);
    let mut peak: f32 = 0.0;
    for n in 0..48_000 {
        let force = if n == 0 { TEST_FORCE } else { 0.0 };
        let out = kernel.process_sample(force);
        assert!(out.is_finite(), "non-finite output at sample {n}");
        peak = peak.max(out.abs());
    }
    assert!(peak > 1.0e-9, "plate is silent: peak={peak}");
    assert!(peak < 1.0, "implausible displacement: peak={peak}");
}

#[test]
fn lossless_plate_stays_bounded_at_the_stability_limit_grid() {
    // σ₀ = σ₁ = 0: the scheme is exactly lossless, so any update/boundary sign error or a
    // stability-condition error grows without bound. Drive with deterministic noise, then
    // ring for 10⁵ samples and bound the tail against the driven peak.
    for boundary in [PlateBoundary::SimplySupported, PlateBoundary::Clamped] {
        let mut kernel = PlateKernel::new(small_plate(boundary, 0.0, 0.0));
        interior_taps(&mut kernel);
        let mut seed = 0x1d2e_3f47_u32;
        let mut reference_peak: f32 = 0.0;
        for _ in 0..10_000 {
            let out = kernel.process_sample(xorshift(&mut seed) * TEST_FORCE);
            assert!(out.is_finite());
            reference_peak = reference_peak.max(out.abs());
        }
        assert!(reference_peak > 0.0, "{boundary:?}: no response to drive");
        let mut tail_peak: f32 = 0.0;
        for n in 0..100_000 {
            let out = kernel.process_sample(0.0);
            assert!(
                out.is_finite(),
                "{boundary:?}: non-finite at tail sample {n}"
            );
            tail_peak = tail_peak.max(out.abs());
        }
        assert!(
            tail_peak < 50.0 * reference_peak,
            "{boundary:?}: lossless ring grew (tail={tail_peak}, driven={reference_peak})"
        );
    }
}

#[test]
fn sigma0_only_decay_matches_analytic_t60() {
    // Frequency-independent σ₀ decays every mode at the same rate, so the broadband RMS
    // envelope is a clean e^{−σ₀t}: T60 measured from two windows must match ln(1000)/σ₀.
    let target_t60 = 0.5_f32;
    let sigma0 = LN_1000 / target_t60;
    let mut kernel = PlateKernel::new(small_plate(PlateBoundary::SimplySupported, sigma0, 0.0));
    interior_taps(&mut kernel);
    let mut output = Vec::with_capacity(28_800);
    for n in 0..28_800 {
        let force = if n == 0 { TEST_FORCE } else { 0.0 };
        output.push(kernel.process_sample(force));
    }
    let rms = |window: &[f32]| -> f32 {
        (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt()
    };
    let early = rms(&output[2_400..4_800]);
    let late = rms(&output[19_200..21_600]);
    assert!(early > 0.0 && late > 0.0, "early={early} late={late}");
    let drop_db = 20.0 * (early / late).log10();
    let dt = (19_200.0 + 21_600.0 - 2_400.0 - 4_800.0) / 2.0 / 48_000.0;
    let measured_t60 = 60.0 * dt / drop_db;
    assert!(
        rel_err(measured_t60, target_t60) < 0.10,
        "measured T60={measured_t60} target={target_t60}"
    );
}

pub(crate) fn goertzel_magnitude(samples: &[f32], frequency_hz: f32, sample_rate: f32) -> f32 {
    let omega = std::f32::consts::TAU * frequency_hz / sample_rate;
    let coeff = 2.0 * omega.cos();
    let (mut s1, mut s2) = (0.0_f32, 0.0_f32);
    for &sample in samples {
        let s0 = sample + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
}

#[test]
fn free_tensioned_crash_scale_voicing_decays() {
    // Regression for the free-edge discretization choice: the centered-ghost form of the
    // classical moment/shear conditions left exactly this voicing (59×48, c = 70.4, σ from
    // the damping map) on a defective marginal boundary eigenvalue — ×11-per-0.25 s growth
    // that flipped on 1e-7 coefficient perturbations. The variational (Neumann
    // graph-Laplacian) free boundary is dissipative by construction, so this voicing must
    // decay toward its (σ₀-damped) rigid offset.
    let config = PlateConfig {
        length_x_m: 0.625,
        length_y_m: 0.5125,
        kappa: 1.07,
        tension_speed: 70.4,
        tension_headroom_speed: 0.0,
        sigma0: 2.125_599_1,
        sigma1: 0.000_375_522_04,
        sample_rate: 48_000.0,
        boundary: PlateBoundary::Free,
    };
    let mut kernel = PlateKernel::new(config);
    interior_taps(&mut kernel);
    let mut peaks = [0.0_f32; 6];
    for (block, peak) in peaks.iter_mut().enumerate() {
        for i in 0..12_000 {
            let n = block * 12_000 + i;
            let out = kernel.process_sample(if n == 0 { TEST_FORCE } else { 0.0 });
            assert!(out.is_finite(), "non-finite at block {block}");
            *peak = peak.max(out.abs());
        }
    }
    // With the rigid offset gauge-fixed away, the displacement tap decays cleanly; the
    // regression (a defective marginal root tipped past 1 by coefficient rounding) grew
    // ×11 per block instead.
    assert!(
        peaks[5] < peaks[0] && peaks[5] < 1.0e-2,
        "tensioned free crash voicing grew: peaks={peaks:?}"
    );
}

#[test]
fn lossless_free_plate_with_tension_stays_bounded() {
    // A tensioned free plate must stay bounded lossless: the tension traction at free
    // edges is intrinsic to the variational (natural-BC) operator, so the membrane term
    // cannot pump energy at the boundary (the original Crash-voicing divergence).
    let mut config = small_plate(PlateBoundary::Free, 0.0, 0.0);
    config.tension_speed = 70.0;
    let mut kernel = PlateKernel::new(config);
    interior_taps(&mut kernel);
    let mut seed = 0x42aa_19d3_u32;
    let mut reference_peak: f32 = 0.0;
    for _ in 0..5_000 {
        let force = xorshift(&mut seed) * TEST_FORCE;
        for signed in [force, -force] {
            let out = kernel.process_sample(signed);
            assert!(out.is_finite());
            reference_peak = reference_peak.max(out.abs());
        }
    }
    let mut tail_peak: f32 = 0.0;
    for n in 0..100_000 {
        let out = kernel.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at tail sample {n}");
        tail_peak = tail_peak.max(out.abs());
    }
    assert!(
        tail_peak < 50.0 * reference_peak,
        "lossless tensioned free ring grew (tail={tail_peak}, driven={reference_peak})"
    );
}

#[test]
fn lossless_free_plate_stays_bounded_at_the_stability_limit_grid() {
    // Free edges are the risky discretization (ADR-0050): an energy-inconsistent ghost
    // condition blows up a lossless run quickly. Drive with momentum-paired forces (+f, −f)
    // so the free plate's rigid-body translation mode ends the drive with zero net
    // momentum — otherwise unbounded displacement drift is physics, not instability.
    let mut kernel = PlateKernel::new(small_plate(PlateBoundary::Free, 0.0, 0.0));
    interior_taps(&mut kernel);
    let mut seed = 0x7a3d_91c5_u32;
    let mut reference_peak: f32 = 0.0;
    for _ in 0..5_000 {
        let force = xorshift(&mut seed) * TEST_FORCE;
        for signed in [force, -force] {
            let out = kernel.process_sample(signed);
            assert!(out.is_finite());
            reference_peak = reference_peak.max(out.abs());
        }
    }
    assert!(reference_peak > 0.0, "no response to drive");
    let mut tail_peak: f32 = 0.0;
    for n in 0..100_000 {
        let out = kernel.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at tail sample {n}");
        tail_peak = tail_peak.max(out.abs());
    }
    assert!(
        tail_peak < 50.0 * reference_peak,
        "lossless free ring grew (tail={tail_peak}, driven={reference_peak})"
    );
}

#[test]
fn free_plate_impulse_drift_is_damped_by_sigma0() {
    // An impulse leaves a free plate with net momentum; σ₀ damps the rigid-body velocity so
    // the late field is a (bounded) constant offset with vanishing motion.
    let sigma0 = LN_1000 / 0.5;
    let mut kernel = PlateKernel::new(small_plate(PlateBoundary::Free, sigma0, 0.0));
    interior_taps(&mut kernel);
    let mut output = Vec::with_capacity(48_000);
    for n in 0..48_000 {
        let out = kernel.process_sample(if n == 0 { TEST_FORCE } else { 0.0 });
        assert!(out.is_finite());
        assert!(
            out.abs() < 1.0,
            "implausible displacement {out} at sample {n}"
        );
        output.push(out);
    }
    let velocity_rms = |range: std::ops::Range<usize>| -> f32 {
        let window: Vec<f32> = output[range.clone()]
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect();
        (window.iter().map(|v| v * v).sum::<f32>() / window.len() as f32).sqrt()
    };
    let early = velocity_rms(2_400..7_200);
    let late = velocity_rms(40_800..45_600);
    assert!(early > 0.0, "plate never moved");
    // > 34 dB of velocity decay: σ₀ damps the rigid velocity and the ring, but the late
    // readings sit on the f32 quantization floor of a state holding the (physical,
    // constant) rigid displacement offset, so demanding the analytic 60 dB+ over-asserts.
    assert!(
        late < 2.0e-2 * early,
        "rigid-body/ring motion not damped: early={early} late={late}"
    );
}

#[test]
fn free_plate_has_spectral_energy_below_the_simply_supported_fundamental() {
    // Physical ordering: the lowest free elastic mode of a square plate (λ² ≈ 13.5, nodal
    // lines on the diagonals) sits well below the simply-supported fundamental (λ² = 2π²).
    // Taps are placed off the diagonals and centerlines so that mode is excited and heard.
    let config = small_plate(PlateBoundary::Free, LN_1000 / 2.0, 0.0);
    let mut kernel = PlateKernel::new(config);
    let grid = kernel.grid();
    let strike = (grid.height / 5) * grid.width + grid.width / 3;
    let pickup = (grid.height / 3) * grid.width + (7 * grid.width) / 10;
    kernel.set_taps(strike, pickup);
    let mut output = Vec::with_capacity(52_800);
    for n in 0..52_800 {
        output.push(kernel.process_sample(if n == 0 { TEST_FORCE } else { 0.0 }));
    }
    let ring = &output[4_800..52_800];
    let f11_ss = mode_frequency_hz(1, 1, &config, &grid);
    let scan = |low: f32, high: f32| -> f32 {
        let mut best = 0.0_f32;
        let mut f = low;
        while f <= high {
            best = best.max(goertzel_magnitude(ring, f, config.sample_rate));
            f += 5.0;
        }
        best
    };
    let below = scan(40.0, 0.9 * f11_ss);
    let overall = scan(40.0, 1.5 * f11_ss);
    assert!(overall > 0.0, "free ring is spectrally empty");
    assert!(
        below > 0.25 * overall,
        "no free mode below the SS fundamental ({} Hz): below={below} overall={overall}",
        f11_ss
    );
}

#[test]
fn plate_process_and_retune_do_not_allocate() {
    let mut kernel = PlateKernel::new(small_plate(PlateBoundary::SimplySupported, 1.0, 0.005));
    interior_taps(&mut kernel);
    let mut larger = small_plate(PlateBoundary::Clamped, 2.0, 0.01);
    larger.length_x_m = 0.3;
    larger.length_y_m = 0.25;
    lindelion_test_allocator::assert_no_allocations("plate_process_and_retune", || {
        let mut sink = 0.0;
        for n in 0..512 {
            sink += kernel.process_sample(if n == 0 { TEST_FORCE } else { 0.0 });
        }
        kernel.reconfigure(larger);
        for _ in 0..512 {
            sink += kernel.process_sample(0.0);
        }
        assert!(sink.is_finite());
    });
}

#[test]
fn degenerate_configs_sanitize_without_panic() {
    for bad in [f32::NAN, f32::INFINITY, -3.0, 0.0] {
        let config = PlateConfig {
            length_x_m: bad,
            length_y_m: bad,
            kappa: bad,
            tension_speed: bad,
            tension_headroom_speed: bad,
            sigma0: bad,
            sigma1: bad,
            sample_rate: bad,
            boundary: PlateBoundary::Free,
        };
        let sane = config.sanitized();
        assert!(sane.length_x_m.is_finite() && sane.kappa.is_finite());
        let grid = PlateGrid::for_config(&config);
        assert!((PLATE_MIN_DIM..=PLATE_MAX_WIDTH).contains(&grid.width));
        assert!((PLATE_MIN_DIM..=PLATE_MAX_HEIGHT).contains(&grid.height));
        assert!(grid.spacing_m.is_finite() && grid.spacing_m > 0.0);
    }
}

#[test]
fn tension_headroom_reserves_modulation_budget() {
    // M3 step 1: the grid is sized for c²_design = c²_voice + headroom², and the
    // closed-form inverse of the stability quadratic certifies that budget at the sized
    // grid. Round-trip: a grid sized for a design speed admits at least that c².
    let (kappa, sigma1, fs) = (1.07_f32, 3.0e-4_f32, 48_000.0_f32);
    let design_speed = 140.0_f32;
    let h = min_stable_spacing(kappa, design_speed, sigma1, fs);
    let c2_max = max_stable_tension_sq(h, kappa, sigma1, fs);
    assert!(
        c2_max >= design_speed * design_speed,
        "sized grid must admit its design tension: c2_max={c2_max}"
    );
    assert!(
        c2_max < design_speed * design_speed * 1.2,
        "inverse drifted: c2_max={c2_max}"
    );

    let voice = PlateConfig {
        tension_speed: 60.0,
        ..PlateConfig::default()
    };
    let with_headroom = PlateConfig {
        tension_headroom_speed: 120.0,
        ..voice
    };
    let bare_grid = PlateGrid::for_config(&voice);
    let head_grid = PlateGrid::for_config(&with_headroom);
    assert!(
        head_grid.spacing_m >= bare_grid.spacing_m,
        "headroom must not shrink h"
    );
    let budget = max_stable_tension_sq(head_grid.spacing_m, 1.07, voice.sigma1, 48_000.0);
    assert!(
        budget >= 60.0 * 60.0 + 120.0 * 120.0,
        "headroom budget not honored: budget={budget}"
    );
}

#[test]
fn bend_energy_bus_tracks_the_strike() {
    // M3 step 2: the plate-internal energy bus (E_bend = −Σu·Lu, the discrete ∫|∇u|²)
    // rises with a strike and decays with the ring — the bloom's drive source is the
    // plate's own state, never output level (ADR-0050).
    let mut config = small_plate(PlateBoundary::Free, LN_1000 / 1.0, 3.0e-4);
    config.tension_speed = 40.0;
    config.tension_headroom_speed = 120.0;
    let mut kernel = PlateKernel::new(config);
    interior_taps(&mut kernel);
    assert_eq!(kernel.bend_energy(), 0.0);
    for n in 0..2_400 {
        kernel.process_sample(if n < 22 { TEST_FORCE } else { 0.0 });
    }
    let early = kernel.bend_energy();
    assert!(early > 0.0, "strike must raise the bend energy");
    for _ in 0..48_000 {
        kernel.process_sample(0.0);
    }
    let late = kernel.bend_energy();
    assert!(
        late < early * 0.2,
        "bend energy must decay with the ring: early={early} late={late}"
    );

    // With a normalization set, the drive engages and is clamped to [0, 1].
    let mut kernel = PlateKernel::new(config);
    interior_taps(&mut kernel);
    kernel.set_drive_normalization(1.0e9);
    for n in 0..2_400 {
        kernel.process_sample(if n < 22 { TEST_FORCE } else { 0.0 });
    }
    let drive = kernel.tension_drive();
    assert!((0.0..=1.0).contains(&drive) && drive > 0.0, "drive={drive}");
}

#[test]
fn lossless_free_tensioned_plate_with_full_bloom_drive_stays_bounded() {
    // The modulation clamp is the referee: at full drive c²_eff pins at the closed-form
    // stability budget for the actual grid; lossless boundedness must hold there.
    let mut config = small_plate(PlateBoundary::Free, 0.0, 0.0);
    config.tension_speed = 40.0;
    config.tension_headroom_speed = 120.0;
    let mut kernel = PlateKernel::new(config);
    interior_taps(&mut kernel);
    kernel.set_drive_normalization(1.0e12);
    let mut seed = 0x5ca1_ab1e_u32;
    let mut reference_peak: f32 = 0.0;
    for _ in 0..5_000 {
        let force = xorshift(&mut seed) * TEST_FORCE;
        for signed in [force, -force] {
            let out = kernel.process_sample(signed);
            assert!(out.is_finite());
            reference_peak = reference_peak.max(out.abs());
        }
    }
    let mut tail_peak: f32 = 0.0;
    for n in 0..100_000 {
        let out = kernel.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at tail sample {n}");
        tail_peak = tail_peak.max(out.abs());
    }
    assert!(
        tail_peak < 50.0 * reference_peak,
        "full-drive lossless ring grew (tail={tail_peak}, driven={reference_peak})"
    );
}
