# Dynamic Response — Implementation Plan

Turn Lamath into a whole-system *dynamic-response* instrument. Today its resonators react to
playing dynamics mostly as level; the goal is an instrument whose **timbre changes across the whole
chain** — driver, resonator/transport, body, and the couplings between them — driven by a shared
effort/energy bus, so loud-vs-soft is a physical change of character rather than a gain change. This
is the differentiator: existing physical-modeling synths flatten dynamics by giving one subsystem a
static character and driving it with a velocity-scaled impulse.

**This program targets the waveguide family only.** The Modal resonator (`ModalBank` in
`dsp/modal.rs`) already sounds good and is preserved unchanged as the quality reference; the
weak-sounding waveguides (1D string, 1D tube, 2D mesh) are what this work fixes. Out of scope:
**any change to `ModalBank`'s behavior**; shipping new presets/sound design beyond what each
milestone needs for its tests; a CLAP/AU adapter; any change to the plugin framework (see
[ADR-0002](docs/adr/0002-no-plugin-framework.md)). This plan is milestone altitude — step-level
file/test detail is filled just-in-time by the `plan-phase` skill before each milestone runs.

## Confirmed decisions

- **Architecture.** Chain model (driver → coupling → resonator → two-way body → surrounding) driven
  by a shared effort/energy bus. Prepared operators are an efficiency substrate to fund the
  nonlinear core, not a reduced end-model. ([ADR-0014](docs/adr/0014-dynamic-response-effort-energy-bus.md))
- **Effort/energy bus is hybrid.** Player effort (velocity/pressure/breath) drives the driver;
  per-sample measured resonator energy drives resonator nonlinearity, couplings, and body balance.
- **Budget: expressive low polyphony (1–4 voices).** Spend per-voice richness, not voice count;
  full nonlinear physics including mesh nonlinearity and a two-way body run per voice.
  ([ADR-0015](docs/adr/0015-expressive-low-polyphony-budget.md))
- **Global 2x oversampled nonlinear inner loop from the start**, allocation-free, identity-
  equivalent at zero nonlinearity. ([ADR-0016](docs/adr/0016-oversampled-nonlinear-inner-loop.md))
- **Driver layer is additive.** Physical drivers are a new selectable source; sample/sidechain
  excitation stays. ([ADR-0017](docs/adr/0017-additive-physical-driver-layer.md))
- **Take perceptual "few-formant body" shortcuts with a grain of salt.** The body is ≈linear (that
  is physics, not a shortcut); reduce it on physical modal-overlap grounds and keep it two-way
  coupled, never a post-EQ.
- **The Modal resonator stays as-is.** `ModalBank` is the reference-quality model; it gets no
  nonlinearity, no energy modulation, and no refactor. Every milestone applies to the waveguides.

## Context / reuse map

Reference behavior the executor re-derives from here. Verify against current code, not this summary.

**Composition seam — `plugins/lamath/src/dsp/voice/resonator_stack.rs` (+ `resonator_stack/`).**
Two parallel `ResonatorEngine`s (`resonator_a`/`resonator_b`), each wrapping `ModalBank`,
`WaveguideResonator`, or `MeshResonator`, with `routing` ∈ {Parallel, Series, BodyColor (a → exciter
→ b)}. Control-rate methods already exist: `set_base_configs`, `configure_modulated` (epsilon-gated:
reconfigures only when `force || config != prev`), `retune`, `set_waveguide_loop_gain`,
`apply_structural_transitions`, `current_loop_gain`. Per-sample audio enters each engine's
`process_sample`. This epsilon gate is the natural seat for the `PreparedResonatorModel` cache.

**Modulation spine — `plugins/lamath/src/dsp/voice/modulation_state.rs`, `patch.rs`.**
`ExpressionStream` carries `pressure`, `brightness`, `velocity`, `pitch_bend` (+ `mod_wheel`).
Sources: SecondaryEnvelope, Lfo, Velocity, Aftertouch, ModWheel, Brightness. Destinations:
FilterCutoff, ResonatorA/B Damping, ResonatorA/B Position, ExcitationGain, LfoRate. The effort/energy
bus is a new source family computed in `ModulationState::next_sources`, with the measured-energy half
sampled in the `resonator_stack` per-sample loop.

**Reusable DSP — `crates/lindelion-dsp-utils`.** `DelayLine` (4-point Lagrange/cubic fractional
read), `FirstOrderAllpass`, `WaveguideDispersion` (two allpass stages), `Biquad`/`BiquadCoefficients`,
`Svf`, `OnePoleLowpass`, `soft_saturate(input, drive)`, `Adsr`; analysis: `rms`, `peak_abs`,
`audio_window_metrics` (peak/rms/dc/spectral_centroid), `harmonic_decay_profile`,
`spectral_centroid_hz`, `sampled_high_frequency_ratio`, `estimate_f0_autocorrelation[_refined]`,
`rms_difference`, `gain_fitted_rms_difference`, `assert_all_finite`. **Gaps to build:** no per-sample
energy follower (only static `rms`); no oversampling stage.

