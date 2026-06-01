use super::*;
use crate::dsp::render_metrics::{RenderExcitation, render_metric_profile, render_response};
use lindelion_dsp_utils::analysis::{
    assert_all_finite, audio_window_metrics, rms_difference, sampled_high_frequency_ratio,
};

#[test]
fn geometric_coupling_spreads_energy_upward_with_drive() {
    // A gong-like mesh: low-loss free boundary so a hard strike rings and the
    // geometric coupling can spread its energy upward into higher modes.
    let config = RectangularMesh2dConfig {
        boundary: MeshBoundaryConfig::free(0.02),
        ..RectangularMesh2dConfig::default()
    };
    let mode = mode_frequency(config);

    let centroid = |drive: f32| {
        let output = render_mesh_with_drive(config, 24_000, move |_| drive);
        audio_window_metrics(&output[2_000..10_000], config.sample_rate)
            .spectral_centroid_hz
            .unwrap()
    };
    let high_frequency_ratio = |drive: f32| {
        let output = render_mesh_with_drive(config, 24_000, move |_| drive);
        sampled_high_frequency_ratio(&output[2_000..10_000], config.sample_rate, mode * 3.0, mode)
    };

    // Upward energy spread rises monotonically with drive across the bloom's
    // onset (the geometric coupling steers more energy into higher modes as
    // playing energy rises — the gong bloom), then plateaus at full coupling.
    // Energies straddle the recalibrated `GEOMETRIC_ENERGY_REF` (~0.013, the real
    // per-voice bus level): `mid` partway up the squared curve, `loud` near a hard
    // strike's full coupling.
    let quiet = centroid(0.0);
    let mid = centroid(0.007);
    let loud = centroid(0.013);
    assert!(
        quiet < mid && mid < loud,
        "centroid not monotone: quiet={quiet} mid={mid} loud={loud}"
    );
    assert!(
        loud > quiet * 2.0,
        "upward spread too small: quiet={quiet} loud={loud}"
    );
    // Much more high-frequency (higher-mode) energy at high drive.
    assert!(
        high_frequency_ratio(0.013) > high_frequency_ratio(0.0) + 0.2,
        "high-frequency ratio did not rise with drive"
    );
}

#[test]
fn geometric_coupling_is_energy_stable_and_bounded_under_extreme_drive() {
    // Energy-conserving under drive: a lossless mesh at full geometric coupling
    // must not gain energy (the junction rotation preserves per-junction energy,
    // so the scheme only ever redistributes energy, never injects it).
    let mut mesh = RectangularMesh2d::new(RectangularMesh2dConfig {
        boundary: MeshBoundaryConfig::free(0.0),
        ..RectangularMesh2dConfig::default()
    });
    mesh.set_geometric_drive(0.3);
    mesh.process_sample(1.0);
    let initial_energy = mesh.total_energy();
    for _ in 0..512 {
        mesh.set_geometric_drive(0.3);
        mesh.process_sample(0.0);
        assert!(
            mesh.total_energy() <= initial_energy * 1.01,
            "energy grew: initial={initial_energy}, current={}",
            mesh.total_energy()
        );
    }

    // Extreme, rapidly-changing, and non-finite drive across configs stays
    // finite and bounded.
    for &damping in &[0.02_f32, 0.4, 0.16] {
        let config = RectangularMesh2dConfig {
            boundary: MeshBoundaryConfig::fixed(damping),
            ..RectangularMesh2dConfig::default()
        };
        let output = render_mesh_with_drive(config, 16_000, |index| match index % 5 {
            0 => 1_000.0,
            1 => f32::NAN,
            2 => f32::INFINITY,
            3 => -5.0,
            _ => (index as f32 * 0.01).sin() * 50.0,
        });
        assert_all_finite(&output);
        assert!(
            audio_window_metrics(&output, config.sample_rate).peak_abs < 8.0,
            "damping={damping} peak too high"
        );
    }
}

