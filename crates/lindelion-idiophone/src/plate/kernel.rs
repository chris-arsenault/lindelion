//! Explicit FDTD kernel for the Kirchhoff plate with tension and σ₀/σ₁ losses
//! (scheme and references in the parent module / ADR-0050):
//!
//! `(1+σ₀k)·u⁺ = 2u − (1−σ₀k)·u⁻ + k²(−κ²δΔΔ + c²δΔ)u + 2σ₁k·δΔ(u − u⁻)`
//!
//! `δΔ` is the 5-point Laplacian; `δΔΔ = δΔ∘δΔ` is computed as two passes (exactly the
//! 13-point biharmonic stencil), with the second pass fused into the update loop. State is
//! four buffers (`u`, `u_prev`, `lap`, `lap_prev`) allocated once at the maximum grid; the
//! active grid is the contiguous row-major prefix with stride = active width, so live
//! re-tuning never allocates (ADR-0001). Free edges are the natural boundary condition of
//! the discrete plate energy (Neumann-truncated graph Laplacian; see
//! `neumann_edge_laplacian`). No per-cell denormal guards: the audio thread runs in
//! hardware flush-to-zero and the once-per-sample output is snapped at the boundary — the
//! same policy the membrane kernel documented.

use lindelion_dsp_utils::math;

use super::neumann::{neumann_edge_laplacian, neumann_edge_update};
use super::spatial::SpatialWeights;
use super::{PLATE_MAX_CELLS, PlateBoundary, PlateConfig, PlateGrid};

/// Local strain runs ~tens of times the spatial mean at the contact; the local
/// modulation normalization is scaled down from the mean-calibrated drive so only the
/// hottest cells approach full depth (keeps the glide in the musical band).
const LOCAL_STRAIN_RATIO: f32 = 1.0;

/// Bloom follower: onset swells over a few ms, release lets the tail disengage.
const BLOOM_ATTACK_SECONDS: f32 = 0.003;
const BLOOM_RELEASE_SECONDS: f32 = 0.080;

#[derive(Debug, Clone, PartialEq)]
pub struct PlateKernel {
    config: PlateConfig,
    grid: PlateGrid,
    u: Vec<f32>,
    u_prev: Vec<f32>,
    lap: Vec<f32>,
    lap_prev: Vec<f32>,
    /// Edge-weighted tension force `Tᵢ = Σₑ wₑ(uⱼ − uᵢ)` (divergence-form tension): the
    /// energy-conserving home of both the linear tension term and the bloom modulation
    /// (a network of stiffening springs — the discrete gradient of `Σₑ φ(sₑ)`).
    tension_force: Vec<f32>,
    edge_force_x: Vec<f32>,
    edge_force_y: Vec<f32>,
    active_cells: usize,
    inv_h2: f32,
    coeff_velocity: f32,
    coeff_lap2: f32,
    /// Linear tension coefficient `c²k²/(1+σ₀k)` applied through the (already computed)
    /// Laplacian — identical to the uniform edge form, at zero extra cost.
    coeff_lap: f32,
    /// Tension-modulation span in `coeff_lap` units: at full drive the effective tension
    /// pins at the closed-form stability budget for this grid (M3, ADR-0050).
    coeff_lap_span: f32,
    coeff_s1: f32,
    coeff_force: f32,
    /// Plate-internal mean bending strain `mean(−u·Lu)` (the area-averaged `|∇u|²` —
    /// Berger's intensive strain measure, so plate size doesn't dilute the bloom),
    /// smoothed by an attack/release follower; the bloom's drive source.
    bend_energy_smoothed: f32,
    bend_attack: f32,
    bend_release: f32,
    /// Maps smoothed bend energy to drive ∈ [0, 1]; 0 disables the bloom.
    drive_normalization: f32,
    /// Fraction of the stability headroom the bloom spends at full drive. The clamp budget
    /// is much deeper than musical: spending it all would glide tension-dominated low
    /// modes far past the real-gong ~25% (set by the runtime; 1 = full budget).
    bloom_depth: f32,
    /// Glide dose ∈ [0, 1]: fraction of the modulation's DC (slow stiffening) component
    /// kept. The audio-rate cascade always passes at full strength; the DC part is what
    /// reads as a pitch sweep over a long ring (M4 Ride verdict). Stiff plates keep less.
    glide_dose: f32,
    /// Floor on the (DC-subtracted) modulation so the total edge stiffness stays ≥ ~0.
    modulation_floor: f32,
    /// Spatial mean of the per-edge modulation from the previous sample: the DC estimate
    /// the glide dose subtracts (envelope-slow, so the one-sample lag is irrelevant).
    mean_modulation: f32,
    tension_drive: f32,
    strike_index: usize,
    pickup_index: usize,
}

