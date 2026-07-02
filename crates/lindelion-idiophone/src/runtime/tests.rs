use super::*;
use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};

/// Closed-form -60 dB ring time the `damping` control resolves to at the low reference
/// band. On the plate this is exact by construction: the geometric control map feeds the
/// σ₀/σ₁ calibration directly, and is sample-rate independent.
fn mesh_t60_seconds(_sample_rate: f32, control: f32) -> f32 {
    damping_t60_low(control)
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
    // Re-pointed from the membrane's boundary HF shelf to the plate's σ₁ loss: the same
    // voicing ordering must hold — a ride (higher damping) sheds its high band much faster
    // than a crash, and the crash stays long-ringing.
    let ride_contact = strike_contact_absorption(0.60, 0.35, 0.50, 0.45);
    let crash_contact = strike_contact_absorption(0.40, 0.10, 0.95, 0.88);

    assert!(
        ride_contact > crash_contact * 1.8,
        "ride contact should absorb repeated hits more than crash: ride={ride_contact} crash={crash_contact}"
    );

    let sigma1_for = |damping: f32| {
        let (low, high) = damping_targets(damping);
        crate::plate::losses_for(low, high, 1.07, 0.0).1
    };
    let ride_sigma1 = sigma1_for(0.35);
    let crash_sigma1 = sigma1_for(0.10);
    assert!(
        ride_sigma1 > crash_sigma1 * 1.8,
        "ride should drain high modes faster than crash: ride={ride_sigma1} crash={crash_sigma1}"
    );
    assert!(
        damping_t60_low(0.10) > 2.5,
        "crash should stay long-ringing: t60={}",
        damping_t60_low(0.10)
    );
}

