# Phase M7 — Two-way-coupled reduced body (execution steps)

Expanded from milestone **M7** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M1], landed). Replace the heuristic biquad body with a **physically reduced modal body
coupled two-way at the bridge** — so the body changes *which modes the string drives*, not just an
EQ on the output. Scope is the waveguide path; `ModalBank` is untouched (the body reuses
`ModalMode` *code* but must not modify the `ModalBank` resonator slot).

## Reference behavior (re-derive before editing)

**String–body bridge coupling (Woodhouse; Smith *Physical Audio Signal Processing*; euphonics.org —
plan §Reference research).** A real string couples to the instrument body at the **bridge**, which
presents a frequency-dependent driving-point **admittance** `Y_b(ω)`. The coupling is **two-way**:
the body is driven by the bridge force, and the body's motion is the string's bridge boundary
condition. Consequences a post-EQ cannot reproduce: near a body resonance the bridge is "open" (the
body absorbs and radiates energy), so the string partial there **decays faster or is pulled in
frequency** (the body's signature, the wolf note) — the body *changes which modes the string
sustains*. The radiated sound is the **body's** surface motion (through a radiation transfer), not
the string pickup. The body is ≈linear (that is physics, not a shortcut); it is *reduced* on
modal-overlap grounds — a handful of low **signature modes**, an explicit **air/Helmholtz** cavity
mode, and a **broad formant** standing in for the dense high-frequency region where individual modes
overlap.

**Digital bridge (waveguide).** The bridge reflectance is `R(z) = (1/Z_s − Y_b(z)) / (1/Z_s +
Y_b(z))` for a string of characteristic impedance `Z_s` terminating in admittance `Y_b(z)`. With
`Y_b` a reduced modal sum, `|R|` **dips** at body resonances (the string loses energy there → faster
partial decay), and the transmitted `(1 + R)` **peaks** there (the body radiates strongly at its
modes). The reflectance sits **in the string loop** (two-way), and the transmitted signal drives the
body radiation = the output.

**Current wiring (post-M1/M4/M5).** `body.rs`'s `WaveguideBody` is a one-way post-EQ: four cached
biquads (`highpass`, `lowpass`, `low_resonance`, `high_resonance`) applied to the resonator pickup.
In `string_1d.rs` it filters `pickup.average()`; in `tube_1d.rs` it filters the bore pickup and is
summed with the M5 bell radiation. The string's bridge is the **left** termination (the
loop-damping filter, `reflected_sample(BoundarySide::Left, …)`); the right is rigid. The body is a
separate instance per resonator, so the string and tube bodies are independent.

**Scope of the two-way coupling.** "Bridge admittance" is a string concept; M7's two-way coupling
targets the **String**. The reduced modal body replaces the String's heuristic body and couples at
its bridge termination. The **Tube** has no bridge — M5's bell radiation already models its body
coupling; whether the Tube also adopts the reduced-body *radiation* (still one-way, a bore has no
bridge) is part of the `[DECISION]` (body families). Modal is untouched.

## Difficulty note (read before steps 2–3)

Two-way string–body coupling is a known-tricky, potentially-unstable area (a body resonance fed
back into the string loop can ring up). Like M5/M6, the recommended scheme is a starting point. If a
stable, passive, audibly-two-way coupling does not land, **surface the finding and options to the
user** rather than silently falling back to a post-EQ (a post-EQ is exactly what M7 must replace —
the confirmed decision says the body is two-way coupled, never a post-EQ). Passivity (the coupling
splits energy between reflected and radiated, never injects) is the hard constraint.

---

## Steps

