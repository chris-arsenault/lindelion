use super::*;
use lindelion_dsp_utils::analysis::{
    assert_all_finite, peak_abs, rms, spectral_centroid_trajectory,
};

/// Closed-form -60 dB ring time the `damping` control resolves to (at the default
/// grid), inverting `boundary_damping_loss`: `T60 = K / -ln(1 - loss)`.
fn mesh_t60_seconds(sample_rate: f32, control: f32) -> f32 {
    let (w, h) = (RUNTIME_MESH_WIDTH, RUNTIME_MESH_HEIGHT);
    let loss = boundary_damping_loss(sample_rate, control, w, h);
    mesh_decay_k(sample_rate, w, h) / -(1.0 - loss).ln()
}

fn midi_note_hz(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

fn span(values: impl IntoIterator<Item = f32>) -> f32 {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for value in values {
        min = min.min(value);
        max = max.max(value);
    }
    max - min
}

fn point_distance(a: MeshPoint, b: MeshPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn combined_position_zones(points: &[(MeshPoint, MeshPoint)]) -> usize {
    let mut zones = Vec::new();
    for &(strike, pickup) in points {
        let zone = (
            (strike.x * 20.0).round() as i32,
            (strike.y * 20.0).round() as i32,
            (pickup.x * 20.0).round() as i32,
            (pickup.y * 20.0).round() as i32,
        );
        if !zones.contains(&zone) {
            zones.push(zone);
        }
    }
    zones.len()
}

/// Regression guard, proved by math rather than a render sweep: the `damping`
/// control must have no degenerate region. Across the whole `0..1` range the
/// resolved (1,1)-mode T60 stays inside the musical band and is monotonic.
#[test]
fn mesh_damping_control_has_no_degenerate_region() {
    for &sample_rate in &[44_100.0_f32, 48_000.0, 96_000.0] {
        assert!(
            (mesh_t60_seconds(sample_rate, 0.0) - MESH_T60_MAX_S).abs() < 0.05,
            "min-damping T60 {} != {MESH_T60_MAX_S}",
            mesh_t60_seconds(sample_rate, 0.0)
        );
        assert!(
            (mesh_t60_seconds(sample_rate, 1.0) - MESH_T60_MIN_S).abs() < 0.02,
            "max-damping T60 {} != {MESH_T60_MIN_S}",
            mesh_t60_seconds(sample_rate, 1.0)
        );

        let mut previous = f32::INFINITY;
        for step in 0..=200 {
            let control = step as f32 / 200.0;
            let t60 = mesh_t60_seconds(sample_rate, control);
            assert!(
                t60 >= 0.25,
                "damping {control} at {sample_rate} Hz dips to T60 {t60} s"
            );
            assert!(
                t60 <= previous + 1.0e-4,
                "damping not monotonic at {control} ({sample_rate} Hz): {t60} > {previous}"
            );
            previous = t60;
        }
    }
}

#[test]
fn mesh_note_mapping_uses_independent_strike_and_pickup_paths() {
    let scale_points = [60, 62, 64, 65, 67, 69, 71, 72]
        .map(|note| mesh_note_position(midi_note_hz(note)))
        .map(|note_position| {
            (
                mesh_note_strike_position(0.42, note_position),
                mesh_note_pickup_position(0.42, note_position),
            )
        });

    assert!(
        span(scale_points.iter().map(|(strike, _)| strike.x)) > 0.15,
        "scale should move the strike horizontally: {scale_points:?}"
    );
    assert!(
        span(scale_points.iter().map(|(strike, _)| strike.y)) > 0.25,
        "scale should move the strike vertically: {scale_points:?}"
    );
    assert!(
        span(scale_points.iter().map(|(_, pickup)| pickup.x)) > 0.08,
        "scale should move the pickup horizontally: {scale_points:?}"
    );
    assert!(
        span(scale_points.iter().map(|(_, pickup)| pickup.y)) > 0.18,
        "scale should move the pickup vertically: {scale_points:?}"
    );
    assert!(
        combined_position_zones(&scale_points) >= 6,
        "C4-C5 scale collapsed into too few strike/pickup zones: {scale_points:?}"
    );
    for (strike, pickup) in scale_points {
        assert!(
            point_distance(strike, pickup) >= MESH_MIN_STRIKE_PICKUP_DISTANCE - 0.01,
            "pickup should stay outside the strike aperture: strike={strike:?} pickup={pickup:?}"
        );
    }
}

#[test]
fn ride_contact_damps_restrikes_more_than_crash() {
    let ride_contact = strike_contact_absorption(0.60, 0.35, 0.50, 0.45);
    let crash_contact = strike_contact_absorption(0.40, 0.10, 0.95, 0.88);
    let ride_hf_loss = boundary_hf_loss(0.60, 0.35, 0.50, 0.45);
    let crash_hf_loss = boundary_hf_loss(0.40, 0.10, 0.95, 0.88);

    assert!(
        ride_contact > crash_contact * 1.8,
        "ride contact should absorb repeated hits more than crash: ride={ride_contact} crash={crash_contact}"
    );
    assert!(
        ride_hf_loss > crash_hf_loss * 1.8,
        "ride boundary should drain high modes faster than crash: ride={ride_hf_loss} crash={crash_hf_loss}"
    );
    assert!(
        (0.000_9..=0.001_3).contains(&crash_hf_loss),
        "crash should stay near the low-loss bloom point: {crash_hf_loss}"
    );
}

#[test]
fn hard_material_uses_narrower_contact_aperture() {
    let soft = voice_config(
        48_000.0,
        MeshVoiceParams {
            material: 0.1,
            ..MeshVoiceParams::default()
        },
    );
    let hard = voice_config(
        48_000.0,
        MeshVoiceParams {
            material: 0.95,
            ..MeshVoiceParams::default()
        },
    );

    assert!(
        hard.excitation_width < soft.excitation_width * 0.35,
        "hard stick contact should be much narrower than soft contact: soft={} hard={}",
        soft.excitation_width,
        hard.excitation_width
    );
}

fn render_default_mesh(sample_rate: f32, seconds: f32) -> Vec<f32> {
    let mut mesh = MeshResonator::new(sample_rate);
    mesh.configure(MeshVoiceParams::default());
    let len = (sample_rate * seconds) as usize;
    (0..len)
        .map(|index| mesh.process_sample((index == 0) as u8 as f32))
        .collect()
}

#[test]
fn pickup_does_not_read_same_sample_strike_force() {
    let sample_rate = 48_000.0;
    let mut mesh = RectangularMesh2d::new(RectangularMesh2dConfig {
        width: 32,
        height: 24,
        sample_rate,
        strike_position: MeshPoint::new(0.5, 0.5),
        pickup_position: MeshPoint::new(0.5, 0.5),
        excitation_width: 0.03,
        pickup_width: 0.03,
        ..RectangularMesh2dConfig::default()
    });

    let first_sample = mesh.process_sample(1.0);
    let propagated = (0..128)
        .map(|_| mesh.process_sample(0.0))
        .collect::<Vec<_>>();

    assert!(
        first_sample.abs() <= 1.0e-7,
        "pickup read unpropagated strike force: {first_sample}"
    );
    assert!(
        peak_abs(&propagated) > 1.0e-5,
        "strike should still enter the mesh and propagate"
    );
}

fn rms_envelope(samples: &[f32], window: usize) -> Vec<f32> {
    let mut env = Vec::new();
    let mut start = 0;
    while start + window <= samples.len() {
        env.push(rms(&samples[start..start + window]));
        start += window;
    }
    env
}

fn ring_out_seconds(samples: &[f32], sample_rate: f32, window: usize, floor_db: f32) -> f32 {
    let env = rms_envelope(samples, window);
    let peak = env.iter().copied().fold(0.0_f32, f32::max).max(1.0e-12);
    let threshold = peak * 10.0_f32.powf(floor_db / 20.0);
    let last = env
        .iter()
        .rposition(|&level| level > threshold)
        .unwrap_or(0);
    (last * window + window / 2) as f32 / sample_rate
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn mesh_shell_body_colors_the_radiated_output() {
    use lindelion_dsp_utils::analysis::dft_magnitude_at;

    let sample_rate = 48_000.0;
    let render = |bare: bool| {
        let mut mesh = MeshResonator::new(sample_rate);
        mesh.configure(MeshVoiceParams::default());
        (0..48_000)
            .map(|index| {
                let excitation = (index == 0) as u8 as f32;
                if bare {
                    mesh.process_sample_bare(excitation)
                } else {
                    mesh.process_sample(excitation)
                }
            })
            .collect::<Vec<_>>()
    };
    let bodied = render(false);
    let bare = render(true);
    assert_all_finite(&bodied);
    assert!(peak_abs(&bodied) < 4.0, "peak_abs={}", peak_abs(&bodied));

    let emphasis = |out: &[f32]| {
        dft_magnitude_at(out, sample_rate, 3_400.0)
            / dft_magnitude_at(out, sample_rate, 6_000.0).max(1.0e-9)
    };
    assert!(
        emphasis(&bodied) > emphasis(&bare) * 1.2,
        "shell body should emphasize the bridge-hill formant: bodied={}, bare={}",
        emphasis(&bodied),
        emphasis(&bare)
    );
}

#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn default_mesh_rings_with_metallic_shimmer_and_stays_stable() {
    let sample_rate = 48_000.0;
    let output = render_default_mesh(sample_rate, 5.0);
    assert_all_finite(&output);
    assert!(peak_abs(&output) < 4.0, "peak_abs={}", peak_abs(&output));

    let ring_out = ring_out_seconds(&output, sample_rate, 4_800, -40.0);
    assert!(
        (1.5..=3.5).contains(&ring_out),
        "mesh -40 dB ring-out: {ring_out}"
    );

    let trajectory = spectral_centroid_trajectory(&output, sample_rate, 2_048, 16_384);
    assert!(
        trajectory.len() >= 2,
        "trajectory too short: {trajectory:?}"
    );
    assert!(
        trajectory.first().unwrap() > trajectory.last().unwrap(),
        "mesh tail should darken: {trajectory:?}"
    );

    let early = rms(&output[..24_000]);
    let late = rms(&output[192_000..]);
    assert!(
        late < early,
        "mesh tail should decay, not grow: early={early}, late={late}"
    );
}

// Local perf-pass harness (run with `cargo test -p lindelion-idiophone -- --ignored
// --nocapture mesh_crash_perf`): times a Crash-scale mesh and prints a checksum so a
// refactor can be confirmed output-identical. Not a make-ci gate.
#[ignore = "local perf measurement; see optimization pass"]
#[test]
fn mesh_crash_perf() {
    let crash = MeshVoiceParams {
        frequency_hz: midi_note_hz(60),
        material: 0.40,
        size: 0.95,
        damping: 0.10,
        tension: 0.88,
        strike_position: 0.90,
        pickup_spread: 0.42,
    };
    let ride = MeshVoiceParams {
        frequency_hz: midi_note_hz(60),
        material: 0.60,
        size: 0.50,
        damping: 0.35,
        tension: 0.45,
        strike_position: 0.72,
        pickup_spread: 0.42,
    };
    measure_mesh("crash drive=0.5", crash, 0.5);
    measure_mesh("crash drive=0.0", crash, 0.0);
    measure_mesh("ride  drive=0.5", ride, 0.5);
    measure_mesh("ride  drive=0.0", ride, 0.0);
}

fn measure_mesh(label: &str, params: MeshVoiceParams, drive: f32) {
    lindelion_dsp_utils::denormal::flush_denormals_on_this_thread();
    let sample_rate = 48_000.0;
    let mut mesh = MeshResonator::new(sample_rate);
    mesh.configure(params);
    let frames = 96_000usize;
    let mut checksum: u64 = 0;
    let mut out_buf = Vec::with_capacity(frames);
    let start = std::time::Instant::now();
    for i in 0..frames {
        // Brief strike impulse, then free ring with a constant geometric drive so the
        // von Karman path runs every block (as it does for a sounding voice).
        let excitation = if i < 64 { 0.9 } else { 0.0 };
        mesh.set_geometric_drive(drive);
        let out = mesh.process_sample(excitation);
        checksum = checksum
            .wrapping_mul(1_000_003)
            .wrapping_add(out.to_bits() as u64);
        out_buf.push(out);
    }
    let elapsed = start.elapsed();
    let realtime = frames as f64 / sample_rate as f64;
    eprintln!(
        "mesh_perf [{label}]: {:.3} ms  ({:.1}% of realtime)  checksum={checksum:#018x}  rms={:.6} peak={:.6}",
        elapsed.as_secs_f64() * 1e3,
        elapsed.as_secs_f64() / realtime * 100.0,
        rms(&out_buf),
        peak_abs(&out_buf),
    );
}
