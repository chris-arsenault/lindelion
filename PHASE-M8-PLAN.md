# Phase M8 — Force-dependent driver layer (execution steps)

Expanded from milestone **M8** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M2], landed). Add **additive physical drivers** (reed/lip pressure-flow, bow stick-slip,
pick/hammer contact) as a **new selectable excitation source** feeding the resonator, driven by the
**effort** half of the effort/energy bus and running **inside the 2x oversampled inner loop**. Sample
and sidechain excitation stay as one driver option. Scope is the waveguide family; `ModalBank` is
untouched.

## Reference behavior (re-derive before editing)

**Additive driver layer (ADR-0017).** Excitation today is sample + sidechain playback scaled by a
velocity gain (`dsp/excitation.rs`; summed in `voice/mod.rs` and injected via
`resonators.process_sample(excitation, energy)`). There is no physical driver, so the
force-dependent character of a reed, lip, bow, pick, or hammer is absent — and for winds and the
picked-vs-strummed string axis, that driver-under-force behavior is where most of the dynamic
identity lives. M8 adds a driver **stage between excitation selection and the resonator input**,
selectable per patch, with sample/sidechain preserved as one option (existing patches unchanged).

**Where it runs (ADR-0016/0017).** The 2x oversampler lives **inside each `ResonatorEngine`**
(`voice/resonator_stack.rs`: `self.oversampler.process(input, |sample| waveguide.process_sample(
sample, params))`). The driver runs **inside that closure** so its nonlinearity interacts with the
resonator at the higher rate. The driver is therefore a stage on the **waveguide** engine's
excitation path (Modal has no oversampler and is out of scope; whether Mesh also gets a driver is
part of the `[DECISION]`). Consequence to surface: the driver is **per waveguide engine**, so a
Parallel patch with two waveguide slots drives each independently — acceptable for the 1–4 voice
expressive budget, but call it out.

**Effort source (M2).** `ModulationState::next_sources` already computes `effort` (dominant of
velocity and aftertouch/pressure) as `ModulationSources::effort`, currently `#[allow(dead_code)]`.
M8 is where it is consumed: the voice threads `effort` to the driver exactly as it threads `energy`
to the resonator (`set_tension_drive`/`set_energy_drive`). Player **effort** drives the driver;
measured **energy** keeps driving resonator nonlinearity (confirmed-decisions split).

**Stability (Bilbao *Numerical Sound Synthesis*; Woodhouse bow; reed pressure-flow).** Self-
oscillating drivers (reed/bow) are feedback nonlinear systems: below a pressure/force threshold they
are quiescent, above it they enter a self-sustained regime. They can fail to oscillate or blow up if
the coupling is wrong. A contact driver (pick/hammer) is a feed-forward force-shaped transient and is
much simpler. Passivity/bounded energy is the hard constraint, as in M4–M7.

## Difficulty note (read before steps 4–5)

A true reed/bow regime change needs the resonator's **input-end feedback** (bore pressure / string
velocity at the driven termination) fed back into the driver each 2x sample — a tighter coupling
than the current closure exposes. A feed-forward contact/effort driver does **not**. If a stable,
audibly force-dependent self-oscillating driver does not land for a chosen archetype, **surface the
finding and options** (e.g. ship the contact driver first, or a bounded effort-driven excitation
shaper) rather than silently shipping a feed-forward fake of a reed — the confirmed decision is a
*physical* driver. Which coupling model is in scope is settled in step 1.

---

## Steps

