//! Runtime driver that promotes the rectangular 2D waveguide mesh to a
//! selectable resonator model. Buffers are allocated once at the maximum grid, so
//! every [`MeshResonator::configure`] re-tunes the active grid (and its `size`/
//! `tension`-driven cell count) in place without allocating.

use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math::{self, finite_clamp},
};

use super::{
    MAX_MESH_HEIGHT, MAX_MESH_WIDTH, MeshBoundaryConfig, MeshPoint, RectangularMesh2d,
    RectangularMesh2dConfig,
};
use crate::sanitize_sample_rate;

/// Active grid the mesh is born with, before the first `configure` sets it from
/// `size`/`tension`. Just a sane pre-roll default; buffers allocate at the maximum.
const RUNTIME_MESH_WIDTH: usize = 32;
const RUNTIME_MESH_HEIGHT: usize = 24;

/// Physical, per-voice parameters for the 2D-mesh resonator. Every control is
/// normalised to `0..1` except `frequency_hz`. NB: `frequency_hz` does **not**
/// tune the unit-delay mesh; it moves the strike and pickup over the fixed grid so
/// notes select different modal color mixes while `size`/`tension` carry the
/// grid-density timbre.
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
/// The played note's `frequency_hz` moves the strike and pickup positions only: the
/// mesh remains a fixed-pitch struck idiophone, but notes select different modal
/// colors.
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

    // The note can't tune a fixed-grid mesh, but it *is* useful as a modal selector:
    // move both the strike and pickup along different 2-D paths so a played note has
    // two independent nodal selectors instead of one fixed readout point. This widens
    // the note-to-timbre vocabulary without changing the grid or making the note a
    // pitch target.
    let note_position = mesh_note_position(params.frequency_hz);
    RectangularMesh2dConfig {
        width,
        height,
        sample_rate,
        boundary,
        strike_position: mesh_note_strike_position(params.strike_position, note_position),
        pickup_position: mesh_note_pickup_position(params.strike_position, note_position),
        excitation_width: contact_excitation_width(material),
        // Aperture-integral pickup is a radiation area, not the old normalized
        // averaging window. Keep it narrower so dense plates do not cancel their
        // shimmer before it reaches the output.
        pickup_width: lerp(0.015, 0.16, clamp01(params.pickup_spread)),
        boundary_hf_loss: boundary_hf_loss(material, params.damping, size, tension),
        strike_contact_absorption: strike_contact_absorption(
            material,
            params.damping,
            size,
            tension,
        ),
    }
}

/// Smallest active grid dimension (sparse, near-pitched triangle end of `size`/`tension`).
const MESH_MIN_WIDTH: usize = 10;
const MESH_MIN_HEIGHT: usize = 8;

/// How far across the plate the played note moves the strike position (peak-to-peak). The
/// note maps to a ±half-this offset around the patch strike position.
const MESH_NOTE_STRIKE_SPREAD: f32 = 0.7;
/// Vertical strike travel (peak-to-peak) driven by the note's independent Lissajous path.
const MESH_NOTE_STRIKE_Y_SPREAD: f32 = 0.62;
/// How much the strike control biases the independent vertical path.
const MESH_STRIKE_CONTROL_Y_BIAS: f32 = 0.24;
/// Horizontal pickup travel (peak-to-peak), intentionally opposing the strike's note
/// travel to expose more modal combinations.
const MESH_NOTE_PICKUP_X_SPREAD: f32 = 0.36;
/// Vertical pickup travel (peak-to-peak) driven by a different note path than the strike.
const MESH_NOTE_PICKUP_Y_SPREAD: f32 = 0.48;
/// How much the strike control nudges the pickup away from the biased strike side.
const MESH_STRIKE_CONTROL_PICKUP_BIAS: f32 = 0.16;
/// Minimum normalized source-to-pickup separation. The pickup is a radiating
/// aperture, not a contact mic at the strike point; keeping it outside the strike
/// footprint prevents the hammer impulse from becoming a two-sample output spike
/// when the note path crosses the pickup path.
const MESH_MIN_STRIKE_PICKUP_DISTANCE: f32 = 0.34;
/// Keep moving strike/pickup targets off the exact boundaries and centerline singular
/// spots while preserving most of the playable plate area.
const MESH_POSITION_INSET: f32 = 0.06;

/// Map a `0..1` control onto an active grid dimension in `[min, max]` cells.
fn grid_dim(control: f32, min: usize, max: usize) -> usize {
    lerp(min as f32, max as f32, clamp01(control)).round() as usize
}

fn strike_contact_absorption(material: f32, damping: f32, size: f32, tension: f32) -> f32 {
    let stiffness = clamp01(material);
    let damping = clamp01(damping);
    let density = 0.5 * (clamp01(size) + clamp01(tension));
    let contact_impedance = 0.7 * stiffness + 0.3 * damping;
    let dense_plate_escape = 1.0 - 0.55 * density;
    finite_clamp(
        0.04 + 0.50 * contact_impedance * dense_plate_escape + 0.18 * damping,
        0.0,
        0.55,
        0.2,
    )
}

