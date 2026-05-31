# Phase M6 — Mesh von Kármán geometric nonlinearity (execution steps)

Expanded from milestone **M6** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M2, M3], both landed; sibling of M4/M5). The third nonlinear stage: make the linear 2D
mesh amplitude-coupled so a hard strike produces the gong/plate **bloom and shimmer** — energy
spreads upward into higher modes as drive rises. Scope is `mesh_2d` only; String (M4), Tube (M5), and
`ModalBank` are untouched.

## Reference behavior (re-derive before editing)

**Von Kármán geometric (large-deflection) plate nonlinearity (Bilbao *Numerical Sound Synthesis*;
Ducceschi; Woodhouse — plan §Reference research).** At small amplitude a plate/membrane is linear:
fixed modes, no coupling. At large amplitude the transverse displacement *stretches* the plate — the
in-plane tension rises with the spatially-averaged squared slope `∫(∇w)²` — which (a) raises the
effective wave speed, so **mode frequencies glide up** with amplitude, and (b) introduces a
conservative coupling (the von Kármán bracket `[w, F]`) that **transfers energy between modes**, i.e.
spreads energy upward into higher modes. That upward spread is the audible gong "bloom"/"shimmer"
and the crash bloom of a struck cymbal. The defining numerical requirement is a **stable,
energy-conserving scheme** — the nonlinearity must redistribute energy, never inject it (Bilbao's
energy-conserving finite-difference von Kármán is the canonical reference).

**Current mesh (post-M3).** `mesh_2d.rs` is a **linear, lossless digital-waveguide mesh**: a fixed
14×10 grid of `DirectionalWaves` (`from_left/right/top/bottom`), scattered at each junction by
`pressure = 0.5·Σ inᵢ`, `outᵢ = pressure − inᵢ` (the standard energy-preserving DWM scattering),
then propagated to neighbours with boundary reflection/damping. The grid is fixed at construction
(allocation-free re-tuning); `wave_speed_mps` sets the mode lattice and is **constant per note**.
There is a passivity test (`lossless_boundary_scattering_is_passive_without_new_excitation`) asserting
the linear scheme never gains energy — the M6 nonlinearity must keep that property.

**Where it plugs in.** The mesh runs at 2× inside `ResonatorEngine` (built at `2·sample_rate`, M3),
so the nonlinearity runs against the oversampled clock — exactly the substrate that keeps the new
upper-mode energy alias-controlled (ADR-0016). The geometric coupling is a **per-junction (or
per-step) amplitude-dependent term in the scatter/propagate update**, driven by measured energy,
applied on top of the linear scattering.

**Energy plumbing (extend M4/M5).** M4/M5 thread `energy` from the voice to
`ResonatorEngine::process_sample(input, energy)`; the Waveguide arm forwards it via
`set_energy_drive`. The **Mesh arm currently ignores `energy`** (`mesh.process_sample(sample)`). M6
wires it: `ResonatorEngine`'s Mesh arm calls `self.mesh.set_geometric_drive(energy)` before the
oversampled loop; `MeshResonator::set_geometric_drive` (runtime.rs) forwards to `RectangularMesh2d`,
which stores it (default `0.0` → inert, so all existing mesh tests stay bit-identical) and uses it in
the junction update.

## Difficulty note (read before step 2)

