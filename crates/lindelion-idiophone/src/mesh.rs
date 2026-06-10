use lindelion_dsp_utils::math;

use super::sanitize_sample_rate;

#[path = "boundary.rs"]
mod boundary;
mod constants;

use constants::*;
#[path = "runtime.rs"]
mod runtime;
#[path = "mesh/spatial.rs"]
mod spatial;
use boundary::{
    BoundaryLowpass, DEFAULT_BOUNDARY_HF_LOSS, MeshBoundaryEdge, boundary_lowpass_step,
};
pub use runtime::{MeshResonator, MeshVoiceParams};
use spatial::{SpatialNormalization, SpatialWeights};

/// Squared, normalized energy term in `[0, GEOMETRIC_MAX_DRIVE]` setting how
/// strongly the mesh couples at the current playing energy.
fn drive_term(energy: f32) -> f32 {
    let normalized = math::finite_or(energy, 0.0).max(0.0) / GEOMETRIC_ENERGY_REF;
    math::finite_clamp(normalized * normalized, 0.0, GEOMETRIC_MAX_DRIVE, 0.0)
}

/// Per-sample geometric coupling coefficient `GEOMETRIC_MAX_SIN * drive`, conditioned once before
/// the junction loop. `0.0` means the coupling is inert this sample (no drive). `drive` is already
/// in `[0, GEOMETRIC_MAX_DRIVE]` and finite (it comes from [`drive_term`]), so the clamp here is a
/// belt-and-braces guard that runs once per sample rather than once per junction.
fn geometric_sin_coeff(drive: f32) -> f32 {
    let drive = math::finite_clamp(drive, 0.0, GEOMETRIC_MAX_DRIVE, 0.0);
    if drive <= f32::EPSILON {
        0.0
    } else {
        GEOMETRIC_MAX_SIN * drive
    }
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

    /// Zero the half-open cell range `[start, end)`. Used when the active grid grows, to reset the
    /// newly exposed cells (which may hold stale data from an earlier larger grid) without touching
    /// the live ring in `[0, start)`.
    fn clear_range(&mut self, start: usize, end: usize) {
        let end = end.min(self.from_left.len());
        if start >= end {
            return;
        }
        self.from_left[start..end].fill(0.0);
        self.from_right[start..end].fill(0.0);
        self.from_top[start..end].fill(0.0);
        self.from_bottom[start..end].fill(0.0);
    }

    fn pressure(&self, index: usize) -> f32 {
        // No software denormal/NaN snap here: this is the per-junction hot path, and the audio
        // thread runs in hardware flush-to-zero (see `flush_denormals_on_this_thread`). The mesh is
        // energy-bounded, so finite inputs stay finite; dropping the `is_finite` branch lets the
        // scatter pass auto-vectorize. The once-per-sample output is still snapped at the boundary.
        0.5 * (self.from_left[index]
            + self.from_right[index]
            + self.from_top[index]
            + self.from_bottom[index])
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
    // Per-cell outgoing waves for the current sample, filled by the scatter pass and consumed by the
    // gather pass (allocated once at the max grid). Splitting scatter into a compute-outgoing pass and
    // a gather-into-next pass removes the neighbor write-conflicts, so both passes are branchless and
    // data-parallel (auto-vectorizable) instead of a scatter with per-cell boundary branches.
    out_left: Vec<f32>,
    out_right: Vec<f32>,
    out_top: Vec<f32>,
    out_bottom: Vec<f32>,
    // Per-cell junction pressure for the current sample, shared between the linear scatter pass and
    // the geometric-coupling pass (so the coupling loop is a branchless, vectorizable kernel).
    pressure_buf: Vec<f32>,
    // Number of active grid cells (`width * height`). The scatter/gather passes only touch this
    // prefix of the max-sized buffers; tracked so `reconfigure` can zero newly grown cells.
    active_cells: usize,
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
            out_left: vec![0.0; MAX_MESH_CELLS],
            out_right: vec![0.0; MAX_MESH_CELLS],
            out_top: vec![0.0; MAX_MESH_CELLS],
            out_bottom: vec![0.0; MAX_MESH_CELLS],
            pressure_buf: vec![0.0; MAX_MESH_CELLS],
            active_cells: config.width * config.height,
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
        let new_cells = config.width * config.height;
        // The active region is always the contiguous prefix `[0, cells)`. When the grid grows, the
        // newly exposed cells may hold stale data from an earlier larger grid; zero just those so the
        // first read after the grow sees a fresh (silent) plate area while the live ring is preserved.
        if new_cells > self.active_cells {
            self.current.clear_range(self.active_cells, new_cells);
            self.next.clear_range(self.active_cells, new_cells);
        }
        self.active_cells = new_cells;
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

    /// Energy-conserving rotation in the plane of the locally-uniform mode `u=(1,1,1,1)/2` and
    /// the alternating mode `v=(1,-1,1,-1)/2`, steering energy from `u` up into `v` (higher
    /// modes). `cos = sqrt(1-sin^2)` keeps `cu^2+cv^2` exact. Hoisting the per-sample
    /// `sin_coeff` gate out of the loop leaves an unconditional body, so the sqrt vectorizes
    /// (packed `sqrtps`) instead of running scalar.
    #[inline]
    fn geometric_coupling_pass(&mut self, sin_coeff: f32, active: usize) {
        let RectangularMesh2d {
            out_left,
            out_right,
            out_top,
            out_bottom,
            pressure_buf,
            ..
        } = self;
        // Iterate via zipped iterators over distinct slices: this hands LLVM the non-aliasing
        // guarantee (and elides bounds checks) it needs to vectorize the sqrt to packed `sqrtps`.
        let left = &mut out_left[..active];
        let right = &mut out_right[..active];
        let top = &mut out_top[..active];
        let bottom = &mut out_bottom[..active];
        let pressure = &pressure_buf[..active];
        for ((((l, r), t), b), &p) in left
            .iter_mut()
            .zip(right.iter_mut())
            .zip(top.iter_mut())
            .zip(bottom.iter_mut())
            .zip(pressure.iter())
        {
            let amplitude = (p.abs() * GEOMETRIC_AMPLITUDE_SENS).clamp(0.0, 1.0);
            let sin = sin_coeff * amplitude;
            let cos = (1.0 - sin * sin).max(0.0).sqrt();
            let cu = 0.5 * (*l + *r + *t + *b);
            let cv = 0.5 * (*l - *r + *t - *b);
            let du = 0.5 * ((cu * cos - cv * sin) - cu);
            let dv = 0.5 * ((cu * sin + cv * cos) - cv);
            *l = *l + du + dv;
            *r = *r + du - dv;
            *t = *t + du + dv;
            *b = *b + du - dv;
        }
    }

    fn scatter_and_propagate(&mut self) {
        let sin_coeff = geometric_sin_coeff(self.geometric_drive);
        let width = self.config.width;
        let height = self.config.height;
        let active = self.active_cells;

        // Pass 1a — linear scatter: each cell's junction pressure and four outgoing waves, computed
        // independently (no neighbor writes). Branchless, contiguous SoA → auto-vectorizes.
        {
            let RectangularMesh2d {
                current,
                out_left,
                out_right,
                out_top,
                out_bottom,
                pressure_buf,
                ..
            } = self;
            // Bind distinct length-`active` slices so LLVM sees non-aliasing reads/writes (and elides
            // bounds checks), letting this linear pass vectorize to packed adds/subtracts.
            let fl = &current.from_left[..active];
            let fr = &current.from_right[..active];
            let ft = &current.from_top[..active];
            let fb = &current.from_bottom[..active];
            let ol = &mut out_left[..active];
            let or_ = &mut out_right[..active];
            let ot = &mut out_top[..active];
            let ob = &mut out_bottom[..active];
            let pb = &mut pressure_buf[..active];
            for i in 0..active {
                let pressure = 0.5 * (fl[i] + fr[i] + ft[i] + fb[i]);
                pb[i] = pressure;
                ol[i] = pressure - fl[i];
                or_[i] = pressure - fr[i];
                ot[i] = pressure - ft[i];
                ob[i] = pressure - fb[i];
            }
        }

        // Pass 1b — geometric (von Kármán) coupling, only when the bloom is engaged.
        if sin_coeff > 0.0 {
            self.geometric_coupling_pass(sin_coeff, active);
        }

        // Pass 2 — gather: each cell's next incoming wave on an edge is the neighbor's outgoing wave
        // toward it (a shifted read), or, at the grid edge, its own outgoing wave reflected through
        // the per-edge boundary low-pass. Every active cell is assigned exactly once per direction,
        // so no pre-clear is needed.
        let refl_left = self.config.boundary.left.reflection();
        let refl_right = self.config.boundary.right.reflection();
        let refl_top = self.config.boundary.top.reflection();
        let refl_bottom = self.config.boundary.bottom.reflection();
        let hf_loss = self.config.boundary_hf_loss;
        let coeff = self.boundary_lowpass.coeff;
        {
            let RectangularMesh2d {
                next,
                out_left,
                out_right,
                out_top,
                out_bottom,
                boundary_lowpass,
                ..
            } = self;
            // Horizontal neighbors: `from_left[i] = out_right[i - 1]`, `from_right[i] = out_left[i + 1]`,
            // with the left/right columns reflecting their own outgoing wave.
            for y in 0..height {
                let row = y * width;
                next.from_left[row] = boundary_lowpass_step(
                    &mut boundary_lowpass.left[y],
                    coeff,
                    hf_loss,
                    out_left[row],
                ) * refl_left;
                for x in 1..width {
                    next.from_left[row + x] = out_right[row + x - 1];
                }
                for x in 0..width - 1 {
                    next.from_right[row + x] = out_left[row + x + 1];
                }
                next.from_right[row + width - 1] = boundary_lowpass_step(
                    &mut boundary_lowpass.right[y],
                    coeff,
                    hf_loss,
                    out_right[row + width - 1],
                ) * refl_right;
            }
            // Vertical neighbors: `from_top[i] = out_bottom[i - width]`, `from_bottom[i] = out_top[i + width]`,
            // with the top/bottom rows reflecting their own outgoing wave.
            for (x, &out) in out_top.iter().enumerate().take(width) {
                next.from_top[x] =
                    boundary_lowpass_step(&mut boundary_lowpass.top[x], coeff, hf_loss, out)
                        * refl_top;
            }
            next.from_top[width..active].copy_from_slice(&out_bottom[..active - width]);
            next.from_bottom[..active - width].copy_from_slice(&out_top[width..active]);
            let last_row = (height - 1) * width;
            for x in 0..width {
                next.from_bottom[last_row + x] = boundary_lowpass_step(
                    &mut boundary_lowpass.bottom[x],
                    coeff,
                    hf_loss,
                    out_bottom[last_row + x],
                ) * refl_bottom;
            }
        }
        std::mem::swap(&mut self.current, &mut self.next);
    }
}