#[test]
fn rectangular_mesh_renders_finite_decaying_audio() {
    let config = RectangularMesh2dConfig::default();
    let output = render_mesh(config, 24_000, RenderExcitation::ShapedPluck);
    let profile = render_metric_profile(&output, config.sample_rate, mode_frequency(config));

    assert_all_finite(&output);
    assert!(profile.early.rms > 1.0e-8, "profile={profile:?}");
    assert!(
        profile.late.rms < profile.early.rms * 0.8,
        "profile={profile:?}"
    );
    assert!(profile.harmonic_decay.len() >= 4);
}

#[test]
fn lossless_boundary_scattering_is_passive_without_new_excitation() {
    let mut mesh = RectangularMesh2d::new(RectangularMesh2dConfig {
        boundary: MeshBoundaryConfig::free(0.0),
        ..RectangularMesh2dConfig::default()
    });
    mesh.process_sample(1.0);
    let initial_energy = mesh.total_energy();

    for _ in 0..256 {
        mesh.process_sample(0.0);
        assert!(
            mesh.total_energy() <= initial_energy * 1.000_5,
            "initial={}, current={}",
            initial_energy,
            mesh.total_energy()
        );
    }
}

#[test]
fn boundary_loss_and_asymmetry_change_the_render() {
    let lossless = render_mesh(
        RectangularMesh2dConfig {
            boundary: MeshBoundaryConfig::fixed(0.0),
            ..RectangularMesh2dConfig::default()
        },
        18_000,
        RenderExcitation::Impulse,
    );
    let lossy = render_mesh(
        RectangularMesh2dConfig::default(),
        18_000,
        RenderExcitation::Impulse,
    );
    let asymmetric = render_mesh(
        RectangularMesh2dConfig {
            boundary: MeshBoundaryConfig::fixed_edges(0.45, 0.04, 0.16, 0.28),
            ..RectangularMesh2dConfig::default()
        },
        18_000,
        RenderExcitation::Impulse,
    );

    assert_all_finite(&lossless);
    assert_all_finite(&lossy);
    assert_all_finite(&asymmetric);
    assert!(rms_difference(&lossless[4_096..], &lossy[4_096..]) > 1.0e-6);
    assert!(rms_difference(&lossy[512..], &asymmetric[512..]) > 1.0e-6);
}

#[test]
fn strike_and_pickup_positions_change_mesh_response() {
    let center_strike = render_mesh(
        RectangularMesh2dConfig {
            strike_position: MeshPoint::new(0.5, 0.5),
            pickup_position: MeshPoint::new(0.72, 0.58),
            ..RectangularMesh2dConfig::default()
        },
        12_000,
        RenderExcitation::NoiseBurst,
    );
    let off_axis_strike = render_mesh(
        RectangularMesh2dConfig {
            strike_position: MeshPoint::new(0.18, 0.73),
            pickup_position: MeshPoint::new(0.28, 0.24),
            ..RectangularMesh2dConfig::default()
        },
        12_000,
        RenderExcitation::NoiseBurst,
    );

    assert_all_finite(&center_strike);
    assert_all_finite(&off_axis_strike);
    assert!(rms_difference(&center_strike[512..], &off_axis_strike[512..]) > 1.0e-5);
}

#[test]
fn wave_speed_controls_reported_physical_mode_frequency() {
    let slow = RectangularMesh2d::new(RectangularMesh2dConfig {
        wave_speed_mps: 180.0,
        ..RectangularMesh2dConfig::default()
    });
    let fast = RectangularMesh2d::new(RectangularMesh2dConfig {
        wave_speed_mps: 360.0,
        ..RectangularMesh2dConfig::default()
    });

    assert!((fast.mode_frequency_hz(1, 1) / slow.mode_frequency_hz(1, 1) - 2.0).abs() < 0.01);
}

#[test]
fn reset_clears_mesh_state() {
    let config = RectangularMesh2dConfig::default();
    let mut mesh = RectangularMesh2d::new(config);
    let _ = render_response(
        config.sample_rate,
        mode_frequency(config),
        2_048,
        RenderExcitation::Impulse,
        |sample| mesh.process_sample(sample),
    );
    mesh.reset();

    let output = (0..512)
        .map(|_| mesh.process_sample(0.0))
        .collect::<Vec<_>>();

    assert_all_finite(&output);
    assert!(output.iter().all(|sample| sample.abs() < 1.0e-8));
}