### 1. **[DECISION]** Driver archetypes, coupling model, and parameter surface — **needs your call**
- *(This is the milestone's `[DECISION]`: "Which driver archetypes ship first and their parameter
  surface." It is underspecified on the coupling model, which sets steps 4–5's scope, so this step
  settles all three.)* No file change; the executor stops here. Resolve:
  - **Archetype(s) first:** **pick/hammer contact** (feed-forward, force-shaped transient — simplest,
    exercises the contact-brightness test), **reed/lip pressure-flow** (self-oscillating, needs bore
    feedback — exercises the regime-threshold test), and/or **bow stick-slip** (self-oscillating,
    needs string-velocity feedback). Recommend **one feed-forward (pick/hammer) to land the seam +
    contact test**, then one self-oscillator, to bound scope — or a single archetype if you want M8
    minimal.
  - **Coupling model:** **feed-forward** (driver reads `effort` + the upsampled excitation only) vs
    **resonator-coupled feedback** (driver also reads the waveguide's input-end traveling-wave sample
    each 2x step). Feedback is required for a real reed/bow regime; feed-forward suffices for
    pick/hammer. This decides whether step 4's seam only inserts a feed-forward stage or also exposes
    the resonator's feedback sample.
  - **Parameter surface per driver:** e.g. pick — hardness / contact-time (strike position already
    exists on the waveguide); reed — mouth pressure (mapped from `effort`), reed stiffness/cutoff,
    embouchure. Keep it minimal; M11 calibrates ranges.
  - **Which resonators:** waveguide string/tube only (default), or also Mesh.
- **Verify:** none (decision step). Resolution fixes the archetype list, coupling model, and the
  parameter list steps 2–5 implement.

### 2. Add the additive driver config to the patch  [depends on #1]
- **File(s):** `plugins/lamath/src/patch.rs` — a new `DriverConfig` enum and a `driver` field on
  `ResonatorSynthPatch` (or per waveguide slot, matching where the driver will live). Incidental: its
  `Default` impl and inclusion in `ResonatorSynthPatch::default`.
- **Reference behavior:** ADR-0017 — the driver is a **new selectable source**; sample/sidechain
  stays as one option; existing patches unchanged. Mirror the `ResonatorConfig` enum discriminant
  pattern (`patch.rs:156`). The default variant is the **current sample/sidechain-only** behavior, so
  a default patch's driver adds nothing.
- **Change:** define `DriverConfig` (`Sample` default + the archetype variant(s) from #1, each
  carrying its param struct) and store it on the patch. No DSP wiring yet.
- **Verify:** a patch test asserts `ResonatorSynthPatch::default()` has driver `Sample` and that a
  patch carrying an archetype variant round-trips through the existing patch (de)serialization.
  Red→green: `DriverConfig` does not exist yet (compile-level red, greenfield).

### 3. Register the driver-type selector and driver parameters  [depends on #2]
- **File(s):** `plugins/lamath/src/parameters/registry.rs` and the codec module
  (`.../parameters/codecs.rs`).
- **Reference behavior:** the parameter registry is the single source of truth (CLAUDE.md). Mirror
  the `ResonatorModel` codec (`codecs.rs:138`, `define_parameter_codec!`) and the stepped
  "Resonator A Model" entry (`registry.rs:90`) for a **driver-type** selector, plus the continuous-
  parameter pattern (`registry.rs:27`) for each driver scalar from #1. Defaults map to `Sample`, so
  existing automation/patches are unchanged; route each `ParameterPath` to the #2 patch field.
- **Change:** add a `DriverType` codec, one stepped driver-type parameter, and the continuous driver
  parameters; wire their paths to the patch.
- **Verify:** a registry test asserts the driver-type param's plain↔enum mapping (`0 => Sample`
  default) and that each new continuous param exposes its declared range. Red→green: the parameter
  ids / codec do not exist yet (compile-level red).

### 4. Insert the driver stage into the oversampled inner loop (pass-through default)  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/voice/resonator_stack.rs` (+ a `resonator_stack/driver.rs`
  submodule for the stage), and the effort plumbing in `plugins/lamath/src/dsp/voice/mod.rs`
  (thread `sources.effort` alongside `sources.energy` into `resonators.process_sample`, then into
  `ResonatorEngine::process_sample`).
- **Reference behavior:** ADR-0016/0017 — the driver runs **inside** the 2x oversampled closure
  (`resonator_stack.rs` oversampler call; `oversampler.rs::process(input, core)`), feeding the
  waveguide; `effort` comes from the M2 bus (`ModulationSources::effort`, currently dead-code). The
  `Sample`/none driver is an **identity pass-through** (input reaches the resonator unchanged), so a
  patch with no physical driver renders identically — the same identity-guard discipline as M3's
  zero-nonlinearity equivalence (ADR-0016).
