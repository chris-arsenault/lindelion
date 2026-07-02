//! Stiff-plate (Kirchhoff + tension) physical configuration and closed-form scheme math:
//! stability-derived grid sizing, dispersion, and two-target loss calibration.
//!
//! Scheme (ADR-0050; Bilbao, *Numerical Sound Synthesis* §12):
//!
//! `ü = −κ²∇⁴u + c²∇²u − 2σ₀·u̇ + 2σ₁·∇²u̇ + f`
//!
//! - Stability: with `x = 1/h²`, the explicit leapfrog scheme requires
//!   `16κ²k²·x² + (2c²k² + 8σ₁k)·x ≤ 1` (worst Laplacian eigenvalue `8/h²` at the spatial
//!   Nyquist corner), giving the minimum grid spacing `h` as the positive quadratic root.
//!   Pure-plate limit: `h ≥ 2√(κk)`.
//! - Dispersion: a mode with Laplacian eigenvalue `−ξ` rings at `ω² = κ²ξ² + c²ξ`; the
//!   inverse is `ξ(ω) = (−c² + √(c⁴ + 4κ²ω²)) / (2κ²)`.
//! - Loss: per-mode decay rate `σ(ω) = σ₀ + σ₁·ξ(ω)`, `T60 = ln(1000)/σ`. Two `(f, T60)`
//!   targets solve linearly for `(σ₀, σ₁)`.

use lindelion_dsp_utils::math::finite_clamp;

use crate::sanitize_sample_rate;

mod kernel;
mod neumann;
pub(crate) mod spatial;

pub use kernel::PlateKernel;
pub use spatial::MeshPoint;

#[cfg(test)]
#[path = "plate/tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "plate/spectral_tests.rs"]
mod spectral_tests;

/// Grid budget for the plate, matching the membrane's allocation budget: buffers are
/// allocated once at this size and the active grid is a sub-region prefix.
pub(crate) const PLATE_MAX_WIDTH: usize = 64;
pub(crate) const PLATE_MAX_HEIGHT: usize = 48;
pub(crate) const PLATE_MAX_CELLS: usize = PLATE_MAX_WIDTH * PLATE_MAX_HEIGHT;
/// Smallest active dimension: keeps a usable interior for the biharmonic stencil.
pub(crate) const PLATE_MIN_DIM: usize = 4;

/// `ln(1000)`: amplitude decay exponent corresponding to −60 dB.
pub(crate) const LN_1000: f32 = 6.907_755;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlateBoundary {
    /// `u = 0`, zero moment (odd ghost). Trivially energy-stable.
    SimplySupported,
    /// `u = 0`, zero slope (even ghost).
    Clamped,
    /// Free edge as the natural boundary condition of the discrete plate energy
    /// (Neumann-truncated graph Laplacian; see `kernel::neumann_edge_laplacian`).
    Free,
}

/// Physical description of the plate. All fields are continuous physical quantities;
/// the discrete grid is derived (`PlateGrid::for_config`), never specified directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateConfig {
    /// Plate side lengths in meters.
    pub length_x_m: f32,
    pub length_y_m: f32,
    /// Stiffness parameter `κ = √(D/ρh)` in m²/s (bronze ≈ 1.07 at 1 mm).
    pub kappa: f32,
    /// Tension/membrane wave speed `c` in m/s; 0 = pure plate.
    pub tension_speed: f32,
    /// Reserved tension-modulation headroom (m/s): the grid is sized for
    /// `c²_design = c²_voice + headroom²`, so the bloom's modulation budget is uniform
    /// instead of depending on which grids happen to be budget-coarsened (M3, ADR-0050).
    pub tension_headroom_speed: f32,
    /// Frequency-independent loss `σ₀` (1/s).
    pub sigma0: f32,
    /// Frequency-dependent loss `σ₁` (m²/s).
    pub sigma1: f32,
    pub sample_rate: f32,
    pub boundary: PlateBoundary,
}

impl Default for PlateConfig {
    fn default() -> Self {
        Self {
            length_x_m: 0.406,
            length_y_m: 0.406,
            kappa: 1.07,
            tension_speed: 0.0,
            tension_headroom_speed: 0.0,
            sigma0: 1.0,
            sigma1: 0.005,
            sample_rate: 48_000.0,
            boundary: PlateBoundary::Free,
        }
    }
}