impl PlateKernel {
    pub fn new(config: PlateConfig) -> Self {
        let mut kernel = Self {
            config: config.sanitized(),
            grid: PlateGrid::for_config(&config),
            u: vec![0.0; PLATE_MAX_CELLS],
            u_prev: vec![0.0; PLATE_MAX_CELLS],
            lap: vec![0.0; PLATE_MAX_CELLS],
            lap_prev: vec![0.0; PLATE_MAX_CELLS],
            tension_force: vec![0.0; PLATE_MAX_CELLS],
            edge_force_x: vec![0.0; PLATE_MAX_CELLS],
            edge_force_y: vec![0.0; PLATE_MAX_CELLS],
            active_cells: 0,
            inv_h2: 0.0,
            coeff_velocity: 0.0,
            coeff_lap2: 0.0,
            coeff_lap: 0.0,
            coeff_lap_span: 0.0,
            coeff_s1: 0.0,
            coeff_force: 0.0,
            bend_energy_smoothed: 0.0,
            bend_attack: 0.0,
            bend_release: 0.0,
            drive_normalization: 0.0,
            bloom_depth: 1.0,
            glide_dose: 1.0,
            modulation_floor: 0.0,
            mean_modulation: 0.0,
            tension_drive: 0.0,
            strike_index: 0,
            pickup_index: 0,
        };
        kernel.active_cells = kernel.grid.cells();
        kernel.derive_coefficients();
        kernel
    }

    pub fn grid(&self) -> PlateGrid {
        self.grid
    }

    pub fn config(&self) -> &PlateConfig {
        &self.config
    }

    /// Adopt a new physical configuration without allocating. The active region is the
    /// contiguous prefix; newly exposed cells (grid grew) are zeroed so the fresh plate
    /// area is silent, while the live ring on the retained prefix is preserved. `lap_prev`
    /// carried across a grid change is stale for one sample (bounded, transient — the same
    /// accepted artifact class as the membrane's in-place re-tune).
    pub fn reconfigure(&mut self, config: PlateConfig) {
        self.config = config.sanitized();
        self.grid = PlateGrid::for_config(&self.config);
        let new_cells = self.grid.cells();
        if new_cells > self.active_cells {
            for buffer in [
                &mut self.u,
                &mut self.u_prev,
                &mut self.lap,
                &mut self.lap_prev,
            ] {
                buffer[self.active_cells..new_cells].fill(0.0);
            }
        }
        self.active_cells = new_cells;
        self.derive_coefficients();
        self.strike_index = self.strike_index.min(new_cells - 1);
        self.pickup_index = self.pickup_index.min(new_cells - 1);
    }

    /// Set the strike and pickup cell indices (row-major on the active grid). Spatially
    /// weighted taps arrive with the runtime wiring; the kernel reads/writes single cells.
    pub fn set_taps(&mut self, strike_index: usize, pickup_index: usize) {
        self.strike_index = strike_index.min(self.active_cells.saturating_sub(1));
        self.pickup_index = pickup_index.min(self.active_cells.saturating_sub(1));
    }

    pub fn reset(&mut self) {
        self.u.fill(0.0);
        self.u_prev.fill(0.0);
        self.lap.fill(0.0);
        self.lap_prev.fill(0.0);
        self.bend_energy_smoothed = 0.0;
        self.tension_drive = 0.0;
        self.mean_modulation = 0.0;
    }

    /// Smoothed plate-internal bending energy (observability for calibration tests).
    #[cfg(test)]
    pub(crate) fn bend_energy(&self) -> f32 {
        self.bend_energy_smoothed
    }

    /// Current (smoothed) bloom drive ∈ [0, 1] (observability for calibration tests).
    #[cfg(test)]
    pub(crate) fn tension_drive(&self) -> f32 {
        self.tension_drive
    }

