//! Neumann (free-boundary) stencil passes for the plate kernel: the edge ring of the
//! graph Laplacian and the free-boundary update (see `kernel.rs` for the scheme and
//! ADR-0050 for the boundary-condition decision).

use super::kernel::UpdateCoefficients;

/// Free boundary ring of the Neumann (natural / graph) Laplacian: each boundary cell sums
/// only its existing neighbors. This is the free edge as the *natural* boundary condition
/// of the discrete plate energy `½κ²Σ(Lu)² + ½c²Σ|∇u|²`: every spatial operator (bending
/// `L²`, tension `L`, σ₁ loss `L` on velocity) is built from the one symmetric
/// negative-semidefinite `L`, so energy decay is provable for every grid and parameter set.
/// The centered-ghost discretization of the classical moment/shear conditions satisfied the
/// BC equations but was not energy-stable — a defective marginal boundary mode left
/// tensioned free voicings on a pseudospectral knife edge (divergence flipped by 1e-7
/// coefficient perturbations). The trade: the edge moment condition loses its ν-coupling
/// (free-mode frequencies shift slightly), which later milestones voice by audition.
pub(super) fn neumann_edge_laplacian(
    lap: &mut [f32],
    u: &[f32],
    width: usize,
    height: usize,
    inv_h2: f32,
) {
    let last_row = (height - 1) * width;
    for x in 1..width - 1 {
        lap[x] = (u[x - 1] + u[x + 1] + u[x + width] - 3.0 * u[x]) * inv_h2;
        let i = last_row + x;
        lap[i] = (u[i - 1] + u[i + 1] + u[i - width] - 3.0 * u[i]) * inv_h2;
    }
    for y in 1..height - 1 {
        let i = y * width;
        lap[i] = (u[i + 1] + u[i - width] + u[i + width] - 3.0 * u[i]) * inv_h2;
        let i = y * width + width - 1;
        lap[i] = (u[i - 1] + u[i - width] + u[i + width] - 3.0 * u[i]) * inv_h2;
    }
    lap[0] = (u[1] + u[width] - 2.0 * u[0]) * inv_h2;
    let i = width - 1;
    lap[i] = (u[i - 1] + u[i + width] - 2.0 * u[i]) * inv_h2;
    let i = last_row;
    lap[i] = (u[i + 1] + u[i - width] - 2.0 * u[i]) * inv_h2;
    let i = last_row + width - 1;
    lap[i] = (u[i - 1] + u[i - width] - 2.0 * u[i]) * inv_h2;
}

/// Free-boundary update of the edge ring: `lap2 = L(lap)` with the same Neumann rule;
/// the tension force is already edge-assembled (Neumann-natural at the free boundary).
#[allow(clippy::too_many_arguments)]
pub(super) fn neumann_edge_update(
    u_next: &mut [f32],
    u: &[f32],
    lap: &[f32],
    lap_prev: &[f32],
    tension: &[f32],
    width: usize,
    height: usize,
    inv_h2: f32,
    coeffs: UpdateCoefficients,
) {
    let last_row = (height - 1) * width;
    let mut apply = |i: usize, lap2: f32| {
        u_next[i] = coeffs.apply(u[i], u_next[i], lap2, tension[i], lap[i], lap_prev[i]);
    };
    for x in 1..width - 1 {
        let lap2 = (lap[x - 1] + lap[x + 1] + lap[x + width] - 3.0 * lap[x]) * inv_h2;
        apply(x, lap2);
        let i = last_row + x;
        let lap2 = (lap[i - 1] + lap[i + 1] + lap[i - width] - 3.0 * lap[i]) * inv_h2;
        apply(i, lap2);
    }
    for y in 1..height - 1 {
        let i = y * width;
        let lap2 = (lap[i + 1] + lap[i - width] + lap[i + width] - 3.0 * lap[i]) * inv_h2;
        apply(i, lap2);
        let i = y * width + width - 1;
        let lap2 = (lap[i - 1] + lap[i - width] + lap[i + width] - 3.0 * lap[i]) * inv_h2;
        apply(i, lap2);
    }
    let lap2 = (lap[1] + lap[width] - 2.0 * lap[0]) * inv_h2;
    apply(0, lap2);
    let i = width - 1;
    let lap2 = (lap[i - 1] + lap[i + width] - 2.0 * lap[i]) * inv_h2;
    apply(i, lap2);
    let i = last_row;
    let lap2 = (lap[i + 1] + lap[i - width] - 2.0 * lap[i]) * inv_h2;
    apply(i, lap2);
    let i = last_row + width - 1;
    let lap2 = (lap[i - 1] + lap[i - width] - 2.0 * lap[i]) * inv_h2;
    apply(i, lap2);
}