impl PlateConfig {
    pub fn sanitized(self) -> Self {
        Self {
            length_x_m: finite_clamp(self.length_x_m, 0.05, 2.0, 0.406),
            length_y_m: finite_clamp(self.length_y_m, 0.05, 2.0, 0.406),
            kappa: finite_clamp(self.kappa, 0.05, 20.0, 1.07),
            tension_speed: finite_clamp(self.tension_speed, 0.0, 400.0, 0.0),
            tension_headroom_speed: finite_clamp(self.tension_headroom_speed, 0.0, 400.0, 0.0),
            sigma0: finite_clamp(self.sigma0, 0.0, 1_000.0, 1.0),
            sigma1: finite_clamp(self.sigma1, 0.0, 100.0, 0.005),
            sample_rate: sanitize_sample_rate(self.sample_rate),
            boundary: self.boundary,
        }
    }
}

/// The derived discrete grid: active dimensions and the (square) cell spacing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateGrid {
    pub width: usize,
    pub height: usize,
    /// Cell spacing `h` in meters. Always ≥ the stability minimum.
    pub spacing_m: f32,
}

impl PlateGrid {
    /// Derive the grid from the stability condition: `h` is the stability minimum unless
    /// the cell budget forces it coarser; dimensions quantize the requested lengths and
    /// clamp to the allocation budget. The chosen `h` is kept (the physical size quantizes),
    /// so the scheme is stable by construction for every sanitized config.
    pub fn for_config(config: &PlateConfig) -> Self {
        let config = config.sanitized();
        let design_speed = (config.tension_speed * config.tension_speed
            + config.tension_headroom_speed * config.tension_headroom_speed)
            .sqrt();
        let h_min = min_stable_spacing(
            config.kappa,
            design_speed,
            config.sigma1,
            config.sample_rate,
        );
        let h = h_min
            .max(config.length_x_m / PLATE_MAX_WIDTH as f32)
            .max(config.length_y_m / PLATE_MAX_HEIGHT as f32);
        let dim = |length: f32, max: usize| -> usize {
            (((length / h).round() as isize).max(PLATE_MIN_DIM as isize) as usize).min(max)
        };
        Self {
            width: dim(config.length_x_m, PLATE_MAX_WIDTH),
            height: dim(config.length_y_m, PLATE_MAX_HEIGHT),
            spacing_m: h,
        }
    }

    pub fn cells(&self) -> usize {
        self.width * self.height
    }
}

/// Safety factor applied above the exact stability-limit spacing. At the exact limit the
/// leapfrog amplification polynomial has a repeated root at the spatial-Nyquist mode —
/// neutrally stable in exact arithmetic but linear-in-n drift under f32 rounding — so the
/// grid is sized a hair coarser than the limit.
pub(crate) const STABILITY_MARGIN: f32 = 1.002;

/// Minimum stable grid spacing for the explicit scheme: the positive root of
/// `16κ²k²·x² + (2c²k² + 8σ₁k)·x − 1 = 0` in `x = 1/h²`, widened by
/// [`STABILITY_MARGIN`].
pub(crate) fn min_stable_spacing(kappa: f32, tension_speed: f32, sigma1: f32, fs: f32) -> f32 {
    let k = 1.0 / sanitize_sample_rate(fs);
    let a = 16.0 * kappa * kappa * k * k;
    let b = 2.0 * tension_speed * tension_speed * k * k + 8.0 * sigma1 * k;
    let x = if kappa <= f32::EPSILON {
        // Membrane/tension-only limit: x ≤ 1/b.
        1.0 / b.max(f32::EPSILON)
    } else {
        (-b + (b * b + 4.0 * a).sqrt()) / (2.0 * a)
    };
    (1.0 / x.max(f32::EPSILON)).sqrt() * STABILITY_MARGIN
}

