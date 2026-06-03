//! Runtime driver that promotes the rectangular 2D waveguide mesh to a
//! selectable resonator model. Buffers are allocated once at the maximum grid, so
//! every [`MeshResonator::configure`] re-tunes the active grid (and its `size`/
//! `tension`-driven cell count) in place without allocating.

use crate::{
    filters::{Biquad, BiquadCoefficients},
    math::{self, finite_clamp},
};

use super::{
    MAX_MESH_HEIGHT, MAX_MESH_WIDTH, MeshBoundaryConfig, MeshPoint, RectangularMesh2d,
    RectangularMesh2dConfig,
};
use crate::idiophone::sanitize_sample_rate;

/// Active grid the mesh is born with, before the first `configure` sets it from
/// `size`/`tension`. Just a sane pre-roll default; buffers allocate at the maximum.
const RUNTIME_MESH_WIDTH: usize = 32;
const RUNTIME_MESH_HEIGHT: usize = 24;

/// Physical, per-voice parameters for the 2D-mesh resonator. Every control is
/// normalised to `0..1` except `frequency_hz`. NB: `frequency_hz` is currently
/// **ignored** — a unit-delay mesh is a fixed-pitch struck idiophone, so the note
/// shapes nothing here; `size`/`tension` (grid cell count) carry the timbre instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVoiceParams {
    pub frequency_hz: f32,
    pub material: f32,
    pub size: f32,
    pub damping: f32,
    pub tension: f32,
    pub strike_position: f32,
    pub pickup_spread: f32,
}

impl Default for MeshVoiceParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            material: 0.5,
            size: 0.5,
            // Matches the shipped `MeshConfig`/host-parameter default; the geometric
            // `boundary_damping_loss` map puts 0.3 at a ~1.8 s metallic-shimmer T60.
            damping: 0.3,
            tension: 0.5,
            strike_position: 0.4,
            pickup_spread: 0.3,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshResonator {
    sample_rate: f32,
    mesh: RectangularMesh2d,
    /// One-way shell body coloring the radiated mesh output (M11 P4). Fixed-
    /// frequency, built once; a note only resets it.
    body: OutputBody,
}

impl MeshResonator {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let mesh = RectangularMesh2d::new(RectangularMesh2dConfig {
            width: RUNTIME_MESH_WIDTH,
            height: RUNTIME_MESH_HEIGHT,
            sample_rate,
            ..RectangularMesh2dConfig::default()
        });
        Self {
            sample_rate,
            mesh,
            body: mesh_output_body(sample_rate),
        }
    }

    pub fn configure(&mut self, params: MeshVoiceParams) {
        self.mesh
            .reconfigure(voice_config(self.sample_rate, params));
    }

    pub fn reset(&mut self) {
        self.mesh.reset();
        self.body.reset();
    }

    pub fn process_sample(&mut self, excitation: f32) -> f32 {
        // The shell body colors the radiated pickup output one-way; it never feeds
        // back into the mesh, so it cannot affect the ring-out or stability.
        self.body
            .process_sample(self.mesh.process_sample(excitation))
    }

    /// Render the raw mesh pickup without the shell body (test-only reference for
    /// isolating the body's coloration).
    #[cfg(test)]
    fn process_sample_bare(&mut self, excitation: f32) -> f32 {
        self.mesh.process_sample(excitation)
    }

    /// Forward the measured-energy bus (M2) to the mesh's geometric (von Kármán)
    /// coupling (M6). Set once per host sample by the resonator engine.
    pub fn set_geometric_drive(&mut self, drive: f32) {
        self.mesh.set_geometric_drive(drive);
    }
}

