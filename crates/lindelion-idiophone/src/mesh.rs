use lindelion_dsp_utils::math;

use super::sanitize_sample_rate;

#[path = "boundary.rs"]
mod boundary;
#[path = "runtime.rs"]
mod runtime;
use boundary::{BoundaryLowpass, DEFAULT_BOUNDARY_HF_LOSS, boundary_lowpass_step};
pub use runtime::{MeshResonator, MeshVoiceParams};

const MIN_MESH_SIZE: usize = 3;
/// Maximum active grid the mesh can be configured to. Buffers are allocated once at
/// this size; `size`/`tension` select a smaller active sub-region (no reallocation).
/// Grid cell count is the timbral density lever in a unit-delay waveguide mesh — a
/// large grid carries a dense, inharmonic, cymbal-like spectrum, a small one a sparse,
/// near-pitched triangle. CPU is `O(width·height)` per sample; ample for a single
/// idiophone voice.
const MAX_MESH_WIDTH: usize = 64;
const MAX_MESH_HEIGHT: usize = 48;
const MAX_MESH_CELLS: usize = MAX_MESH_WIDTH * MAX_MESH_HEIGHT;

/// Measured-energy (RMS) at which the geometric (von Kármán) coupling reaches its
/// target depth; the squared, normalized drive `(energy/REF)^2` keeps soft strikes
/// linear and concentrates the bloom on hard ones. Calibrated to the measured per-voice
/// energy bus so a full-velocity Mesh strike sits near ≈0.7 drive (the upward modal
/// bloom a hard gong/cymbal makes). The larger active grid spreads the strike energy
/// over more cells, lowering the measured RMS, so this REF was dropped from the old
/// 14×10-grid value (0.013) to keep the bloom engaging at musical strike levels.
const GEOMETRIC_ENERGY_REF: f32 = 0.003;
/// Clamp on the normalized squared energy term (the coupling depth at peak energy).
const GEOMETRIC_MAX_DRIVE: f32 = 1.0;
/// Maximum rotation `sin` factor at full coupling: the fraction of the low mode's
/// amplitude rotated up into the high-spatial-frequency mode each junction.
/// Bounded below 1 so the per-junction transfer stays gentle and the scheme stays
/// stable. Specified as `sin` (not an angle) so the rotation needs only a `sqrt`,
/// not `sin_cos` — cheap enough for the per-junction inner loop while staying
/// exactly energy-conserving (`cos = sqrt(1 - sin^2)`, so `sin^2 + cos^2 = 1`).
const GEOMETRIC_MAX_SIN: f32 = 0.3;
/// Maps the local junction displacement to the [0,1] amplitude factor: high-
/// pressure junctions couple most (the large-deflection geometric nonlinearity).
const GEOMETRIC_AMPLITUDE_SENS: f32 = 6.0;
/// Blend from pure aperture pressure toward a bending/curvature radiation term.
/// Cymbals do not radiate only by summing signed displacement over a broad area:
/// high-spatial-frequency bending also couples to air. The curvature tap keeps
/// dense plates from collapsing into a dark low-mode area integral after the
/// source/pickup normalization fix.
const CURVATURE_RADIATION_GAIN: f32 = 4.0;
/// Dense cymbal meshes need a separate high-spatial-frequency radiation path:
/// a broad signed aperture carries body level, while this density-scaled narrow
/// tap lets shimmer/bloom radiate without adding a family output gain stage.
const SHIMMER_RADIATION_GAIN: f32 = 12.0;
const SHIMMER_RADIATION_DENSITY_EXP: f32 = 1.5;
const SHIMMER_PICKUP_WIDTH_SCALE: f32 = 0.45;
const SHIMMER_PICKUP_WIDTH_MIN: f32 = 0.012;
const STRIKE_CONTACT_MAX_LOSS_FRACTION: f32 = 0.35;
const STRIKE_CONTACT_MOTION_DAMPING: f32 = 0.62;
const STRIKE_CONTACT_PRESSURE_DAMPING: f32 = 0.10;

/// Squared, normalized energy term in `[0, GEOMETRIC_MAX_DRIVE]` setting how
/// strongly the mesh couples at the current playing energy.
fn drive_term(energy: f32) -> f32 {
    let normalized = math::finite_or(energy, 0.0).max(0.0) / GEOMETRIC_ENERGY_REF;
    math::finite_clamp(normalized * normalized, 0.0, GEOMETRIC_MAX_DRIVE, 0.0)
}