/// Closed-form inverse of the stability quadratic at a **fixed** grid spacing: the
/// largest tension `c²` the explicit scheme admits on that grid,
/// `c²_max = (1 − 16κ²k²x² − 8σ₁kx) / (2k²x)` with `x = 1/h²`, clamped at 0. The
/// tension-modulation clamp (M3) is this bound at the kernel's actual spacing.
pub(crate) fn max_stable_tension_sq(h: f32, kappa: f32, sigma1: f32, fs: f32) -> f32 {
    let k = 1.0 / sanitize_sample_rate(fs);
    let x = 1.0 / (h * h).max(f32::EPSILON);
    let remainder = 1.0 - 16.0 * kappa * kappa * k * k * x * x - 8.0 * sigma1 * k * x;
    (remainder / (2.0 * k * k * x)).max(0.0)
}

/// Laplacian eigenvalue magnitude `ξ` of the continuous mode ringing at `ω` (rad/s):
/// the inverse of `ω² = κ²ξ² + c²ξ`.
pub(crate) fn xi_for_omega(omega: f32, kappa: f32, tension_speed: f32) -> f32 {
    let c2 = tension_speed * tension_speed;
    let k2 = kappa * kappa;
    if k2 <= f32::EPSILON {
        // Pure membrane: ω = c√ξ.
        return omega * omega / c2.max(f32::EPSILON);
    }
    ((c2 * c2 + 4.0 * k2 * omega * omega).sqrt() - c2) / (2.0 * k2)
}

/// Continuous simply-supported mode frequency (Hz) for mode `(m, n)` on the quantized
/// grid: the analytic anchor for spectral tests. The vibrating lengths are
/// `(dims − 1)·h` (displacement is pinned at the boundary cells).
pub fn mode_frequency_hz(m: usize, n: usize, config: &PlateConfig, grid: &PlateGrid) -> f32 {
    let config = config.sanitized();
    let lx = (grid.width.max(2) - 1) as f32 * grid.spacing_m;
    let ly = (grid.height.max(2) - 1) as f32 * grid.spacing_m;
    let gx = m as f32 * std::f32::consts::PI / lx;
    let gy = n as f32 * std::f32::consts::PI / ly;
    let xi = gx * gx + gy * gy;
    let omega2 =
        config.kappa * config.kappa * xi * xi + config.tension_speed * config.tension_speed * xi;
    omega2.max(0.0).sqrt() / std::f32::consts::TAU
}

/// A decay target: the mode band centred at `frequency_hz` should ring for `t60_s`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayTarget {
    pub frequency_hz: f32,
    pub t60_s: f32,
}

/// Closed-form `(σ₀, σ₁)` from two decay targets: solve
/// `σ₀ + σ₁·ξ(ω_i) = ln(1000)/T60_i` for both targets. `σ₁` clamps to ≥ 0 (a
/// high-frequency target that out-rings the low one is unphysical for this loss model;
/// the low target then governs alone), and `σ₀` clamps to ≥ 0.
pub fn losses_for(
    low: DecayTarget,
    high: DecayTarget,
    kappa: f32,
    tension_speed: f32,
) -> (f32, f32) {
    let xi_low = xi_for_omega(
        std::f32::consts::TAU * low.frequency_hz.max(1.0),
        kappa,
        tension_speed,
    );
    let xi_high = xi_for_omega(
        std::f32::consts::TAU * high.frequency_hz.max(1.0),
        kappa,
        tension_speed,
    );
    let s_low = LN_1000 / low.t60_s.max(1.0e-3);
    let s_high = LN_1000 / high.t60_s.max(1.0e-3);
    let denom = xi_high - xi_low;
    let sigma1 = if denom.abs() <= f32::EPSILON {
        0.0
    } else {
        ((s_high - s_low) / denom).max(0.0)
    };
    let sigma0 = (s_low - sigma1 * xi_low).max(0.0);
    (sigma0, sigma1)
}

/// Per-mode decay rate `σ(ω)` for calibrated losses (test/verification helper).
#[cfg(test)]
pub(crate) fn decay_rate_for_omega(
    omega: f32,
    sigma0: f32,
    sigma1: f32,
    kappa: f32,
    tension_speed: f32,
) -> f32 {
    sigma0 + sigma1 * xi_for_omega(omega, kappa, tension_speed)
}