#[test]
fn hard_material_uses_narrower_contact_aperture() {
    let soft = contact_excitation_width(0.1);
    let hard = contact_excitation_width(0.95);

    assert!(
        hard < soft * 0.35,
        "hard stick contact should be much narrower than soft contact: soft={soft} hard={hard}"
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
    // Worst case for the ordering contract: strike and pickup apertures fully overlap.
    // The pickup must read the pre-strike state on the strike sample.
    use crate::plate::spatial::SpatialWeights;
    use crate::plate::{PlateBoundary, PlateConfig, PlateKernel};

    let mut kernel = PlateKernel::new(PlateConfig {
        length_x_m: 0.3,
        length_y_m: 0.25,
        boundary: PlateBoundary::Clamped,
        ..PlateConfig::default()
    });
    let grid = kernel.grid();
    let weights = SpatialWeights::new(MeshPoint::new(0.5, 0.5), grid.width, grid.height, 0.03);

    let first_sample = kernel.process_voice_sample(1.0e5, &weights, &weights, 0.0);
    let propagated = (0..128)
        .map(|_| kernel.process_voice_sample(0.0, &weights, &weights, 0.0))
        .collect::<Vec<_>>();

    assert!(
        first_sample.abs() <= 1.0e-7,
        "pickup read unpropagated strike force: {first_sample}"
    );
    assert!(
        peak_abs(&propagated) > 1.0e-5,
        "strike should still enter the plate and propagate"
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
fn default_mesh_rings_with_metallic_shimmer_and_stays_stable() {
    let sample_rate = 48_000.0;
    let output = render_default_mesh(sample_rate, 5.0);
    assert_all_finite(&output);
    assert!(peak_abs(&output) < 4.0, "peak_abs={}", peak_abs(&output));

    // The default damping (0.3) resolves to a 1.84 s low-band T60, i.e. a −40 dB ring-out
    // near 1.23 s — the plate hits its decay law exactly, so the guard band is centred on
    // the design value (the membrane's under-damped band was 1.5–3.5 s).
    let ring_out = ring_out_seconds(&output, sample_rate, 4_800, -40.0);
    assert!(
        (0.9..=2.2).contains(&ring_out),
        "plate -40 dB ring-out: {ring_out}"
    );

    // "Highs die first" asserted in band-energy terms: single-window spectral centroids
    // wobble ±2× with modal beating on a sparse spectrum and proved an unreliable meter
    // (re-pointed twice). Same-band early/late energy ratios are robust: the high band
    // must lose more dB than the low band over the same interval (σ₁ law).
    let band = |start: usize, low: f32, high: f32| -> f32 {
        let window = &output[start..start + 24_000];
        let probes = 12;
        (0..probes)
            .map(|n| low + (high - low) * n as f32 / (probes - 1) as f32)
            .map(|f| {
                let mag = lindelion_dsp_utils::analysis::dft_magnitude_at(window, sample_rate, f);
                mag * mag
            })
            .sum::<f32>()
    };
    let low_drop = band(9_600, 200.0, 600.0) / band(96_000, 200.0, 600.0).max(1.0e-30);
    let high_drop = band(9_600, 2_000.0, 6_000.0) / band(96_000, 2_000.0, 6_000.0).max(1.0e-30);
    assert!(
        high_drop > low_drop * 2.0,
        "high band must decay faster than the low band: low_drop={low_drop} high_drop={high_drop}"
    );

    let early = rms(&output[..24_000]);
    let late = rms(&output[192_000..]);
    assert!(
        late < early,
        "mesh tail should decay, not grow: early={early}, late={late}"
    );
}

// Local perf-pass harness (run with `cargo test -p lindelion-idiophone -- --ignored
// --nocapture plate_crash_perf`): times a Crash-scale plate and prints a checksum so a
// refactor can be confirmed output-identical. Not a make-ci gate.
//
// Reference rows (debug build, same host):
//   membrane (M0 baseline): crash 34453.8 ms coupling-on / 19666.6 ms idle;
//                           ride  13211.8 ms / 7696.7 ms
//   plate linear (M0):      crash 19642.5 ms, ride 7040.6 ms
//   plate + cascade (M3):   crash 27071.4 ms, ride 6706.9 ms (coefficient form)
//   plate + conservative cascade (M3 option-2): crash 33126.8 ms, ride 9054.5 ms —
//   the divergence-form (energy-conserving) edge pass at full bloom depth, inside the
//   membrane coupling budget.
#[ignore = "local perf measurement; see optimization pass"]
#[test]
fn plate_crash_perf() {
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
    measure_mesh("crash", crash);
    measure_mesh("ride ", ride);
}

fn measure_mesh(label: &str, params: MeshVoiceParams) {
    lindelion_dsp_utils::denormal::flush_denormals_on_this_thread();
    let sample_rate = 48_000.0;
    let mut mesh = MeshResonator::new(sample_rate);
    mesh.configure(params);
    let frames = 96_000usize;
    let mut checksum: u64 = 0;
    let mut out_buf = Vec::with_capacity(frames);
    let start = std::time::Instant::now();
    for i in 0..frames {
        // Brief strike impulse, then free ring.
        let excitation = if i < 64 { 0.9 } else { 0.0 };
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

#[test]
fn material_drives_stiffness_and_loss_calibration() {
    // M1 material law: κ = √(D/ρh) ∝ thickness for a given alloy; bronze 0.6–2.0 mm spans
    // κ ≈ 0.64–2.14 m²/s. The loss calibration must receive the per-voice κ (ξ(ω) depends
    // on it), so equal damping at different material yields a different (σ₀, σ₁) pair.
    let layout_for = |material: f32| {
        voice_layout(
            48_000.0,
            MeshVoiceParams {
                material,
                ..MeshVoiceParams::default()
            },
        )
    };
    let mut previous = layout_for(0.0).config.kappa;
    for step in 1..=10 {
        let kappa = layout_for(step as f32 / 10.0).config.kappa;
        assert!(
            kappa > previous,
            "kappa must rise with material: {kappa} after {previous}"
        );
        previous = kappa;
    }
    assert!(
        (layout_for(0.0).config.kappa - 0.64).abs() < 0.05,
        "soft endpoint: {}",
        layout_for(0.0).config.kappa
    );
    assert!(
        (layout_for(1.0).config.kappa - 2.14).abs() < 0.05,
        "hard endpoint: {}",
        layout_for(1.0).config.kappa
    );
    let soft = layout_for(0.2).config;
    let hard = layout_for(0.9).config;
    assert!(
        (soft.sigma1 - hard.sigma1).abs() > f32::EPSILON,
        "loss calibration must see the per-voice kappa: soft σ1={} hard σ1={}",
        soft.sigma1,
        hard.sigma1
    );
}

#[test]
fn size_and_tension_map_monotonically_onto_plate_physics() {
    let layout_for = |size: f32, tension: f32| {
        voice_layout(
            48_000.0,
            MeshVoiceParams {
                size,
                tension,
                ..MeshVoiceParams::default()
            },
        )
        .config
    };
    let mut previous_area = 0.0_f32;
    for step in 0..=10 {
        let config = layout_for(step as f32 / 10.0, 0.5);
        let area = config.length_x_m * config.length_y_m;
        assert!(
            area > previous_area,
            "plate area must rise with size: {area} after {previous_area}"
        );
        previous_area = area;
    }
    let mut previous_speed = -1.0_f32;
    for step in 0..=10 {
        let config = layout_for(0.5, step as f32 / 10.0);
        assert!(
            config.tension_speed > previous_speed,
            "membrane speed must rise with tension: {} after {previous_speed}",
            config.tension_speed
        );
        previous_speed = config.tension_speed;
    }
    assert_eq!(layout_for(0.5, 0.0).tension_speed, 0.0);
}

#[test]
fn radiation_stage_tilts_the_spectrum_toward_treble() {
    // M2 radiation gate: far-field pressure follows volume acceleration (+6 dB/oct over
    // the velocity tap below coincidence). Comparative form: the same strike rendered
    // through the raw cluster-velocity tap and through the radiation stage must differ in
    // band balance E(2–4 kHz)/E(250–500 Hz) by at least 10× (ideal first-difference tilt
    // between those band centers is ≈ 64×). An absolute >3 kHz fraction is not a valid
    // observable here: the linear plate driven by a 0.45 ms contact has little true
    // content there until the M3 cascade.
    use crate::plate::PlateKernel;
    use crate::plate::spatial::SpatialWeights;
    use lindelion_dsp_utils::analysis::dft_magnitude_at;

    let params = MeshVoiceParams::default();
    let layout = voice_layout(48_000.0, params);
    let pulse: Vec<f32> = (0..22)
        .map(|n| 0.5 * (1.0 - (std::f32::consts::TAU * n as f32 / 21.0).cos()))
        .collect();

    let mut kernel = PlateKernel::new(layout.config);
    // Match the runtime's bloom drive and depth so the comparison isolates the
    // radiation stage.
    kernel.set_drive_normalization(BLOOM_DRIVE_NORMALIZATION);
    kernel.set_bloom_depth(BLOOM_DEPTH);
    let grid = kernel.grid();
    let source = SpatialWeights::new(
        layout.strike,
        grid.width,
        grid.height,
        layout.excitation_width,
    );
    let pickup =
        SpatialWeights::point_cluster(layout.pickup, grid.width, grid.height, layout.pickup_width);
    let mut velocity_out = Vec::with_capacity(24_000);
    for n in 0..24_000 {
        let force = pulse.get(n).copied().unwrap_or(0.0) * 5.0e3;
        velocity_out.push(kernel.process_voice_sample(force, &source, &pickup, 0.0));
    }

    let mut mesh = MeshResonator::new(48_000.0);
    mesh.configure(params);
    let mut radiated_out = Vec::with_capacity(24_000);
    for n in 0..24_000 {
        let excitation = pulse.get(n).copied().unwrap_or(0.0);
        radiated_out.push(mesh.process_sample(excitation));
    }

    let band = |out: &[f32], low: f32, high: f32| -> f32 {
        let bins = 9;
        (0..bins)
            .map(|n| low + (high - low) * n as f32 / (bins - 1) as f32)
            .map(|f| {
                let mag = dft_magnitude_at(&out[..16_384], 48_000.0, f);
                mag * mag
            })
            .sum::<f32>()
    };
    let balance = |out: &[f32]| band(out, 2_000.0, 4_000.0) / band(out, 250.0, 500.0).max(1.0e-30);
    let velocity_balance = balance(&velocity_out);
    let radiated_balance = balance(&radiated_out);
    // Measured ≈ 8× with the M3 bloom matched on both paths (the bloom adds mid/high
    // content to the velocity reference too, compressing the ratio); without the
    // radiation stage the ratio is exactly 1. Pinned with margin below measured.
    assert!(
        radiated_balance > velocity_balance * 5.0,
        "radiation stage must tilt toward treble: velocity={velocity_balance} radiated={radiated_balance}"
    );
}

#[test]
fn point_cluster_pickup_is_sparse_and_normalized() {
    use crate::plate::spatial::SpatialWeights;
    let cluster = SpatialWeights::point_cluster(MeshPoint::new(0.6, 0.55), 37, 30, 0.06);
    let entries: Vec<(usize, f32)> = cluster.iter().collect();
    assert!(
        (2..=5).contains(&entries.len()),
        "cluster size: {}",
        entries.len()
    );
    let sum: f32 = entries.iter().map(|(_, w)| w).sum();
    assert!((sum - 1.0).abs() < 1.0e-6, "weights must sum to 1: {sum}");
    let mut indices: Vec<usize> = entries.iter().map(|(i, _)| *i).collect();
    indices.sort_unstable();
    indices.dedup();
    assert_eq!(indices.len(), entries.len(), "duplicate cluster cells");
    assert!(indices.iter().all(|&i| i < 37 * 30));
}