Like M5, the *named* mechanism may not land measurably on the first try, and the energy-conserving 2D
nonlinearity is genuinely the hardest of the three stages. The recommended scheme below is a starting
point, not a guarantee. If it cannot produce a stable, measurable upward energy spread, **surface the
finding and the options to the user** (as in M5) rather than silently substituting a weaker or
energy-injecting mechanism — the energy-conserving requirement is a product constraint (ADR-0014,
this plan's exit), not optional.

---

## Steps

### 1. **[DECISION]** Mesh nonlinearity depth and the quality gate — **RESOLVED**
- *(This is the milestone's `[DECISION]`: "Mesh nonlinearity depth and whether a per-voice quality
  control gates it under the low-poly budget.")* **RESOLVED: always-on (no gate), pronounced bloom.**
  Internal constants in `mesh_2d.rs` (no new user parameter in M6 — fixed internal wiring per M2; a
  patch control is later scope):
  - **Quality gate:** none — the geometric nonlinearity and the mesh's 2× oversampling run
    unconditionally on every mesh voice ([ADR-0015](docs/adr/0015-expressive-low-polyphony-budget.md);
    the 1–4 voice budget affords it). No second code path / quality switch in M6.
  - **Depth + voicing:** a **pronounced** bloom — a hard strike clearly spreads energy upward.
    Squared, normalized energy curve `drive_term = clamp((energy/E_REF)², 0, max)` (`E_REF ≈ 0.15`,
    matching M4/M5) so soft strikes stay clean; the coupling strength is tuned in execution to a
    clear, monotone upward spread that **never injects energy** (the passivity/stability invariant
    is the hard ceiling on how far "pronounced" can go).

### 2. Mesh geometric nonlinearity + energy drive plumbing  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/mesh_2d.rs` (the geometric coupling +
  `geometric_drive` state on `RectangularMesh2d`), `plugins/lamath/src/dsp/waveguide/mesh_2d/runtime.rs`
  (`MeshResonator::set_geometric_drive` forwarding to the grid),
  `plugins/lamath/src/dsp/voice/resonator_stack.rs` (the `ResonatorEngine` Mesh arm sets the drive
  before the oversampled loop). Incidental: `MeshResonator` re-exports / `reset` clearing the drive.
- **Reference behavior:** as above — the von Kármán stretching raises an effective tension with the
  mesh's vibration energy, coupling modes and spreading energy upward, via a **stable,
  energy-conserving** amplitude-dependent term in the scatter/propagate update. Recommended starting
  scheme: a measured-energy-driven coupling that steers energy toward higher spatial frequencies
  while preserving the per-step energy (e.g. an amplitude-dependent, energy-preserving perturbation
  of the junction scattering, or an amplitude-dependent dispersion on the propagated waves — the
  2D analogue of M4's tension delay / M5's dispersion steepening). Keep it bounded and never
  energy-injecting (the passivity invariant). `geometric_drive` defaults to `0.0` (inert) and is
  reset in `RectangularMesh2d::reset`.
- **Change:** Add `geometric_drive: f32` to `RectangularMesh2d` (default `0.0`) and a setter;
  apply the energy-driven geometric coupling inside `scatter_junction`/`scatter_and_propagate`. Add
  `MeshResonator::set_geometric_drive` forwarding to the grid. In `ResonatorEngine::process_sample`'s
  Mesh arm, call `self.mesh.set_geometric_drive(energy)` before `self.oversampler.process(...)`.
- **Verify:** New `#[cfg(test)]` test in `mesh_2d.rs` driving `set_geometric_drive` directly: **upward
  energy spread with drive** — a struck mesh rendered at higher energy has measurably more
  high-frequency / higher-mode energy (rising spectral centroid or `sampled_high_frequency_ratio`)
  than at lower/zero energy, with the effect monotone in drive. Red→green: `set_geometric_drive` and
  the coupling do not exist yet (compile-level red), and at zero drive the render is bit-identical to
  pre-M6 (existing mesh tests, incl. the passivity test, stay green — the inert-at-zero guarantee).

### 3. Stability, energy-conservation, fixed-memory and no-allocation guards  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/waveguide/mesh_2d.rs` (tests),
  `plugins/lamath/src/dsp/voice/resonator_stack/tests.rs` (no-alloc test for a Mesh patch).
- **Reference behavior:** M6 exit — "stable at parameter extremes; fixed memory; no-alloc." The
  scheme must not inject energy (extend the passivity intuition to the driven case) and must stay
  bounded under any drive; the grid is already fixed-size so the nonlinearity must add no allocation.
  ADR-0001 requires no-alloc coverage on the new audio-thread work.
- **Change:** Tests only (no production change unless the sweep surfaces a real
  stability/energy-injection defect, in which case tighten the bound/clamp — not the semantics).
- **Verify:** New `#[cfg(test)]` tests: (a) drive the mesh with an extreme/noisy/non-finite geometric
  drive across several configs (sizes, boundaries, frequencies) and assert the output stays finite
  and bounded, and the mesh energy does not blow up (stays bounded across a long run — the
  energy-conserving/stability contract under drive); (b) `assert_no_allocations` over an engine render
  of a **Mesh** waveguide patch that sets a non-zero geometric drive each sample. Red→green: the
  bounded/energy-stable-under-drive contract is new.

### 4. Exit gate
- **File(s):** verification only; doc-surface update per the plan's cross-cutting constraint.
- **Reference behavior:** M6 exit — "`make ci` green; objective amplitude-dependent mode coupling /
  upward energy spread vs drive; stable at parameter extremes; fixed memory; no-alloc; `make bench`
  within the per-voice budget." Plan cross-cutting + [ADR-0015](docs/adr/0015-expressive-low-polyphony-budget.md):
  per-voice CPU tracked by `make bench` at the 1–4 voice target.
- **Change:** Add one curated `CHANGELOG.md` line (a hard strike now blooms — the mesh's energy
  spreads upward into higher modes with playing energy, gong/cymbal-style; soft strikes and a mesh at
  rest are unchanged; String/Tube/Modal unaffected). Run `make bench` to confirm the per-voice cost
  stays within the low-poly budget; record the figure (the mesh is the most expensive resonator, so
  watch this).
- **Verify:** Run `make ci` (green) and show output. The step-2 upward-spread test and step-3 guards
  satisfy the behavioral exit clauses; `make bench` confirms the budget.

---

## Decision resolved

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 1 | Mesh nonlinearity depth/voicing and whether a per-voice quality control gates it. | **RESOLVED: always-on (no gate), pronounced bloom.** Squared/normalized energy curve (`E_REF ≈ 0.15`), coupling tuned in execution to a clear upward spread bounded by the energy-conserving/passivity invariant. Internal constants in `mesh_2d.rs`; not a user parameter in M6. |