- **Change:** add a `Driver` stage owned by the waveguide `ResonatorEngine`; thread `effort` into
  `ResonatorStack::process_sample` and `ResonatorEngine::process_sample`; inside the oversampled
  closure run `driver.process(sample, effort)` before `waveguide.process_sample`. The pass-through
  driver returns its input unchanged. *(If #1 chose the feedback-coupled model, also pass the
  resonator's prior input-end traveling-wave sample into `driver.process`, re-derived from the
  waveguide's driven-termination structure; otherwise feed-forward only.)*
- **Verify:** a regression test renders a waveguide voice/engine with the pass-through driver and
  asserts the output equals the pre-driver path (RMS + sample-wise within tolerance); plus
  `assert_no_allocations` over the seam. Red→green: the `driver`/`effort` arguments do not exist on
  the process path yet (compile-level red); green = identical render through the new seam.

### 5. Implement the first physical driver archetype(s) with objective force tests  [depends on #1, #4]
- **File(s):** `plugins/lamath/src/dsp/voice/resonator_stack/driver.rs` — the archetype DSP, with
  fixed-size state allocated at construction (ADR-0001), mirroring `ModalBank::with_capacity`.
- **Reference behavior:** re-derive the chosen archetype's force law from the cited research (Bilbao
  *Numerical Sound Synthesis*; Woodhouse bow friction; reed pressure-flow). E.g. **reed:** a
  pressure-controlled nonlinear flow with an **oscillation threshold** — quiescent below it,
  self-oscillating above; **pick/hammer:** a hardness-/force-dependent contact pulse whose spectral
  content **brightens with strike force**. Driven by `effort`; runs inside the 2x loop (#4); bounded
  so it cannot inject unbounded energy (M4–M6 stability discipline).
- **Change:** implement the #1 archetype(s) behind the `DriverConfig` variant(s), reading `effort`
  (+ feedback if chosen).
- **Verify:** per-archetype **objective force** test (cross-cutting "objective audio tests per
  effect"): reed — quiescent below an effort/pressure threshold, self-oscillating above (a *regime*
  change, not gain); pick/hammer — spectral centroid / HF ratio rises monotonically with
  effort/strike force; plus finite + bounded across the effort range. Red→green: the archetype does
  not exist yet, and a flat/pass-through driver shows no regime change or brightening (the behavior
  is new).

### 6. Exit gate  [depends on #5]
- **File(s):** verification only, plus doc surface (`CHANGELOG.md`; `docs/architecture.md` and the
  AGENTS code-map current-state, per the cross-cutting doc constraint).
- **Reference behavior:** M8 exit — "`make ci` green; per-driver objective force behavior; existing
  sample/sidechain patches unchanged; no-alloc." Per-milestone CPU via `make bench` at 1–4 voices.
- **Change:** add one curated `CHANGELOG.md` line (the waveguide gains a selectable force-dependent
  physical driver — <archetypes> — driven by playing effort, with sample/sidechain preserved); update
  `architecture.md` / AGENTS code-map if the driver adds a module.
- **Verify:** run `make ci` (green) and show output; an equivalence test confirms a
  sample/sidechain-only patch renders unchanged vs the pre-M8 path (existing-patches-unchanged
  clause); `make bench` shows the per-voice cost within the 1–4 voice budget. The #4 no-alloc and #5
  force tests satisfy the remaining exit clauses.

---

## Decision resolved

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 1 | Driver archetype(s), coupling model, parameter surface, resonator scope. | **RESOLVED: ship pick/hammer + reed; build the resonator-coupled feedback seam now; waveguide string/tube only (no Mesh, no Modal).** Parameter surface: pick → hardness + contact-time (strike position already exists); reed → mouth-pressure (mapped from `effort`) + reed stiffness/cutoff + embouchure. |
