# 0050 — Lamath Cymbal resonator is a stiff-plate FDTD, not a membrane waveguide mesh

- Status: Accepted
- Date: 2026-06-10

## Context

The cymbal voice review traced the product's two standing sound defects — a tinny body and
loud, seconds-long ringing in the 12–24 kHz band — to the model class, not to tuning. The
rectilinear waveguide mesh in `lindelion-idiophone` is exactly equivalent (at junction
pressures) to an FDTD scheme for the 2D scalar wave equation: a tension-restored ideal
membrane. A cymbal is a stiff plate: its restoring force is bending (`−D∇⁴u`), giving
`f ∝ k²` dispersion, widely spaced strong low modes, and constant modal density. The membrane
inverts that layout (sparse low modes from ~480 Hz up, density growing with frequency), and
the mismatch is compounded by structural artifacts:

- The mesh dispersion relation has two zero-group-velocity accumulation points (axial band
  edge at fs/4, checkerboard corner at fs/2). Modes pile up there and cannot propagate — and
  boundary reflection is the model's **only** loss mechanism, so effective T60 diverges in
  exactly that band.
- The geometric (von Kármán) coupling rotated junction energy directly into the checkerboard
  mode (rings at fs/2), and its drive was gated on radiated-output RMS with a reference
  ~50 dB below audible ring levels — pinned at maximum depth for the entire audible tail,
  with a positive feedback through the HF-tilted output taps.
- The output stage compensated for the missing plate spectrum with high-spatial-frequency
  gain (shimmer ×12·density^1.5, curvature ×4 within it) over a unity body aperture — the
  tinny balance.
- Two of four striker wavelets (including the default) alternated sign every sample: a
  near-Nyquist burst, not a contact-force pulse.

Real-time linear Kirchhoff plate FDTD with frequency-dependent loss is established practice
(Bilbao, *Numerical Sound Synthesis*; Wang/Bilbao/Erbe/Puckette, SMC 2023, real-time on CPU
with AVX). At bronze stiffness (κ ≈ 1.1 m²/s, 1 mm) and 48 kHz, the stability-limited grid
for a 16" plate is ~43×43 cells — at or below the membrane Crash's 2623 active cells, with
half the state arrays.

## Decision

- **The resonator core is an explicit FDTD scheme for the Kirchhoff plate with a tension
  term**: `ü = −κ²∇⁴u + c²∇²u − losses`, on a rectangular grid. The tension blend spans a
  physical membrane↔plate morph, so one model covers the product's drum-to-cymbal range.
- **Loss is distributed and frequency-dependent** (σ₀ + σ₁∇² interior terms), calibrated to
  T60 targets at two reference frequencies. High modes die first by physics; boundary-only
  loss and its trapped-mode failure are gone.
- **Boundaries are pinned/free per the material control.** Free edges are the *natural*
  boundary condition of the discrete plate energy: every spatial operator (bending `L²`,
  tension `L`, σ₁ loss on velocity) is built from one symmetric negative-semidefinite
  Neumann-truncated graph Laplacian, so energy decay is provable for every grid and
  parameter set. The centered-ghost discretization of the classical ν-coupled moment/shear
  conditions was built first and rejected by the stability tests: it satisfies the BC
  equations but is not energy-stable (M0 found a defective marginal boundary mode). The
  trade: the free-edge moment condition loses its ν-coupling, shifting free-mode
  frequencies slightly — voiced by audition in later milestones.
- **The scheme update is the velocity-form leapfrog** (`u⁺ = u + g·(u−u⁻) + spatial`,
  `g = (1−σ₀k)/(1+σ₀k)`): algebraically identical to the textbook two-coefficient form,
  but the characteristic polynomial factors as `(λ−1)(λ−g)` structurally, keeping the free
  plate's rigid-body root exactly neutral. In the textbook form, f32 rounding of the
  coefficients perturbs that root by up to ~ulp/(2σ₀k) — randomly tipping the rigid mode
  into slow exponential growth depending on the σ₀ bit pattern. The rigid offset itself is
  gauge-fixed away each step (spatial-mean subtraction, free plates only): a common shift
  is exactly invariant for the dynamics, and carrying the offset in f32 otherwise floors
  the oscillating field's precision (quantization limit cycles ~−66 dB).