### 1. **[DECISION]** Mode count, body families, and the air-mode control — **RESOLVED**
- *(This is the milestone's `[DECISION]`: "Mode count and which body families ship; whether air-mode
  coupling is a user control.")* **RESOLVED: ~10–12 modes; ship Guitar + Violin string families;
  air-mode fixed per family.** Internal constants/configs in the body module (no new user parameter
  in M7):
  - **Mode count:** ~10–12 modes per family — roughly 8–10 low **signature modes** + 1 **air/
    Helmholtz** cavity mode + 1 **broad formant** for the high-frequency modal-overlap region. The
    richer signature is closer to a measured driving-point admittance; size the modes at construction
    (allocation-free, like `ModalBank::with_capacity`) and keep the two-way coupling bounded for
    stability (more modes = more care needed).
  - **Body families:** ship **Guitar** (large air cavity, lower signature modes) and **Violin**
    (brighter, higher signature modes, strong formant) — clearly spectrally distinct. The **Tube
    keeps its M5 bell radiation** (a bore has no bridge; no Tube reduced body in M7).
  - **Air-mode:** **fixed per family** (baked into each family's mode table); not a user parameter
    in M7.

### 2. Build the reduced modal body component  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/body.rs` (a new reduced-body type alongside or
  replacing `WaveguideBody`'s internals), reusing `crate::dsp::modal::ModalMode` (a `pub` 2-pole
  mode; **do not touch `ModalBank`**). Incidental: a per-family config table.
- **Reference behavior:** as above — the reduced body is a sum of `ModalMode`s: a few low signature
  modes, one air/Helmholtz mode, one broad (low-Q) formant mode, per family. It models the body's
  driving-point response; fixed-size (modes allocated at construction, allocation-free re-tune,
  mirroring `ModalBank::with_capacity`/`MeshResonator`). This step builds and unit-tests the
  component **standalone** (not yet wired two-way — the old `WaveguideBody` stays in use until
  step 3, so guard against dead-code as in M2 if needed).
- **Change:** Define the reduced body (modes + per-family config + `new`/`reset`/a drive method) in
  `body.rs`, built from `ModalMode`. No change to `string_1d.rs`/`tube_1d.rs` yet.
- **Verify:** New `#[cfg(test)]` tests in `body.rs`: (a) **spectrally distinct per family** — two
  families driven by the same input produce measurably different spectra (centroid / shape); (b)
  **finite/decaying** — an impulse-driven body is finite and rings down. Red→green: the reduced-body
  type does not exist yet (compile-level red for new code).

### 3. Two-way couple the reduced body at the String bridge  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs` (the bridge termination + output),
  `plugins/lamath/src/dsp/waveguide/body.rs` (the bridge-coupling/reflection method), and the
  existing body/string tests that the new body changes.
- **Reference behavior:** as above — the body's modal admittance enters the **bridge reflection**
  (the String's left termination), so it sits in the loop (two-way): `|R|` dips at body modes (string
  partials there decay faster / are pulled), and the transmitted signal drives the body radiation =
  the String output (replacing the pickup post-EQ). Recommended starting scheme (stable, passive):
  drive the body with the bridge-incident wave, radiate the body's velocity as the output, and
  subtract the body's (bounded) reaction from the reflected wave so energy splits between the loop
  and the radiated body — never injected. The exact `R(z) = (1/Z_s − Y_b)/(1/Z_s + Y_b)` form is the
  refinement; keep the coupling bounded for stability.
- **Change:** Route the String's bridge termination through the reduced body (two-way), make the
  String output the body's radiation, and remove the one-way pickup post-EQ for the String. Update
  the existing body/string tests whose sound the new body changes (their thresholds, not their
  intent). Keep the Tube on its current body/M5 radiation unless the step-1 decision added a Tube
  reduced body.
- **Verify:** New `#[cfg(test)]` test: **two-way, not just EQ** — a string partial near a body
  resonance decays faster (or shifts) than the same partial with the body bypassed / with a flat
  body, i.e. the body changes *which modes survive in the loop*, which an output-only EQ cannot do.
  Red→green: the two-way coupling does not exist yet; before it the body only EQs the output (the
  loop's partial decay is body-independent). Existing String tuning/finite/decay tests stay green
  (the fundamental still tunes; the body colors decay, not pitch).

### 4. Stability/passivity, bounds and no-allocation guards  [depends on #3]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs` / `body.rs` (tests),
  `plugins/lamath/src/dsp/voice/resonator_stack/tests.rs` (no-alloc, if the String path's coverage
  needs extending for the body).
- **Reference behavior:** M7 exit — "finite/decaying; no-alloc." The two-way coupling must stay
  **passive** (never injects energy → the string-plus-body always decays) and bounded across the
  parameter range; ADR-0001 requires no-alloc coverage on the new audio-thread work.
- **Change:** Tests only (no production change unless the sweep surfaces a real
  instability/energy-injection defect, in which case tighten the coupling bound — not the
  semantics).
- **Verify:** New `#[cfg(test)]` tests: (a) the body-coupled String stays finite and **decays** (no
  ring-up) across body families, frequencies, and loop gains, including high loop gain where a body
  resonance could otherwise destabilize; (b) `assert_no_allocations` over a String render (and/or an
  engine render) exercising the body coupling. Red→green: the coupled-and-stable contract is new.

### 5. Exit gate
- **File(s):** verification only; doc-surface update per the plan's cross-cutting constraint.
- **Reference behavior:** M7 exit — "`make ci` green; body changes which modes the source drives
  (two-way, not just additive EQ); spectrally distinct per family; finite/decaying; no-alloc." Plan
  cross-cutting: per-milestone CPU tracked by `make bench` at the 1–4 voice target.
- **Change:** Add one curated `CHANGELOG.md` line (the String now plays through a physically reduced,
  two-way-coupled body — the body's signature/air modes shape the string's decay and radiate it,
  selectable per body family — replacing the heuristic output EQ). Run `make bench` to confirm the
  per-voice cost stays within the budget.
- **Verify:** Run `make ci` (green) and show output. The step-2 spectral-distinctness, step-3
  two-way, and step-4 stability/no-alloc tests satisfy the exit clauses; `make bench` confirms the
  budget.

---

## Decision resolved

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 1 | Reduced-body mode count, which families ship, Tube treatment, air-mode control. | **RESOLVED: ~10–12 modes (≈8–10 signature + air + formant); ship Guitar + Violin string families; Tube keeps its M5 bell radiation; air-mode fixed per family.** Internal configs in the body module; not a user parameter in M7. |