    /// Calibration from smoothed bend energy to drive (set by the runtime; 0 = bloom off).
    pub(crate) fn set_drive_normalization(&mut self, normalization: f32) {
        self.drive_normalization = math::finite_clamp(normalization, 0.0, f32::MAX, 0.0);
    }

    /// Fraction of the stability headroom spent at full drive (re-derives coefficients).
    pub(crate) fn set_bloom_depth(&mut self, depth: f32) {
        self.bloom_depth = math::finite_clamp(depth, 0.0, 1.0, 0.0);
        self.derive_coefficients();
    }

    /// Fraction of the modulation's DC (glide) component kept (1 = full gong glide).
    pub(crate) fn set_glide_dose(&mut self, dose: f32) {
        self.glide_dose = math::finite_clamp(dose, 0.0, 1.0, 1.0);
    }

    /// Advance one sample: read the pickup from the *current* state (a strike this sample
    /// must propagate before it can be heard — the membrane's pickup-ordering contract),
    /// then inject `force` (force density per unit ρh) and step the scheme.
    pub fn process_sample(&mut self, force: f32) -> f32 {
        let output = self.u[self.pickup_index];
        self.compute_laplacian();
        self.update();
        if self.config.boundary == PlateBoundary::Free {
            self.remove_rigid_offset();
        }
        let force = math::snap_to_zero(force);
        if force != 0.0 {
            self.u_prev[self.strike_index] += self.coeff_force * force;
        }
        std::mem::swap(&mut self.u, &mut self.u_prev);
        std::mem::swap(&mut self.lap, &mut self.lap_prev);
        math::snap_to_zero(output)
    }

    /// Weighted transverse velocity (m/s) over the current state pair: the voice pickup.
    pub(crate) fn weighted_velocity(&self, weights: &SpatialWeights) -> f32 {
        let velocity_scale = self.config.sample_rate;
        weights
            .iter()
            .map(|(index, weight)| (self.u[index] - self.u_prev[index]) * weight)
            .sum::<f32>()
            * velocity_scale
    }

    /// Voice-path step: read the weighted-velocity pickup from the *current* state (the
    /// pickup-ordering contract: a strike this sample must propagate before it is heard),
    /// step the scheme, then apply the stick contact — local velocity damping under the
    /// source aperture (a stick touching an already-moving plate absorbs local motion;
    /// membrane behavior carried over) — and inject the strike force through the source
    /// weights into the new state.
    #[cfg(test)]
    pub(crate) fn process_voice_sample(
        &mut self,
        force: f32,
        source: &SpatialWeights,
        pickup: &SpatialWeights,
        contact_amount: f32,
    ) -> f32 {
        let output = self.weighted_velocity(pickup);
        self.step_voice(force, source, contact_amount);
        math::snap_to_zero(output)
    }

    /// Advance one sample without a pickup read (the runtime reads its crossfading taps
    /// before stepping, preserving the pickup-ordering contract).
    pub(crate) fn step_voice(&mut self, force: f32, source: &SpatialWeights, contact_amount: f32) {
        self.compute_laplacian();
        self.update();
        if self.config.boundary == PlateBoundary::Free {
            self.remove_rigid_offset();
        }
        let force = math::snap_to_zero(force);
        if force != 0.0 {
            let amount = math::finite_clamp(contact_amount, 0.0, 0.95, 0.0);
            for (index, weight) in source.iter() {
                if amount > 0.0 {
                    let keep = 1.0 - (amount * weight.abs()).min(0.95);
                    self.u_prev[index] =
                        self.u[index] + (self.u_prev[index] - self.u[index]) * keep;
                }
                self.u_prev[index] += self.coeff_force * force * weight;
            }
        }
        std::mem::swap(&mut self.u, &mut self.u_prev);
        std::mem::swap(&mut self.lap, &mut self.lap_prev);
    }