- **Geometry is rectangular.** It preserves the allocation-free SoA/active-region kernel
  architecture and SIMD-friendly stencils; a circular mask and shell curvature are backlog
  follow-ons, not part of this replacement.
- **Physical parameterization**: grid spacing derived from the stability condition
  (`h ≥ 2√(κ·Δt)`), plate dimensions/stiffness from the size/material controls. The
  `MeshResonator` public API and seven-control voice-parameter shape are preserved.
- **The nonlinear bloom is a per-edge tension-modulation cascade** (as built in M3/M4):
  each grid edge stiffens with its own strain — the divergence-form (energy-conserving)
  discretization of local Berger-type tension modulation, an edge-weighted graph
  Laplacian clamped inside the grid's closed-form stability budget. Two construction
  facts proved load-bearing: the *global* Berger scalar cannot cascade (modal
  orthogonality kills every cross term — it only glides), and the *coefficient-form*
  local modulation (`c²(x,t)·∇²u`) parametrically pumps (it is not a potential gradient);
  only the divergence/edge form is both cascading and passive. The slow (DC) component —
  the gong pitch glide — is dosed per voicing (`∝ 1/κ²`) so stiff plates hold pitch. The
  drive is the plate's own mean bending strain, never output level.
- **Strikers are contact-force pulses** whose duration carries hardness; no sign-alternating
  wavelets.
- **The radiated output is a derived radiation model** (plate radiation efficiency rises with
  frequency below coincidence), replacing the aperture/shimmer/curvature gain set.
- **The membrane mesh is fully replaced.** No dual-model selector; the membrane remains in
  git history.

## Alternatives

- **Keep the membrane and retune taps/losses.** Cannot fix mode placement or the
  zero-group-velocity traps; every fix would remain compensation for the wrong PDE.
- **Allpass-dispersive waveguide mesh.** Approximates stiffness dispersion crudely over the
  band, roughly doubles per-cell state and cost, and keeps the directional-wave architecture
  that the artifacts live in.
- **Full von Kármán nonlinearity.** Physically exact cascade, but requires an Airy-stress
  solve per sample (FD form) or a precomputed O(N³) modal coupling tensor (Ducceschi & Touzé,
  DAFx-15) — offline-grade cost in the published cymbal results. Documented as the upgrade
  path if the phenomenological cascade fails audition.
- **Circular mask from day one.** Truer cymbal mode structure, but free-edge biharmonic
  conditions on a staircase boundary are the known-hard discretization and would add
  per-cell masking to every kernel in the first milestone.

## Consequences

- Partial layout, modal density, and transient dispersion (highs arrive first) become
  plate-correct; the tinny-by-construction balance and the undamped near-Nyquist ring are
  removed at the root.
- All nine shipped presets must be re-voiced by audition; drum-like territory moves from
  "small membrane grid" to "high tension blend".
- The discrete free-edge corner conditions are the riskiest scheme detail and are
  re-derived from the literature during phase planning, not from recall.
- CPU stays within the existing budget (fewer cells, half the state, comparable stencil
  cost), re-verified against the `mesh_crash_perf` harness.

## References

- S. Bilbao, *Numerical Sound Synthesis: Finite Difference Schemes and Simulation in Musical
  Acoustics*, Wiley, 2009 (plate schemes, energy-method stability, free-edge conditions).
- Z. Wang, S. Bilbao, T. Erbe, M. Puckette, "Real-time implementation of the Kirchhoff plate
  equation using finite-difference time-domain methods on CPU", SMC 2023.
- M. Ducceschi, C. Touzé, "Simulations of nonlinear plate dynamics: an accurate and efficient
  modal algorithm", DAFx-15; and "Modal approach for nonlinear vibrations of damped impacted
  plates: application to sound synthesis of gongs and cymbals", J. Sound & Vibration, 2015.