/// Map the six physical controls onto a mesh configuration, steering the lowest
/// active grid from `size`/`tension`. A unit-delay waveguide mesh has a structurally
/// fixed wave speed, so its pitch and modal density come from the **grid cell count**,
/// not from `wave_speed`/physical dimensions (which the update never reads). `size`
/// scales the grid and `tension` its aspect, together spanning a 2-D timbre space:
/// small/sparse = near-pitched triangle, large/dense = inharmonic cymbal-like wash.
/// The played note's `frequency_hz` is intentionally ignored — the mesh is a struck
/// idiophone, not a tuned voice.
fn voice_config(sample_rate: f32, params: MeshVoiceParams) -> RectangularMesh2dConfig {
    let size = clamp01(params.size);
    let tension = clamp01(params.tension);
    let width = grid_dim(size, MESH_MIN_WIDTH, MAX_MESH_WIDTH);
    let height = grid_dim(tension, MESH_MIN_HEIGHT, MAX_MESH_HEIGHT);

    // `material` morphs membrane (free, drum-like) to plate (fixed, stiff) and
    // sets how hard/spread the strike couples in.
    let material = clamp01(params.material);
    // The damping control targets a decay *time*; the per-reflection loss that hits
    // that time depends on the active grid (more cells = more samples to cross), so the
    // ring length stays consistent as `size`/`tension` change the grid.
    let damping = boundary_damping_loss(sample_rate, params.damping, width, height);
    let boundary = if material < 0.5 {
        MeshBoundaryConfig::free(damping)
    } else {
        MeshBoundaryConfig::fixed(damping)
    };

    // The note can't tune a fixed-grid mesh, but it *is* useful as **where you strike**:
    // map the played pitch across the plate around the patch's strike position, so different
    // notes excite different mode mixes — an audibly different intonation/timbre per note,
    // the way striking a cymbal at different spots sounds different.
    let note_offset = (mesh_note_position(params.frequency_hz) - 0.5) * MESH_NOTE_STRIKE_SPREAD;
    let strike = clamp01(params.strike_position + note_offset);
    RectangularMesh2dConfig {
        width,
        height,
        sample_rate,
        // Unused by the unit-delay update (kept only for the struct / test helper).
        wave_speed_mps: 220.0,
        physical_width_m: 0.7,
        physical_height_m: 0.45,
        boundary,
        strike_position: MeshPoint::new(strike, lerp(0.3, 0.7, strike)),
        pickup_position: MeshPoint::new(0.7, 0.55),
        excitation_width: lerp(0.03, 0.12, material),
        pickup_width: lerp(0.02, 0.25, clamp01(params.pickup_spread)),
    }
}

/// Smallest active grid dimension (sparse, near-pitched triangle end of `size`/`tension`).
const MESH_MIN_WIDTH: usize = 10;
const MESH_MIN_HEIGHT: usize = 8;

/// How far across the plate the played note moves the strike position (peak-to-peak). The
/// note maps to a ±half-this offset around the patch strike position.
const MESH_NOTE_STRIKE_SPREAD: f32 = 0.7;

/// Map a `0..1` control onto an active grid dimension in `[min, max]` cells.
fn grid_dim(control: f32, min: usize, max: usize) -> usize {
    lerp(min as f32, max as f32, clamp01(control)).round() as usize
}

/// Normalised position of a played pitch across the C2–C6 register (`0..1`), used to move
/// the mesh strike position with the note. Returns the centre (0.5) for an invalid pitch.
fn mesh_note_position(frequency_hz: f32) -> f32 {
    const C2_HZ: f32 = 65.41;
    if frequency_hz > 0.0 && frequency_hz.is_finite() {
        clamp01((frequency_hz / C2_HZ).log2() / 4.0)
    } else {
        0.5
    }
}

/// Longest / shortest ring the `damping` control spans, as a −60 dB decay time. The
/// control maps geometrically (perceptually uniform in decay ratio) across this band,
/// so the bottom end is a long metallic shimmer and the top end a tight but still
/// clearly audible plate — **no value in `0..1` is a dead thud** (the old map let the
/// top ~¾ of the range collapse to a ~50 ms transient; LAMATH-RENDER-FIXES P2).
const MESH_T60_MAX_S: f32 = 4.0;
const MESH_T60_MIN_S: f32 = 0.30;

/// Per-reflection boundary loss for the `damping` control (`0..1`), derived from the
/// physics rather than hand-tuned. A wave crosses the `W×H` grid one cell per sample
/// and loses a factor `(1 − loss)` at each edge reflection, so the slow (1,1) mode
/// decays as `(1 − loss)^(t · fs · (1/W + 1/H))`. Inverting the standard −60 dB ring
/// time gives `loss = 1 − exp(−3·ln10 / (T60 · fs · (1/W + 1/H)))`. Mapping the control
/// to a geometric `T60 ∈ [MESH_T60_MIN_S, MESH_T60_MAX_S]` makes every value musical by
/// construction and makes ring length sample-rate-independent (the old fixed
/// coefficient drifted with `fs`).
fn boundary_damping_loss(sample_rate: f32, control: f32, width: usize, height: usize) -> f32 {
    let t60 = MESH_T60_MAX_S * (MESH_T60_MIN_S / MESH_T60_MAX_S).powf(clamp01(control));
    let loss = 1.0 - (-mesh_decay_k(sample_rate, width, height) / t60).exp();
    finite_clamp(loss, 0.0, 1.0, 0.0)
}