/// Energy-conserving geometric coupling at a junction (von Kármán large-deflection
/// nonlinearity). Rotates the outgoing wave by `angle` in the plane spanning the
/// locally-uniform mode `u = (1,1,1,1)/2` (low spatial frequency) and the
/// alternating mode `v = (1,-1,1,-1)/2` (high spatial frequency), transferring
/// energy from `u` to `v` — upward into higher modes. The rotation preserves
/// `cu^2 + cv^2`, so the junction (and the mesh) never gains energy.
fn geometric_couple(
    outgoing: (f32, f32, f32, f32),
    pressure: f32,
    drive: f32,
) -> (f32, f32, f32, f32) {
    let drive = math::finite_clamp(drive, 0.0, GEOMETRIC_MAX_DRIVE, 0.0);
    if drive <= f32::EPSILON {
        return outgoing;
    }
    let amplitude = math::finite_clamp(pressure.abs() * GEOMETRIC_AMPLITUDE_SENS, 0.0, 1.0, 0.0);
    // Energy-exact rotation with no transcendental: pick `sin` directly, derive
    // `cos = sqrt(1 - sin^2)` so the (cu, cv) transfer preserves cu^2 + cv^2.
    let sin = GEOMETRIC_MAX_SIN * drive * amplitude;
    let cos = (1.0 - sin * sin).max(0.0).sqrt();
    let (left, right, top, bottom) = outgoing;
    let cu = 0.5 * (left + right + top + bottom);
    let cv = 0.5 * (left - right + top - bottom);
    let du = 0.5 * ((cu * cos - cv * sin) - cu);
    let dv = 0.5 * ((cu * sin + cv * cos) - cv);
    (
        left + du + dv,
        right + du - dv,
        top + du + dv,
        bottom + du - dv,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshPoint {
    x: f32,
    y: f32,
}

impl MeshPoint {
    fn new(x: f32, y: f32) -> Self {
        Self {
            x: math::finite_clamp(x, 0.0, 1.0, 0.5),
            y: math::finite_clamp(y, 0.0, 1.0, 0.5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeshBoundaryKind {
    Fixed,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshBoundaryEdge {
    kind: MeshBoundaryKind,
    damping: f32,
}

impl MeshBoundaryEdge {
    fn fixed(damping: f32) -> Self {
        Self {
            kind: MeshBoundaryKind::Fixed,
            damping,
        }
    }

    fn free(damping: f32) -> Self {
        Self {
            kind: MeshBoundaryKind::Free,
            damping,
        }
    }

    fn reflection(self) -> f32 {
        let sign = match self.kind {
            MeshBoundaryKind::Fixed => -1.0,
            MeshBoundaryKind::Free => 1.0,
        };
        sign * (1.0 - math::finite_clamp(self.damping, 0.0, 1.0, 0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshBoundaryConfig {
    left: MeshBoundaryEdge,
    right: MeshBoundaryEdge,
    top: MeshBoundaryEdge,
    bottom: MeshBoundaryEdge,
}

impl MeshBoundaryConfig {
    fn fixed(damping: f32) -> Self {
        Self {
            left: MeshBoundaryEdge::fixed(damping),
            right: MeshBoundaryEdge::fixed(damping),
            top: MeshBoundaryEdge::fixed(damping),
            bottom: MeshBoundaryEdge::fixed(damping),
        }
    }

    fn free(damping: f32) -> Self {
        Self {
            left: MeshBoundaryEdge::free(damping),
            right: MeshBoundaryEdge::free(damping),
            top: MeshBoundaryEdge::free(damping),
            bottom: MeshBoundaryEdge::free(damping),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RectangularMesh2dConfig {
    width: usize,
    height: usize,
    sample_rate: f32,
    boundary: MeshBoundaryConfig,
    strike_position: MeshPoint,
    pickup_position: MeshPoint,
    excitation_width: f32,
    pickup_width: f32,
    boundary_hf_loss: f32,
    /// Per-strike local absorption while the stick/contact excitation is active.
    /// This models relative-motion contact: a stick hitting a plate that is already
    /// moving absorbs local energy instead of adding an independent impulse forever.
    strike_contact_absorption: f32,
}

impl RectangularMesh2dConfig {
    fn sanitized(self) -> Self {
        Self {
            width: self.width.clamp(MIN_MESH_SIZE, MAX_MESH_WIDTH),
            height: self.height.clamp(MIN_MESH_SIZE, MAX_MESH_HEIGHT),
            sample_rate: sanitize_sample_rate(self.sample_rate),
            boundary: self.boundary,
            strike_position: self.strike_position,
            pickup_position: self.pickup_position,
            excitation_width: math::finite_clamp(self.excitation_width, 0.005, 0.4, 0.06),
            pickup_width: math::finite_clamp(self.pickup_width, 0.005, 0.4, 0.025),
            boundary_hf_loss: math::finite_clamp(
                self.boundary_hf_loss,
                0.000_4,
                0.008,
                DEFAULT_BOUNDARY_HF_LOSS,
            ),
            strike_contact_absorption: math::finite_clamp(
                self.strike_contact_absorption,
                0.0,
                1.0,
                0.0,
            ),
        }
    }
}

impl Default for RectangularMesh2dConfig {
    fn default() -> Self {
        Self {
            width: 14,
            height: 10,
            sample_rate: 48_000.0,
            boundary: MeshBoundaryConfig::fixed(0.16),
            strike_position: MeshPoint::new(0.35, 0.42),
            pickup_position: MeshPoint::new(0.72, 0.58),
            excitation_width: 0.055,
            pickup_width: 0.025,
            boundary_hf_loss: DEFAULT_BOUNDARY_HF_LOSS,
            strike_contact_absorption: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpatialWeight {
    index: usize,
    weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct SpatialWeights {
    weights: Vec<SpatialWeight>,
    normalization: SpatialNormalization,
}

impl SpatialWeights {
    fn new(
        point: MeshPoint,
        width: usize,
        height: usize,
        width_fraction: f32,
        normalization: SpatialNormalization,
    ) -> Self {
        // Capacity is the MAXIMUM grid, so an in-place recompute onto a larger active
        // sub-region (via `reconfigure`) never reallocates on the audio thread.
        let mut weights = Self {
            weights: Vec::with_capacity(MAX_MESH_CELLS),
            normalization,
        };
        weights.recompute(point, width, height, width_fraction);
        weights
    }

    /// Recompute the Gaussian spatial weights in place: `clear` + `push` reuses
    /// the grid-sized capacity, so re-tuning a live voice never allocates.
    fn recompute(&mut self, point: MeshPoint, width: usize, height: usize, width_fraction: f32) {
        self.weights.clear();
        let center_x = point.x * (width - 1) as f32;
        let center_y = point.y * (height - 1) as f32;
        let sigma = (width.min(height) as f32 * width_fraction).max(0.35);
        let radius = (sigma * 2.5).ceil() as isize;
        let mut sum = 0.0;

        for y in (center_y.floor() as isize - radius)..=(center_y.floor() as isize + radius) {
            for x in (center_x.floor() as isize - radius)..=(center_x.floor() as isize + radius) {
                if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
                    continue;
                }
                let dx = (x as f32 - center_x) / sigma;
                let dy = (y as f32 - center_y) / sigma;
                let weight = (-0.5 * (dx * dx + dy * dy)).exp();
                if weight <= 1.0e-6 {
                    continue;
                }
                self.weights.push(SpatialWeight {
                    index: y as usize * width + x as usize,
                    weight,
                });
                sum += weight;
            }
        }

        if sum <= f32::EPSILON {
            self.weights.clear();
            let x = center_x.round().clamp(0.0, (width - 1) as f32) as usize;
            let y = center_y.round().clamp(0.0, (height - 1) as f32) as usize;
            self.weights.push(SpatialWeight {
                index: y * width + x,
                weight: 1.0,
            });
            return;
        }

        match self.normalization {
            SpatialNormalization::UnitEnergy => {
                let square_sum: f32 = self
                    .weights
                    .iter()
                    .map(|weight| weight.weight * weight.weight)
                    .sum();
                let normalizer = square_sum.sqrt().max(f32::EPSILON);
                for weight in &mut self.weights {
                    weight.weight /= normalizer;
                }
            }
            SpatialNormalization::ApertureIntegral => {}
        }
    }

    fn inject_pressure(&self, waves: &mut DirectionalWaves, pressure: f32) {
        let component = math::snap_to_zero(pressure) * 0.25;
        for spatial_weight in &self.weights {
            waves.add_uniform(spatial_weight.index, component * spatial_weight.weight);
        }
    }

    fn absorb_contact_motion(&self, waves: &mut DirectionalWaves, amount: f32) {
        let amount = math::finite_clamp(amount, 0.0, 0.95, 0.0);
        if amount <= f32::EPSILON {
            return;
        }
        for spatial_weight in &self.weights {
            let local_loss = (amount * spatial_weight.weight.abs()).min(0.95);
            waves.damp_directional_motion(spatial_weight.index, local_loss);
            waves.damp_pressure(
                spatial_weight.index,
                local_loss * STRIKE_CONTACT_PRESSURE_DAMPING,
            );
        }
    }

    fn pressure(&self, waves: &DirectionalWaves) -> f32 {
        self.weights
            .iter()
            .map(|spatial_weight| waves.pressure(spatial_weight.index) * spatial_weight.weight)
            .sum()
    }

    fn curvature_pressure(&self, waves: &DirectionalWaves, width: usize, height: usize) -> f32 {
        self.weights
            .iter()
            .map(|spatial_weight| {
                let index = spatial_weight.index;
                let x = index % width;
                let y = index / width;
                let center = waves.pressure(index);
                let left = if x > 0 {
                    waves.pressure(index - 1)
                } else {
                    center
                };
                let right = if x + 1 < width {
                    waves.pressure(index + 1)
                } else {
                    center
                };
                let top = if y > 0 {
                    waves.pressure(index - width)
                } else {
                    center
                };
                let bottom = if y + 1 < height {
                    waves.pressure(index + width)
                } else {
                    center
                };
                let curvature = center - 0.25 * (left + right + top + bottom);
                curvature * spatial_weight.weight
            })
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpatialNormalization {
    /// Source velocity maps to fixed strike energy; widening the strike footprint
    /// suppresses high modes but does not make the whole instrument vanish.
    UnitEnergy,
    /// Raw signed aperture integral. The shipped broad body/radiation pickup uses
    /// this, while the separate shimmer pickup uses `UnitEnergy` so the narrow
    /// high-mode tap changes color without becoming a hidden level control.
    ApertureIntegral,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionalWaves {
    from_left: Vec<f32>,
    from_right: Vec<f32>,
    from_top: Vec<f32>,
    from_bottom: Vec<f32>,
}

impl DirectionalWaves {
    fn new(len: usize) -> Self {
        Self {
            from_left: vec![0.0; len],
            from_right: vec![0.0; len],
            from_top: vec![0.0; len],
            from_bottom: vec![0.0; len],
        }
    }

    fn clear(&mut self) {
        self.from_left.fill(0.0);
        self.from_right.fill(0.0);
        self.from_top.fill(0.0);
        self.from_bottom.fill(0.0);
    }

    fn pressure(&self, index: usize) -> f32 {
        math::snap_to_zero(
            0.5 * (self.from_left[index]
                + self.from_right[index]
                + self.from_top[index]
                + self.from_bottom[index]),
        )
    }

    fn add_uniform(&mut self, index: usize, component: f32) {
        let component = math::snap_to_zero(component);
        self.from_left[index] = math::snap_to_zero(self.from_left[index] + component);
        self.from_right[index] = math::snap_to_zero(self.from_right[index] + component);
        self.from_top[index] = math::snap_to_zero(self.from_top[index] + component);
        self.from_bottom[index] = math::snap_to_zero(self.from_bottom[index] + component);
    }

    fn damp_directional_motion(&mut self, index: usize, loss: f32) {
        let loss = math::finite_clamp(loss, 0.0, 0.95, 0.0);
        if loss <= f32::EPSILON {
            return;
        }
        let keep = 1.0 - loss;
        let average = 0.25
            * (self.from_left[index]
                + self.from_right[index]
                + self.from_top[index]
                + self.from_bottom[index]);
        self.from_left[index] =
            math::snap_to_zero(average + (self.from_left[index] - average) * keep);
        self.from_right[index] =
            math::snap_to_zero(average + (self.from_right[index] - average) * keep);
        self.from_top[index] =
            math::snap_to_zero(average + (self.from_top[index] - average) * keep);
        self.from_bottom[index] =
            math::snap_to_zero(average + (self.from_bottom[index] - average) * keep);
    }

    fn damp_pressure(&mut self, index: usize, loss: f32) {
        let loss = math::finite_clamp(loss, 0.0, 0.95, 0.0);
        if loss <= f32::EPSILON {
            return;
        }
        let pressure = self.pressure(index);
        self.add_uniform(index, -0.5 * pressure * loss);
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RectangularMesh2d {
    config: RectangularMesh2dConfig,
    current: DirectionalWaves,
    next: DirectionalWaves,
    source_weights: SpatialWeights,
    pickup_weights: SpatialWeights,
    shimmer_weights: SpatialWeights,
    // Per-edge boundary loss filter (M11 P2 step 3): frequency-shaped reflections
    // so high plate modes die first.
    boundary_lowpass: BoundaryLowpass,
    // Measured resonator energy (M2 bus) driving the geometric (von Kármán)
    // coupling; set per host sample, constant across the 2x sub-samples. 0.0 => inert.
    geometric_drive: f32,
    // Output level normalization for the active grid. The broad pickup is a signed
    // aperture and dense meshes add a separate energy-normalized shimmer tap, so
    // the old `cells / REF` correction for a normalized-average pickup no longer
    // applies.
    level_compensation: f32,
}

fn mesh_level_compensation(_width: usize, _height: usize) -> f32 {
    1.0
}

fn shimmer_pickup_width(pickup_width: f32) -> f32 {
    (pickup_width * SHIMMER_PICKUP_WIDTH_SCALE).max(SHIMMER_PICKUP_WIDTH_MIN)
}

fn shimmer_radiation_gain(width: usize, height: usize) -> f32 {
    let density = (width * height) as f32 / MAX_MESH_CELLS as f32;
    SHIMMER_RADIATION_GAIN * density.clamp(0.0, 1.0).powf(SHIMMER_RADIATION_DENSITY_EXP)
}

impl RectangularMesh2d {
    fn new(config: RectangularMesh2dConfig) -> Self {
        let config = config.sanitized();
        // Buffers and boundary/spatial-weight storage are allocated once at the maximum
        // grid; `config.width/height` is the active sub-region (stride = active width),
        // so `reconfigure` can resize the live grid without allocating (ADR-0001).
        Self {
            config,
            current: DirectionalWaves::new(MAX_MESH_CELLS),
            next: DirectionalWaves::new(MAX_MESH_CELLS),
            source_weights: SpatialWeights::new(
                config.strike_position,
                config.width,
                config.height,
                config.excitation_width,
                SpatialNormalization::UnitEnergy,
            ),
            pickup_weights: SpatialWeights::new(
                config.pickup_position,
                config.width,
                config.height,
                config.pickup_width,
                SpatialNormalization::ApertureIntegral,
            ),
            shimmer_weights: SpatialWeights::new(
                config.pickup_position,
                config.width,
                config.height,
                shimmer_pickup_width(config.pickup_width),
                SpatialNormalization::UnitEnergy,
            ),
            boundary_lowpass: BoundaryLowpass::new(
                MAX_MESH_WIDTH,
                MAX_MESH_HEIGHT,
                config.sample_rate,
            ),
            geometric_drive: 0.0,
            level_compensation: mesh_level_compensation(config.width, config.height),
        }
    }

    /// Set the measured-energy drive for the geometric (von Kármán) coupling (M2
    /// energy bus). Called once per host sample by the resonator engine; defaults
    /// to 0.0 so callers that never set it render the linear mesh unchanged.
    fn set_geometric_drive(&mut self, drive: f32) {
        self.geometric_drive = drive_term(drive);
    }

    /// Adopt a new configuration without reallocating: the grid (and therefore
    /// all buffers) is fixed at construction, so only the non-grid fields and the
    /// in-place spatial weights change. Allocation-free for live re-tuning.
    fn reconfigure(&mut self, config: RectangularMesh2dConfig) {
        // The active grid (`width`/`height`) may change here — buffers are already
        // allocated at the maximum, so re-tuning the live grid stays allocation-free.
        let config = config.sanitized();
        self.config = config;
        self.level_compensation = mesh_level_compensation(config.width, config.height);
        self.boundary_lowpass.set_sample_rate(config.sample_rate);
        self.source_weights.recompute(
            config.strike_position,
            config.width,
            config.height,
            config.excitation_width,
        );
        self.pickup_weights.recompute(
            config.pickup_position,
            config.width,
            config.height,
            config.pickup_width,
        );
        self.shimmer_weights.recompute(
            config.pickup_position,
            config.width,
            config.height,
            shimmer_pickup_width(config.pickup_width),
        );
    }

    fn process_sample(&mut self, excitation: f32) -> f32 {
        let aperture_pressure = self.pickup_weights.pressure(&self.current);
        let shimmer_pressure = self.shimmer_weights.pressure(&self.current);
        let curvature_pressure = self.shimmer_weights.curvature_pressure(
            &self.current,
            self.config.width,
            self.config.height,
        );
        let shimmer = shimmer_pressure + CURVATURE_RADIATION_GAIN * curvature_pressure;
        let output = (aperture_pressure
            + shimmer_radiation_gain(self.config.width, self.config.height) * shimmer)
            * self.level_compensation;
        // Read the radiating mesh state before applying this sample's strike force.
        // Otherwise nearby source/pickup apertures create a direct, unpropagated
        // hammer-to-output spike instead of a struck-body response.
        let excitation = math::snap_to_zero(excitation);
        if excitation.abs() > f32::EPSILON {
            let contact_envelope = excitation.abs().sqrt().min(1.0);
            let contact_amount = self.config.strike_contact_absorption
                * contact_envelope
                * STRIKE_CONTACT_MOTION_DAMPING;
            self.source_weights
                .absorb_contact_motion(&mut self.current, contact_amount);
            let local_motion = self.source_weights.pressure(&self.current);
            let aligned_motion = (local_motion * excitation.signum()).max(0.0);
            let max_loss = excitation.abs() * STRIKE_CONTACT_MAX_LOSS_FRACTION;
            let contact_loss =
                (self.config.strike_contact_absorption * contact_envelope * aligned_motion)
                    .min(max_loss);
            let conditioned_magnitude = excitation.abs() - contact_loss;
            self.source_weights.inject_pressure(
                &mut self.current,
                excitation.signum() * conditioned_magnitude,
            );
        }
        self.scatter_and_propagate();
        math::snap_to_zero(output)
    }

    fn reset(&mut self) {
        self.current.clear();
        self.next.clear();
        self.boundary_lowpass.clear();
        self.geometric_drive = 0.0;
    }

    fn scatter_and_propagate(&mut self) {
        self.next.clear();
        for y in 0..self.config.height {
            for x in 0..self.config.width {
                self.scatter_junction(x, y);
            }
        }
        std::mem::swap(&mut self.current, &mut self.next);
    }

    fn scatter_junction(&mut self, x: usize, y: usize) {
        let index = self.index(x, y);
        let pressure = self.current.pressure(index);
        let left = pressure - self.current.from_left[index];
        let right = pressure - self.current.from_right[index];
        let top = pressure - self.current.from_top[index];
        let bottom = pressure - self.current.from_bottom[index];

        // Geometric (von Kármán) coupling: at high amplitude the energy-conserving
        // junction rotation steers energy from the low mode up into higher modes
        // (the gong bloom). Inert at zero drive.
        let (left, right, top, bottom) =
            geometric_couple((left, right, top, bottom), pressure, self.geometric_drive);

        self.propagate_left(x, y, left);
        self.propagate_right(x, y, right);
        self.propagate_top(x, y, top);
        self.propagate_bottom(x, y, bottom);
    }

    fn propagate_left(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if x == 0 {
            let coeff = self.boundary_lowpass.coeff;
            let filtered = boundary_lowpass_step(
                &mut self.boundary_lowpass.left[y],
                coeff,
                self.config.boundary_hf_loss,
                sample,
            );
            self.next.from_left[index] += filtered * self.config.boundary.left.reflection();
        } else {
            let neighbor = self.index(x - 1, y);
            self.next.from_right[neighbor] += sample;
        }
    }

    fn propagate_right(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if x + 1 == self.config.width {
            let coeff = self.boundary_lowpass.coeff;
            let filtered = boundary_lowpass_step(
                &mut self.boundary_lowpass.right[y],
                coeff,
                self.config.boundary_hf_loss,
                sample,
            );
            self.next.from_right[index] += filtered * self.config.boundary.right.reflection();
        } else {
            let neighbor = self.index(x + 1, y);
            self.next.from_left[neighbor] += sample;
        }
    }

    fn propagate_top(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if y == 0 {
            let coeff = self.boundary_lowpass.coeff;
            let filtered = boundary_lowpass_step(
                &mut self.boundary_lowpass.top[x],
                coeff,
                self.config.boundary_hf_loss,
                sample,
            );
            self.next.from_top[index] += filtered * self.config.boundary.top.reflection();
        } else {
            let neighbor = self.index(x, y - 1);
            self.next.from_bottom[neighbor] += sample;
        }
    }

    fn propagate_bottom(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if y + 1 == self.config.height {
            let coeff = self.boundary_lowpass.coeff;
            let filtered = boundary_lowpass_step(
                &mut self.boundary_lowpass.bottom[x],
                coeff,
                self.config.boundary_hf_loss,
                sample,
            );
            self.next.from_bottom[index] += filtered * self.config.boundary.bottom.reflection();
        } else {
            let neighbor = self.index(x, y + 1);
            self.next.from_top[neighbor] += sample;
        }
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * self.config.width + x
    }
}