    /// Assemble the divergence-form tension force `Tᵢ = Σₑ wₑ(uⱼ − uᵢ)` over interior
    /// grid edges. Per-edge weight `wₑ = c² + span·min(γ·sₑ, 1)` with the edge strain
    /// `sₑ = (Δu)²/h²` — a network of stiffening springs: the exact discrete gradient of
    /// the potential `Σₑ φ(sₑ)`, so the modulation is energy-conserving by construction
    /// (no parametric pump). Weights are clamped inside the closed-form stability budget;
    /// missing edges at the boundary are the natural (free/Neumann) condition, and pinned
    /// boundaries zero their ring each step regardless.
    /// Assemble the NONLINEAR part of the divergence-form tension (the linear part rides
    /// the Laplacian at zero cost). Per-edge stiffening weight `min(γ·sₑ, 1)·span`, exact
    /// discrete gradient of the stiffening potential → energy-conserving, no parametric
    /// pump. Fused single pass: edge forces and the gather
    /// `Tᵢ = fxᵢ − fxᵢ₋₁ + fyᵢ − fyᵢ₋w` (fx carried in a register, fy read back from
    /// `width` iterations ago); missing boundary edges contribute nothing (Neumann).
    fn compute_tension_force(&mut self) {
        let width = self.grid.width;
        let inv_h2 = self.inv_h2;
        let gamma_h2 = self.drive_normalization * LOCAL_STRAIN_RATIO * inv_h2;
        let k = 1.0 / self.config.sample_rate;
        let denom = 1.0 + self.config.sigma0 * k;
        let scale = k * k / denom * inv_h2 * self.coeff_lap_span;
        // The smoothed global drive estimates the modulation's DC component; subtracting
        // `(1 − dose)` of it removes that share of the slow stiffening (the audible
        // sweep) while local audio-rate deviations — the cascade — pass at full strength.
        // The subtraction is spatially uniform and envelope-slow: no audio-rate pump.
        let dc_offset = (1.0 - self.glide_dose) * self.mean_modulation;
        let floor = self.modulation_floor;
        let active = self.active_cells;
        let mut modulation_sum = 0.0_f32;
        let PlateKernel {
            u,
            tension_force,
            edge_force_x,
            edge_force_y,
            ..
        } = self;
        let u = &u[..active];
        let fx = &mut edge_force_x[..active];
        let fy = &mut edge_force_y[..active];
        let tension = &mut tension_force[..active];
        let height = self.grid.height;
        for y in 0..height {
            let row = y * width;
            let mut fx_prev = 0.0_f32;
            for x in 0..width {
                let i = row + x;
                let dx = if x + 1 < width { u[i + 1] - u[i] } else { 0.0 };
                let mx = (gamma_h2 * dx * dx).min(1.0);
                let ex = (mx - dc_offset).max(floor) * scale * dx;
                let dy = if y + 1 < height {
                    u[i + width] - u[i]
                } else {
                    0.0
                };
                let my = (gamma_h2 * dy * dy).min(1.0);
                let ey = (my - dc_offset).max(floor) * scale * dy;
                modulation_sum += mx + my;
                fx[i] = ex;
                fy[i] = ey;
                let from_above = if y > 0 { fy[i - width] } else { 0.0 };
                tension[i] = ex - fx_prev + ey - from_above;
                fx_prev = ex;
            }
        }
        self.mean_modulation = modulation_sum / (2.0 * active as f32);
    }

    fn derive_coefficients(&mut self) {
        let k = 1.0 / self.config.sample_rate;
        let h = self.grid.spacing_m;
        let denom = 1.0 + self.config.sigma0 * k;
        let kappa2 = self.config.kappa * self.config.kappa;
        let c2 = self.config.tension_speed * self.config.tension_speed;
        let c2_max = super::max_stable_tension_sq(
            h,
            self.config.kappa,
            self.config.sigma1,
            self.config.sample_rate,
        );
        // Edge-weight span: with the divergence-form (energy-conserving) tension the
        // modulation cannot pump, so the span needs only the hard stability clamp and the
        // designed headroom — no dissipation-scaled budget.
        let headroom_sq = self.config.tension_headroom_speed * self.config.tension_headroom_speed;
        self.coeff_lap = c2 * k * k / denom;
        self.coeff_lap_span = (c2_max - c2).max(0.0).min(headroom_sq) * self.bloom_depth;
        // DC subtraction may locally soften edges; keep total stiffness >= ~0.
        self.modulation_floor = if self.coeff_lap_span > f32::EPSILON {
            (-0.9 * c2 / self.coeff_lap_span).max(-1.0)
        } else {
            0.0
        };
        self.bend_attack = 1.0 - (-1.0 / (BLOOM_ATTACK_SECONDS * self.config.sample_rate)).exp();
        self.bend_release = 1.0 - (-1.0 / (BLOOM_RELEASE_SECONDS * self.config.sample_rate)).exp();
        self.inv_h2 = 1.0 / (h * h);
        // Velocity-form leapfrog: `u⁺ = u + g·(u − u⁻) + (spatial terms)`, with
        // `g = (1−σ₀k)/(1+σ₀k)`. Algebraically identical to the textbook
        // `(2u − (1−σ₀k)u⁻)/(1+σ₀k)` form, but the characteristic polynomial factors as
        // `(λ−1)(λ−g)` *structurally*: the free plate's rigid-body mode keeps its neutral
        // λ = 1 root exactly. In the two-coefficient form, f32 rounding of the
        // coefficients perturbs that root by up to ~ulp/(2σ₀k) — ±7e-4 at typical σ₀ —
        // randomly tipping the rigid mode into exponential growth depending on the exact
        // σ₀ bit pattern (the Neumann Laplacian is exactly zero on constants, so the
        // spatial terms contribute no rounding there).
        self.coeff_velocity = (1.0 - self.config.sigma0 * k) / denom;
        self.coeff_lap2 = -kappa2 * k * k / denom;
        self.coeff_s1 = 2.0 * self.config.sigma1 * k / denom;
        self.coeff_force = k * k / denom;
    }