/// `K = 3·ln10 / (fs · (1/W + 1/H))` — the per-second reflection-decay constant for the
/// active `W×H` grid, so that `T60 = K / −ln(1 − loss)`.
fn mesh_decay_k(sample_rate: f32, width: usize, height: usize) -> f32 {
    let reflections_per_s = sample_rate * (1.0 / width.max(1) as f32 + 1.0 / height.max(1) as f32);
    3.0 * std::f32::consts::LN_10 / reflections_per_s.max(1.0)
}

fn lerp(low: f32, high: f32, fraction: f32) -> f32 {
    low + (high - low) * fraction
}

fn clamp01(value: f32) -> f32 {
    finite_clamp(value, 0.0, 1.0, 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyMode {
    frequency_hz: f32,
    q: f32,
    gain: f32,
}

/// One-way modal coloration body for the mesh's radiated output. It never feeds
/// back into the mesh, so it cannot alter ring-out stability.
#[derive(Debug, Clone, PartialEq)]
struct OutputBody {
    filters: Vec<Biquad>,
    gains: Vec<f32>,
    dry_gain: f32,
}

impl OutputBody {
    fn new(sample_rate: f32, modes: &[BodyMode], dry_gain: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let filters = modes
            .iter()
            .map(|mode| {
                let frequency_hz =
                    math::finite_clamp(mode.frequency_hz, 1.0, sample_rate * 0.45, 100.0);
                let q = math::finite_clamp(mode.q, 0.5, 200.0, 10.0);
                Biquad::new(BiquadCoefficients::bandpass(sample_rate, frequency_hz, q))
            })
            .collect();
        let gains = modes
            .iter()
            .map(|mode| math::finite_clamp(mode.gain, 0.0, 8.0, 0.0))
            .collect();
        Self {
            filters,
            gains,
            dry_gain: math::finite_clamp(dry_gain, 0.0, 2.0, 1.0),
        }
    }

    fn process_sample(&mut self, input: f32) -> f32 {
        let input = math::snap_to_zero(input);
        let mut wet = 0.0;
        for (filter, &gain) in self.filters.iter_mut().zip(&self.gains) {
            wet += gain * filter.process(input);
        }
        math::snap_to_zero(self.dry_gain * input + wet)
    }

    fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
    }
}

const MESH_BODY_MODES: [BodyMode; 2] = [
    BodyMode {
        frequency_hz: 420.0,
        q: 5.0,
        gain: 0.35,
    },
    BodyMode {
        frequency_hz: 3_400.0,
        q: 2.5,
        gain: 0.5,
    },
];

fn mesh_output_body(sample_rate: f32) -> OutputBody {
    OutputBody::new(sample_rate, &MESH_BODY_MODES, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{assert_all_finite, peak_abs, rms, spectral_centroid_trajectory};

    /// Closed-form −60 dB ring time the `damping` control resolves to (at the default
    /// grid), inverting `boundary_damping_loss`: `T60 = K / −ln(1 − loss)`.
    fn mesh_t60_seconds(sample_rate: f32, control: f32) -> f32 {
        let (w, h) = (RUNTIME_MESH_WIDTH, RUNTIME_MESH_HEIGHT);
        let loss = boundary_damping_loss(sample_rate, control, w, h);
        mesh_decay_k(sample_rate, w, h) / -(1.0 - loss).ln()
    }

    /// P2 (LAMATH-RENDER-FIXES) regression guard, proved by math rather than a render
    /// sweep: the `damping` control must have **no degenerate region**. Across the whole
    /// `0..1` range the resolved (1,1)-mode T60 stays inside the musical band, is
    /// monotonic (more damping → shorter ring), and never dips toward the ~60 ms dead
    /// thud the old `lerp(8e-5, 0.5, p³)` map produced over its top ¾. Pure arithmetic
    /// on the boundary-loss closed form, so it runs in the fast `make ci` path.
    #[test]
    fn mesh_damping_control_has_no_degenerate_region() {
        for &sample_rate in &[44_100.0_f32, 48_000.0, 96_000.0] {
            // Endpoints land on the intended band (sample-rate-independent by design).
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
            // Every value rings well clear of the dead-thud threshold (~0.06 s) and the
            // control decreases monotonically.
            let mut previous = f32::INFINITY;
            for step in 0..=200 {
                let control = step as f32 / 200.0;
                let t60 = mesh_t60_seconds(sample_rate, control);
                assert!(
                    t60 >= 0.25,
                    "damping {control} at {sample_rate} Hz dips to T60 {t60} s (dead-thud region)"
                );
                assert!(
                    t60 <= previous + 1.0e-4,
                    "damping not monotonic at {control} ({sample_rate} Hz): {t60} > {previous}"
                );
                previous = t60;
            }
        }
    }

    fn render_default_mesh(sample_rate: f32, seconds: f32) -> Vec<f32> {
        let mut mesh = MeshResonator::new(sample_rate);
        mesh.configure(MeshVoiceParams::default());
        let len = (sample_rate * seconds) as usize;
        (0..len)
            .map(|index| mesh.process_sample((index == 0) as u8 as f32))
            .collect()
    }

    /// Per-window RMS envelope (`window` samples, non-overlapping).
    fn rms_envelope(samples: &[f32], window: usize) -> Vec<f32> {
        let mut env = Vec::new();
        let mut start = 0;
        while start + window <= samples.len() {
            env.push(rms(&samples[start..start + window]));
            start += window;
        }
        env
    }

    /// Audible ring-out: time (s) at which the RMS envelope last sits above
    /// `floor_db` below its peak window. Robust to the mesh's two-rate decay (a
    /// linear T60 fit is fooled by the fast high-mode death over the slow tail).
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

    /// M11 P4 step 3: the one-way shell body colors the radiated mesh output — its
    /// bridge-hill formant is emphasized relative to the bare mesh — while staying
    /// bounded. (The body is one-way, so the mesh ring-out is covered by
    /// `default_mesh_rings_with_metallic_shimmer_and_stays_stable`, which now renders
    /// through the body and still passes.)
    #[cfg_attr(
        not(feature = "integration-tests"),
        ignore = "see make test-integration"
    )]
    #[test]
    fn mesh_shell_body_colors_the_radiated_output() {
        use crate::analysis::dft_magnitude_at;

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

        // The bridge-hill formant (3.4 kHz) is emphasized by the body, relative to an
        // off-formant reference (6 kHz), more than in the bare mesh.
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

    /// M11 P2 step 3: the default mesh voice rings with a long metallic shimmer
    /// (the old uniform-boundary mesh died to silence in ~100 ms), its high modes
    /// die first via the frequency-shaped boundary (centroid falls), and the long
    /// near-lossless ring stays bounded and non-growing. At the shipped default
    /// (`damping = 0.3`) the closed-form (1,1) T60 is ~1.8 s, so the isolated-core
    /// −40 dB ring lands near 1.1 s (~0.6·T60).
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

        // Ring-out far past the old ~100 ms dead tail. The dense default grid (37×28)
        // sustains a rich tail well past the ~1.8 s fundamental T60 (the closed-form
        // map is guarded by `mesh_damping_control_has_no_degenerate_region`).
        let ring_out = ring_out_seconds(&output, sample_rate, 4_800, -40.0);
        assert!(
            (1.5..=3.5).contains(&ring_out),
            "mesh -40 dB ring-out: {ring_out}"
        );

        // High modes die first → the spectral centroid falls over the tail.
        let trajectory = spectral_centroid_trajectory(&output, sample_rate, 2_048, 16_384);
        assert!(
            trajectory.len() >= 2,
            "trajectory too short: {trajectory:?}"
        );
        assert!(
            trajectory.first().unwrap() > trajectory.last().unwrap(),
            "mesh tail should darken: {trajectory:?}"
        );

        // Non-growing.
        let early = rms(&output[..24_000]);
        let late = rms(&output[192_000..]);
        assert!(
            late < early,
            "mesh tail should decay, not grow: early={early}, late={late}"
        );
    }
}
