# ADR-0014: Whole-System Dynamic Response via an Effort/Energy Bus and Prepared Operators

## Status

Accepted

## Date

2026-05-30

## Context

Lamath's resonators respond to playing dynamics mostly as level. Velocity scales excitation
amplitude (`velocity_to_gain` in `dsp/voice/mod.rs`), and the resonator and body characters are
otherwise static within a note. Real instruments change *timbre* with dynamics across the whole
instrument — the driver, the resonator/transport, the body, and the couplings between them — and
that whole-system, amplitude-dependent behavior is precisely what most physical-modeling synths
flatten. A hard pluck *blooms*; a loud bore turns *brassy*; a large body adds warmth at low
dynamics because the string does not overpower it. None of that is reachable by scaling one
subsystem with velocity.

Separately, the realtime path recomputes heavy linear operator derivations every sample. In
`waveguide/string_1d.rs` and `tube_1d.rs`, `process_sample` calls `core::loop_damping` (which
runs a 96-point filter-peak scan and a phase-delay solve), `dispersion::dispersion_profile`,
`core::waveguide_geometry`, and `core::delay_tuning` per sample; `waveguide/body.rs` rebuilds four
biquads per sample. These derivations are constant over a note and waste the CPU a nonlinear core
needs.

## Decision

Model the instrument as a chain — **driver → coupling → resonator/transport → two-way body →
surrounding** — and drive every stage and every coupling from a shared **effort/energy bus**.

The bus is **hybrid**: player effort (velocity, pressure, breath, from `ExpressionStream`) drives
the driver stage, and a per-sample **measured resonator-energy follower** drives resonator
nonlinearity, the couplings, and the body balance. Dynamics are an emergent property of the whole
chain reacting to one correlated effort/energy signal, not a parameter of any single block.

Hoist the per-sample linear operator derivations into a control-rate `PreparedResonatorModel`,
recomputed only when its inputs move past an epsilon (reusing the change-detection already present
in `resonator_stack.rs`). The prepared operator is an **efficiency substrate that frees CPU for the
realtime nonlinear core** — explicitly not a reduced, static end-model.

## Consequences

- A new per-sample energy follower is required; none exists in `lindelion-dsp-utils` today (only
  static `rms` over a slice).
- The effort/energy bus becomes a new modulation source/destination family layered on the existing
  `ModulationSources`/`ModulationDestination` spine.
- The control-rate `PreparedResonatorModel` must be proven output-equivalent to the current
  per-sample derivation before any nonlinear behavior is added (a refactor gate, not a sound
  change).
- The expressive low-polyphony budget ([ADR-0015](0015-expressive-low-polyphony-budget.md)) makes
  the per-voice nonlinear richness affordable; the oversampled inner loop
  ([ADR-0016](0016-oversampled-nonlinear-inner-loop.md)) and additive driver layer
  ([ADR-0017](0017-additive-physical-driver-layer.md)) build on this chain.
- Each stage gains an objective dynamic-response test (e.g. spectral centroid rises with drive),
  not only a finite/decaying check.

## Alternatives

- **Velocity-as-gain only (status quo).** Rejected: it flattens dynamics to loudness and is the
  exact limitation this program targets.
- **A static reduced/commuted body (the "few-formant" perceptual shortcut).** The literature
  validates commuted synthesis and reduced bodies for linear, single-dynamic playback. Rejected as
  the *end* model: it bakes the body into a fixed character and cannot express the dynamic balance
  between subsystems. The efficiency idea is retained only as the prepared-operator substrate.
- **Recompute operators per sample (status quo).** Rejected: it spends on note-constant
  coefficients the CPU the nonlinear core needs.
- **Player-effort-only or measured-energy-only bus.** Rejected: effort-only approximates the
  vibrating state instead of measuring it (no true bloom/steepening); energy-only loses the
  anticipatory driver-force character that makes an attack read as hard or soft. The hybrid keeps
  both.