    /// Pass 1: `lap = δΔ u` over the active grid. Simply supported pins `u = 0` and zero
    /// moment (`lap = 0`) at the edge; clamped pins `u = 0` with an even ghost
    /// (`u₋₁ = u₁`), reducing to `lap_edge = 2·u(inward)/h²` on the zero boundary ring;
    /// free edges use the ghost rings.
    fn compute_laplacian(&mut self) {
        let width = self.grid.width;
        let height = self.grid.height;
        let inv_h2 = self.inv_h2;
        let boundary = self.config.boundary;
        let active = self.active_cells;
        let PlateKernel { u, lap, .. } = self;
        let u = &u[..active];
        let lap = &mut lap[..active];
        interior_laplacian(lap, u, width, height, inv_h2);
        match boundary {
            PlateBoundary::SimplySupported => edge_fill_zero(lap, width, height),
            PlateBoundary::Clamped => clamped_edge_laplacian(lap, u, width, height, inv_h2),
            PlateBoundary::Free => neumann_edge_laplacian(lap, u, width, height, inv_h2),
        }
    }

    /// Pass 2 (fused): `u⁺` from the update equation, written into `u_prev` (swapped by the
    /// caller). Pinned boundaries (simply supported, clamped) move only the interior and
    /// re-zero the boundary ring of the new state — a re-tune from a free-edge config must
    /// not leave stale displacement frozen on the edge. Free boundaries update every cell,
    /// reading the ghost Laplacian rings at the edge.
    fn update(&mut self) {
        let width = self.grid.width;
        let height = self.grid.height;
        let inv_h2 = self.inv_h2;
        let boundary = self.config.boundary;
        // Bloom (M3): the plate's own bending energy modulates the tension term. The raw
        // energy is the dot of the state with its (already computed) Laplacian; the
        // follower shapes onset/release; the span clamp keeps c²_eff inside the grid's
        // closed-form stability budget by construction.
        let raw_energy = (-self.u[..self.active_cells]
            .iter()
            .zip(&self.lap[..self.active_cells])
            .map(|(u, lap)| u * lap)
            .sum::<f32>()
            / self.active_cells as f32)
            .max(0.0);
        // The modulation is LOCAL (per-cell strain |∇u|²ᵢ): the global Berger scalar
        // couples modes only through the spatial mean of the strain, and modal
        // orthogonality kills every cross term — global form = pitch glide but no
        // pair-cascade. Local strain restores the triad coupling (the actual gong/cymbal
        // wash mechanism; the full-vK Airy stress is its exact form). The mean-strain
        // follower below is reporting/calibration only.
        let drive_raw = (self.drive_normalization * raw_energy).clamp(0.0, 1.0);
        let coeff = if drive_raw > self.tension_drive {
            self.bend_attack
        } else {
            self.bend_release
        };
        self.tension_drive += coeff * (drive_raw - self.tension_drive);
        self.bend_energy_smoothed += coeff * (raw_energy - self.bend_energy_smoothed);
        let coeffs = UpdateCoefficients {
            velocity: self.coeff_velocity,
            lap2: self.coeff_lap2,
            lap: self.coeff_lap,
            s1: self.coeff_s1,
        };
        self.compute_tension_force();
        let active = self.active_cells;
        let PlateKernel {
            u,
            u_prev,
            lap,
            lap_prev,
            tension_force,
            ..
        } = self;
        let u = &u[..active];
        let u_prev = &mut u_prev[..active];
        let lap = &lap[..active];
        let lap_prev = &lap_prev[..active];
        let tension = &tension_force[..active];
        for y in 1..height - 1 {
            let row = y * width;
            for x in 1..width - 1 {
                let i = row + x;
                let lap2 = (lap[i - 1] + lap[i + 1] + lap[i - width] + lap[i + width]
                    - 4.0 * lap[i])
                    * inv_h2;
                u_prev[i] = coeffs.apply(u[i], u_prev[i], lap2, tension[i], lap[i], lap_prev[i]);
            }
        }
        match boundary {
            PlateBoundary::SimplySupported | PlateBoundary::Clamped => {
                edge_fill_zero(u_prev, width, height);
            }
            PlateBoundary::Free => {
                neumann_edge_update(
                    u_prev, u, lap, lap_prev, tension, width, height, inv_h2, coeffs,
                );
            }
        }
    }