**Realtime-safety harness — `crates/lindelion-test-allocator`.** `install_test_allocator!()` global
allocator + `assert_no_allocations("label", || ...)`. Every audio-thread path added here needs
coverage (see [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md)).

**Current nonlinearity.** `soft_saturate` is applied at boundary reflections
(`waveguide/string_1d.rs`, `tube_1d.rs`) via `loop_nonlinearity`. `mesh_2d` is a **fixed linear
14x10 scattering grid** today. Excitation (`dsp/excitation.rs`) is sample/sidechain playback scaled
by `velocity_to_gain` — no physical driver.

**Reference research.** Stable nonlinear schemes: Bilbao (*Numerical Sound Synthesis*), Ducceschi,
Woodhouse, Bank/Sujbert (tension modulation). Body acoustics (signature modes + formants, air/
Helmholtz coupling): Woodhouse / euphonics.org. See memory `project_lamath_design_philosophy` and
`project_lindelion_fidelity_tests`.

## Cross-cutting constraints

- **Allocation-free realtime path.** Every new audio-thread struct sizes its buffers at
  construction and adds `assert_no_allocations` coverage. ([ADR-0001](docs/adr/0001-allocation-free-audio-thread.md))
- **Required DSP is a product requirement.** Tension modulation, steepening, mesh nonlinearity,
  drivers, and the coupled body are the deliverable; do not substitute a simpler design, different
  semantics, or a bypass without explicit approval (AGENTS.md, this plan's `[DECISION]` tags).
- **Objective audio tests per effect.** Each effect lands with a measurable, dynamic-dependent
  assertion (e.g. transient pitch-sharpening vs drive, centroid rises with drive, regime threshold
  vs pressure), not only finite/decaying checks.
- **No plugin framework; macOS bundle path unchanged.** ([ADR-0002](docs/adr/0002-no-plugin-framework.md),
  [ADR-0007](docs/adr/0007-macos-vst3-build-path.md))
- **Doc surface follows convention.** `architecture.md` and the AGENTS code-map are updated *as each
  milestone lands* (current-state assertions), with one curated `CHANGELOG.md` line per user-visible
  change. Trade-offs stay in the ADRs.
- **Verification is `make ci`.** Per-milestone CPU is tracked by `make bench` at the 1–4 voice
  target.
- **`ModalBank` is the untouched reference.** No milestone changes its behavior. It stays in the
  equivalence battery so any refactor (M1) or shared-code reuse (M7) that would regress it fails the
  gate.

## Milestones

Foundations M1–M3 unblock everything; M4–M6 are siblings; M7 precedes M9 because the dynamic balance
needs a real body.

### M1 — Prepared-operator / control-rate refactor
Hoist per-sample linear derivations into a `PreparedResonatorModel` recomputed only when inputs move.
- Move `loop_damping` (incl. the 96-point peak scan), `dispersion_profile`, `waveguide_geometry`,
  `delay_tuning`, and `BodyProfile` derivation out of `process_sample` into a control-rate prepared
  model behind the existing `resonator_stack` epsilon gate.
- Keep cheap scalar smoothing on the physical inputs so control-rate recompute does not zipper.
- **[DECISION]** Whether `PreparedResonatorModel` and the energy follower are promoted to
  `lindelion-dsp-utils` now or kept local in Lamath (single consumer today; see
  [ADR-0003](docs/adr/0003-shared-core-extraction.md)).
- Exit: `make ci` green; output equivalence to pre-refactor within tolerance (RMS + per-harmonic
  decay) across the existing render battery; `assert_no_allocations` on the refactored path; `make
  bench` shows reduced per-sample cost.

### M2 — Effort/energy bus spine [depends on M1]
Build the missing per-sample energy follower and the hybrid bus.
- Add an allocation-free one-pole energy/RMS follower primitive.
- Add the hybrid effort/energy source family (effort from `ExpressionStream`; energy measured in the
  `resonator_stack` loop) as new modulation sources/destinations.
- **[DECISION]** Whether the effort/energy bus is user-routable in the mod matrix or fixed internal
  wiring (taste + UI surface).
- Exit: `make ci` green; bus tracks a known effort/energy input within tolerance; no-alloc coverage;
  no audible change yet when depths are zero (regression guard).

### M3 — 2x oversampled nonlinear inner-loop harness [depends on M1]
The shared substrate for all nonlinear stages.
- Add a reusable, fixed-buffer, allocation-free 2x oversampling stage (half-band up/down filters)
  wrapping the resonator core; report its latency.
- Exit: `make ci` green; identity-equivalent (within filter tolerance) at zero nonlinearity vs M1
  output; no-alloc coverage; latency reported; `make bench` quantifies the ~2x core cost.

### M4 — String tension modulation [depends on M2, M3]
Energy modulates effective delay length: the "bloom" of a hard pluck.
- Drive `string_1d` one-way delay from measured energy via a stable, bounded scheme.
- **[DECISION]** Default modulation depth/voicing (tuning by taste).
- Exit: `make ci` green; objective transient pitch-sharpening increases with drive, settling as the
  note decays; tuning unaffected at low drive; finite/bounded across the sweep; no-alloc.

### M5 — Tube/brass finite-amplitude steepening [depends on M2, M3]
Energy-dependent bore steepening: loud turns brassy.
- Add amplitude-dependent steepening/dispersion along the bore in `tube_1d`, stable under hard drive.
- **[DECISION]** Default steepening depth and its interaction with `loop_nonlinearity`.
- Exit: `make ci` green; spectral centroid / high-frequency ratio rises measurably with drive;
  fundamental tuning preserved; bounded; no-alloc.

### M6 — Mesh von Kármán geometric nonlinearity [depends on M2, M3]
Make the linear 2D mesh amplitude-coupled: gong/plate bloom and shimmer.
- Add energy-dependent geometric coupling (wave-speed/tension steering) to `mesh_2d`, with a stable
  energy-conserving scheme.
- **[DECISION]** Mesh nonlinearity depth and whether a per-voice quality control gates it under the
  low-poly budget.
- Exit: `make ci` green; objective amplitude-dependent mode coupling / upward energy spread vs drive;
  stable at parameter extremes; fixed memory; no-alloc; `make bench` within the per-voice budget.

### M7 — Two-way-coupled reduced body [depends on M1]
Replace the heuristic biquad body with a physically reduced, coupled body.
- Build a reduced modal body (a handful of signature modes + an explicit air/Helmholtz coupled mode
  + a broad formant for the modal-overlap region) for the **waveguide** path, coupled via bridge
  admittance (two-way), not a post-EQ. May reuse `modal.rs` *code*, but must not modify the
  `ModalBank` resonator slot.
- **[DECISION]** Mode count and which body families ship; whether air-mode coupling is a user control.
- Exit: `make ci` green; body changes which modes the source drives (two-way, not just additive EQ);
  spectrally distinct per family; finite/decaying; no-alloc.

### M8 — Force-dependent driver layer [depends on M2]
Additive physical drivers feeding the resonator.
- Add a driver stage (reed/lip pressure-flow, bow stick-slip, pick/hammer contact) as a selectable
  source driven by the effort bus, running inside the oversampled loop; sample/sidechain preserved.
- **[DECISION]** Which driver archetypes ship first and their parameter surface.
- Exit: `make ci` green; per-driver objective force behavior (e.g. reed threshold vs pressure,
  contact brightness vs strike force); existing sample/sidechain patches unchanged; no-alloc.

### M9 — Coupling/contact stage and source↔body balance [depends on M7, M8]
The gesture interface and the dynamic balance between subsystems.
- Add contact time / slip / strike-position spread between driver and resonator, and the energy-
  dependent source-versus-body mix (warmth at low dynamics).
- **[DECISION]** Default balance curve and contact-model voicing.
- Exit: `make ci` green; picked-vs-strummed and hard-vs-soft produce distinct, measurable timbres
  via contact + balance (not gain alone); no-alloc.

### M10 — Effort-scaled surrounding effects [depends on M2]
The "other surrounding effects on the sound," scaled by effort/energy.
- Add pick/breath/key mechanical noise, sympathetic resonance, and radiation brightening, scaled by
  the effort/energy bus.
- **[DECISION]** Which surrounding effects ship and how sympathetic resonance is routed (cf. the
  existing cross-coupling/sympathetic backlog item).
- Exit: `make ci` green; surrounding components scale measurably with effort and are defeatable;
  no-alloc.

### Decisions needing your input

| Where | Decision you own |
| ----- | ---------------- |
| M1 | Promote `PreparedResonatorModel` + energy follower to `lindelion-dsp-utils` now, or keep local (single consumer today). |
| M2 | Effort/energy bus user-routable in the mod matrix, or fixed internal wiring. |
| M4 | Default string tension-modulation depth / voicing. |
| M5 | Default bore-steepening depth and interaction with `loop_nonlinearity`. |
| M6 | Mesh nonlinearity depth; whether a per-voice quality control gates it. |
| M7 | Reduced-body mode count, which body families ship, and whether air-mode coupling is a user control. |
| M8 | Which driver archetypes ship first and their parameter surface. |
| M9 | Default source↔body balance curve and contact-model voicing. |
| M10 | Which surrounding effects ship; sympathetic-resonance routing. |

---

This plan is the single source of truth. To execute, run `plan-phase` on one milestone to expand it
into ordered, file-level steps, then the companion execution prompt to run them.
