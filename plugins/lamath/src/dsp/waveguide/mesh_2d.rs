use lindelion_dsp_utils::math;

use super::core;

mod runtime;
pub use runtime::{MeshResonator, MeshVoiceParams};

const MIN_MESH_SIZE: usize = 3;
const MAX_MESH_SIZE: usize = 48;

/// Measured-energy (RMS) at which the geometric (von Kármán) coupling reaches its
/// target depth; the squared, normalized drive `(energy/REF)^2` keeps soft strikes
/// linear and concentrates the bloom on hard ones.
const GEOMETRIC_ENERGY_REF: f32 = 0.15;
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

    #[cfg(test)]
    fn fixed_edges(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left: MeshBoundaryEdge::fixed(left),
            right: MeshBoundaryEdge::fixed(right),
            top: MeshBoundaryEdge::fixed(top),
            bottom: MeshBoundaryEdge::fixed(bottom),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RectangularMesh2dConfig {
    width: usize,
    height: usize,
    sample_rate: f32,
    wave_speed_mps: f32,
    physical_width_m: f32,
    physical_height_m: f32,
    boundary: MeshBoundaryConfig,
    strike_position: MeshPoint,
    pickup_position: MeshPoint,
    excitation_width: f32,
    pickup_width: f32,
}

impl RectangularMesh2dConfig {
    fn sanitized(self) -> Self {
        Self {
            width: self.width.clamp(MIN_MESH_SIZE, MAX_MESH_SIZE),
            height: self.height.clamp(MIN_MESH_SIZE, MAX_MESH_SIZE),
            sample_rate: core::sanitize_sample_rate(self.sample_rate),
            wave_speed_mps: math::finite_clamp(self.wave_speed_mps, 1.0, 4_000.0, 220.0),
            physical_width_m: math::finite_clamp(self.physical_width_m, 0.01, 10.0, 0.7),
            physical_height_m: math::finite_clamp(self.physical_height_m, 0.01, 10.0, 0.45),
            boundary: self.boundary,
            strike_position: self.strike_position,
            pickup_position: self.pickup_position,
            excitation_width: math::finite_clamp(self.excitation_width, 0.005, 0.4, 0.06),
            pickup_width: math::finite_clamp(self.pickup_width, 0.005, 0.4, 0.025),
        }
    }

    #[cfg(test)]
    fn mode_frequency_hz(self, mode_x: usize, mode_y: usize) -> f32 {
        let config = self.sanitized();
        let kx = mode_x as f32 / config.physical_width_m;
        let ky = mode_y as f32 / config.physical_height_m;
        config.wave_speed_mps * 0.5 * (kx * kx + ky * ky).sqrt()
    }
}

impl Default for RectangularMesh2dConfig {
    fn default() -> Self {
        Self {
            width: 14,
            height: 10,
            sample_rate: 48_000.0,
            wave_speed_mps: 220.0,
            physical_width_m: 0.72,
            physical_height_m: 0.48,
            boundary: MeshBoundaryConfig::fixed(0.16),
            strike_position: MeshPoint::new(0.35, 0.42),
            pickup_position: MeshPoint::new(0.72, 0.58),
            excitation_width: 0.055,
            pickup_width: 0.025,
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
}

impl SpatialWeights {
    fn new(point: MeshPoint, width: usize, height: usize, width_fraction: f32) -> Self {
        // Capacity is the full grid, so later in-place recomputes never reallocate.
        let mut weights = Self {
            weights: Vec::with_capacity(width * height),
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

        for weight in &mut self.weights {
            weight.weight /= sum;
        }
    }

    fn inject_pressure(&self, waves: &mut DirectionalWaves, pressure: f32) {
        let component = math::snap_to_zero(pressure) * 0.25;
        for spatial_weight in &self.weights {
            waves.add_uniform(spatial_weight.index, component * spatial_weight.weight);
        }
    }

    fn pressure(&self, waves: &DirectionalWaves) -> f32 {
        self.weights
            .iter()
            .map(|spatial_weight| waves.pressure(spatial_weight.index) * spatial_weight.weight)
            .sum()
    }
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

    #[cfg(test)]
    fn energy(&self) -> f32 {
        self.from_left
            .iter()
            .chain(self.from_right.iter())
            .chain(self.from_top.iter())
            .chain(self.from_bottom.iter())
            .map(|sample| sample * sample)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RectangularMesh2d {
    config: RectangularMesh2dConfig,
    current: DirectionalWaves,
    next: DirectionalWaves,
    source_weights: SpatialWeights,
    pickup_weights: SpatialWeights,
    // Measured resonator energy (M2 bus) driving the geometric (von Kármán)
    // coupling; set per host sample, constant across the 2x sub-samples. 0.0 => inert.
    geometric_drive: f32,
}

impl RectangularMesh2d {
    fn new(config: RectangularMesh2dConfig) -> Self {
        let config = config.sanitized();
        let len = config.width * config.height;
        Self {
            config,
            current: DirectionalWaves::new(len),
            next: DirectionalWaves::new(len),
            source_weights: SpatialWeights::new(
                config.strike_position,
                config.width,
                config.height,
                config.excitation_width,
            ),
            pickup_weights: SpatialWeights::new(
                config.pickup_position,
                config.width,
                config.height,
                config.pickup_width,
            ),
            geometric_drive: 0.0,
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
        let config = RectangularMesh2dConfig {
            width: self.config.width,
            height: self.config.height,
            ..config
        }
        .sanitized();
        self.config = config;
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
    }

    fn process_sample(&mut self, excitation: f32) -> f32 {
        self.source_weights
            .inject_pressure(&mut self.current, excitation);
        let output = self.pickup_weights.pressure(&self.current);
        self.scatter_and_propagate();
        math::snap_to_zero(output)
    }

    fn reset(&mut self) {
        self.current.clear();
        self.next.clear();
        self.geometric_drive = 0.0;
    }

    #[cfg(test)]
    fn total_energy(&self) -> f32 {
        self.current.energy()
    }

    #[cfg(test)]
    fn mode_frequency_hz(&self, mode_x: usize, mode_y: usize) -> f32 {
        self.config.mode_frequency_hz(mode_x, mode_y)
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
            self.next.from_left[index] += sample * self.config.boundary.left.reflection();
        } else {
            let neighbor = self.index(x - 1, y);
            self.next.from_right[neighbor] += sample;
        }
    }

    fn propagate_right(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if x + 1 == self.config.width {
            self.next.from_right[index] += sample * self.config.boundary.right.reflection();
        } else {
            let neighbor = self.index(x + 1, y);
            self.next.from_left[neighbor] += sample;
        }
    }

    fn propagate_top(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if y == 0 {
            self.next.from_top[index] += sample * self.config.boundary.top.reflection();
        } else {
            let neighbor = self.index(x, y - 1);
            self.next.from_bottom[neighbor] += sample;
        }
    }

    fn propagate_bottom(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if y + 1 == self.config.height {
            self.next.from_bottom[index] += sample * self.config.boundary.bottom.reflection();
        } else {
            let neighbor = self.index(x, y + 1);
            self.next.from_top[neighbor] += sample;
        }
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * self.config.width + x
    }
}

#[cfg(test)]
mod promotion_tests;

#[cfg(test)]
fn render_mesh(
    config: RectangularMesh2dConfig,
    sample_count: usize,
    excitation: crate::dsp::render_metrics::RenderExcitation,
) -> Vec<f32> {
    let config = config.sanitized();
    let mut mesh = RectangularMesh2d::new(config);
    crate::dsp::render_metrics::render_response(
        config.sample_rate,
        mesh.mode_frequency_hz(1, 1),
        sample_count,
        excitation,
        |sample| mesh.process_sample(sample),
    )
}

#[cfg(test)]
fn mode_frequency(config: RectangularMesh2dConfig) -> f32 {
    config.mode_frequency_hz(1, 1)
}

#[cfg(test)]
fn render_mesh_with_drive(
    config: RectangularMesh2dConfig,
    sample_count: usize,
    drive: impl Fn(usize) -> f32,
) -> Vec<f32> {
    let config = config.sanitized();
    let mut mesh = RectangularMesh2d::new(config);
    let mut index = 0usize;
    crate::dsp::render_metrics::render_response(
        config.sample_rate,
        mesh.mode_frequency_hz(1, 1),
        sample_count,
        crate::dsp::render_metrics::RenderExcitation::ShapedPluck,
        |sample| {
            mesh.set_geometric_drive(drive(index));
            index += 1;
            mesh.process_sample(sample)
        },
    )
}

#[cfg(test)]
mod tests;