    /// Gauge-fix the free plate's rigid null mode: subtract the spatial mean from both
    /// state buffers. A common shift is exactly invariant for the dynamics (constants are
    /// in the Laplacian's null space and the update reads `u − u⁻` pairs), but leaving the
    /// rigid displacement offset in the f32 state costs the oscillating field mantissa —
    /// damping increments below the offset's ulp quantize to zero and tiny modes sustain
    /// as a quantization limit cycle (~−66 dB after a hard strike).
    fn remove_rigid_offset(&mut self) {
        let active = self.active_cells;
        let mean = self.u_prev[..active].iter().sum::<f32>() / active as f32;
        if mean == 0.0 {
            return;
        }
        for value in &mut self.u_prev[..active] {
            *value -= mean;
        }
        for value in &mut self.u[..active] {
            *value -= mean;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct UpdateCoefficients {
    pub(super) velocity: f32,
    pub(super) lap2: f32,
    pub(super) lap: f32,
    pub(super) s1: f32,
}

impl UpdateCoefficients {
    /// `tension` is the pre-scaled edge-weighted tension force for this cell.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(
        &self,
        u: f32,
        u_prev: f32,
        lap2: f32,
        tension: f32,
        lap: f32,
        lap_prev: f32,
    ) -> f32 {
        u + self.velocity * (u - u_prev)
            + self.lap2 * lap2
            + self.lap * lap
            + tension
            + self.s1 * (lap - lap_prev)
    }
}

fn interior_laplacian(lap: &mut [f32], u: &[f32], width: usize, height: usize, inv_h2: f32) {
    for y in 1..height - 1 {
        let row = y * width;
        for x in 1..width - 1 {
            let i = row + x;
            lap[i] = (u[i - 1] + u[i + 1] + u[i - width] + u[i + width] - 4.0 * u[i]) * inv_h2;
        }
    }
}

fn edge_fill_zero(target: &mut [f32], width: usize, height: usize) {
    let last_row = (height - 1) * width;
    target[..width].fill(0.0);
    target[last_row..last_row + width].fill(0.0);
    for y in 1..height - 1 {
        let row = y * width;
        target[row] = 0.0;
        target[row + width - 1] = 0.0;
    }
}

/// Clamped boundary ring: `lap = 2·u(inward)/h²` (even ghost over a zero boundary ring);
/// corner mirrors land entirely on the zero ring, so corners are 0.
fn clamped_edge_laplacian(lap: &mut [f32], u: &[f32], width: usize, height: usize, inv_h2: f32) {
    let last_row = (height - 1) * width;
    for x in 1..width - 1 {
        lap[x] = 2.0 * u[x + width] * inv_h2;
        lap[last_row + x] = 2.0 * u[last_row + x - width] * inv_h2;
    }
    for y in 1..height - 1 {
        let row = y * width;
        lap[row] = 2.0 * u[row + 1] * inv_h2;
        lap[row + width - 1] = 2.0 * u[row + width - 2] * inv_h2;
    }
    lap[0] = 0.0;
    lap[width - 1] = 0.0;
    lap[last_row] = 0.0;
    lap[last_row + width - 1] = 0.0;
}