fn boundary_hf_loss(material: f32, damping: f32, size: f32, tension: f32) -> f32 {
    let stiffness = clamp01(material);
    let damping = clamp01(damping);
    let density = 0.5 * (clamp01(size) + clamp01(tension));
    finite_clamp(
        0.000_8 + 0.004_2 * damping.powf(1.2) + 0.001_2 * stiffness * (1.0 - density),
        0.000_4,
        0.008,
        0.001_2,
    )
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

fn mesh_note_strike_position(strike_control: f32, note_position: f32) -> MeshPoint {
    let strike_control = clamp01(strike_control);
    let note_position = clamp01(note_position);
    let x = strike_control + (note_position - 0.5) * MESH_NOTE_STRIKE_SPREAD;
    let y = 0.5
        + (strike_control - 0.5) * MESH_STRIKE_CONTROL_Y_BIAS
        + 0.5
            * MESH_NOTE_STRIKE_Y_SPREAD
            * (std::f32::consts::TAU * (1.5 * note_position + 0.25)).sin();
    MeshPoint::new(inset01(x), inset01(y))
}

fn mesh_note_pickup_position(strike_control: f32, note_position: f32) -> MeshPoint {
    let strike_control = clamp01(strike_control);
    let note_position = clamp01(note_position);
    let strike = mesh_note_strike_position(strike_control, note_position);
    let x = 0.7
        - (note_position - 0.5) * MESH_NOTE_PICKUP_X_SPREAD
        - (strike_control - 0.5) * MESH_STRIKE_CONTROL_PICKUP_BIAS;
    let y = 0.55
        + 0.5
            * MESH_NOTE_PICKUP_Y_SPREAD
            * (std::f32::consts::TAU * (1.25 * note_position + 0.375)).sin();
    pickup_position_with_min_separation(strike, MeshPoint::new(inset01(x), inset01(y)))
}

fn pickup_position_with_min_separation(strike: MeshPoint, pickup: MeshPoint) -> MeshPoint {
    let dx = pickup.x - strike.x;
    let dy = pickup.y - strike.y;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance >= MESH_MIN_STRIKE_PICKUP_DISTANCE {
        return pickup;
    }

    let (unit_x, unit_y) = if distance > f32::EPSILON {
        (dx / distance, dy / distance)
    } else {
        (1.0, 0.0)
    };
    let along_path = MeshPoint::new(
        inset01(strike.x + unit_x * MESH_MIN_STRIKE_PICKUP_DISTANCE),
        inset01(strike.y + unit_y * MESH_MIN_STRIKE_PICKUP_DISTANCE),
    );
    if mesh_point_distance(strike, along_path) >= MESH_MIN_STRIKE_PICKUP_DISTANCE - 0.005 {
        return along_path;
    }

    [
        MeshPoint::new(
            inset01(strike.x + MESH_MIN_STRIKE_PICKUP_DISTANCE),
            strike.y,
        ),
        MeshPoint::new(
            inset01(strike.x - MESH_MIN_STRIKE_PICKUP_DISTANCE),
            strike.y,
        ),
        MeshPoint::new(
            strike.x,
            inset01(strike.y + MESH_MIN_STRIKE_PICKUP_DISTANCE),
        ),
        MeshPoint::new(
            strike.x,
            inset01(strike.y - MESH_MIN_STRIKE_PICKUP_DISTANCE),
        ),
    ]
    .into_iter()
    .filter(|candidate| {
        mesh_point_distance(strike, *candidate) >= MESH_MIN_STRIKE_PICKUP_DISTANCE - 0.005
    })
    .max_by(|left, right| {
        pickup_alignment(strike, *left, unit_x, unit_y)
            .total_cmp(&pickup_alignment(strike, *right, unit_x, unit_y))
    })
    .unwrap_or(along_path)
}

fn mesh_point_distance(a: MeshPoint, b: MeshPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn pickup_alignment(strike: MeshPoint, pickup: MeshPoint, unit_x: f32, unit_y: f32) -> f32 {
    (pickup.x - strike.x) * unit_x + (pickup.y - strike.y) * unit_y
}

/// Longest / shortest ring the `damping` control spans, as a −60 dB decay time. The
/// control maps geometrically (perceptually uniform in decay ratio) across this band,
/// so the bottom end is a long metallic shimmer and the top end a tight but still
/// clearly audible plate — **no value in `0..1` is a dead thud** (the old map let the
/// top ~¾ of the range collapse to a ~50 ms transient).
const MESH_T60_MAX_S: f32 = 4.0;
const MESH_T60_MIN_S: f32 = 0.30;

const CONTACT_WIDTH_SOFT: f32 = 0.13;
const CONTACT_WIDTH_HARD: f32 = 0.018;
const CONTACT_HARDNESS_CURVE: f32 = 1.4;

fn contact_excitation_width(material: f32) -> f32 {
    let hardness = clamp01(material).powf(CONTACT_HARDNESS_CURVE);
    lerp(CONTACT_WIDTH_SOFT, CONTACT_WIDTH_HARD, hardness)
}

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

fn inset01(value: f32) -> f32 {
    finite_clamp(value, MESH_POSITION_INSET, 1.0 - MESH_POSITION_INSET, 0.5)
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
#[path = "runtime/tests.rs"]
mod tests;
