# Phase M4 — String tension modulation (execution steps)

Expanded from milestone **M4** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M2, M3], both landed). The first nonlinear stage: energy modulates the string's
effective delay length so a hard pluck *blooms* — transient pitch-sharpening at the attack that
settles back to nominal as the note decays. Scope is `string_1d` only; Tube (M5), Mesh (M6), and
`ModalBank` are untouched.

## Reference behavior (re-derive before editing)

**Tension modulation (Bank/Sujbert, Tolonen/Välimäki, Bilbao — plan §Reference research).** Plucking
a real string hard raises its average tension; higher tension raises wave speed, so the resonant
frequency rises transiently and relaxes as the vibration (and tension) decay. In a digital
waveguide this is modelled by **shortening the effective delay-line length as a function of the
string's (low-passed) energy**: `L_eff = L0 / (1 + k·drive)`, which sharpens pitch monotonically with
drive, returns to `L0` (nominal tuning) at zero drive, and is intrinsically bounded (the delay only
ever shortens, never below `L0/(1+k·drive_max)`, always within the fixed traveling-wave capacity).
The drive is the **measured resonator energy** from the M2 bus (the hybrid bus's measured-energy
half, ADR-0014), not a separate follower.

**Where it plugs in (post-M1/M3).** `String1d::process_sample` reads `prepared.one_way_delay` from
the control-rate `PreparedStringModel` (M1 cache) and uses it for `waves.boundary_samples`,
`waves.pickup_samples`, and `waves.add_symmetric_excitation`. The tension term is a **per-sample
modulation applied on top of** the cached `one_way_delay` — energy is deliberately *not* part of the
prepared-model cache key, so the cache stays warm and the nonlinearity is the cheap per-sample term
the prepared operators were refactored to fund (M1/ADR-0014). The string runs at 2× inside the M3
oversampler, so the scheme runs against the oversampled clock (ADR-0016).

**Energy plumbing (state, not signature).** `ModulationSources.energy` (M2, fixed internal wiring:
"M4–M8 read `sources.energy` directly") is available in the voice loop before
`resonators.process_sample`. Thread it down as a **set-before-process drive** to avoid churning the
many `WaveguideResonator::process_sample`/`String1d::process` call sites (tests, `render_metrics`):
`ResonatorEngine` sets the string's `tension_drive` once per host sample, then runs the unchanged
oversampled core. The drive defaults to `0.0`, so every existing caller stays inert — bit-identical.

## Energy/causality note

The M2 follower measures the resonator output **after** `process_sample` (one-sample feedback), so
the drive applied at sample *n* reflects the output energy through *n−1*. At note-on the follower is
reset to zero, so the very first samples have zero drive (nominal tuning) and the bloom develops as
the attack energy rises — exactly the desired envelope. This composition (M2 energy tracks the note
envelope × M4 tension responds to energy) produces the integrated bloom; the binding unit test
imposes a drive envelope directly so the mechanism is deterministic.

---

## Steps

### 1. **[DECISION]** Default tension-modulation depth and voicing — **RESOLVED**
- *(This is the milestone's `[DECISION]`: "Default modulation depth/voicing (tuning by taste).")*
  **RESOLVED: moderate depth ≈ +40 cents peak, squared (energy²) voicing.** Two internal constants in
  `string_1d.rs` (no new user parameter in M4 — fixed internal wiring per M2; exposing a patch
  control is later scope):
  - **Voicing:** `drive = clamp(energy², 0, drive_max)` — the squared curve concentrates the effect
    on hard hits and keeps low/medium dynamics in tune.
  - **Depth `k`:** sized for ≈ **+40 cents** at a hard pluck's peak energy. Re-derive from the cents
    target: a rise of `c` cents needs `L0/L_eff = 2^(c/1200)`, so `k·drive_peak = 2^(40/1200) − 1 ≈
    0.0234`. Pick `k` against a representative hard-pluck peak `drive` (peak energy² after the M2
    follower) so the peak sharpening lands near +40 cents; the objective test asserts *measurable*
    sharpening, not an exact cent value, so tune `k` to the observed peak.
  - **`drive_max` clamp:** sized so a worst-case energy cannot shorten the delay below a safe
    interpolation margin (delay stays ≳ a few samples), keeping the loop bounded.

### 2. String tension scheme + energy drive plumbing  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs` (the scheme + `tension_drive` state),
  `plugins/lamath/src/dsp/waveguide.rs` (`WaveguideResonator::set_tension_drive` forwarding to the
  string), `plugins/lamath/src/dsp/voice/resonator_stack.rs`
  (`ResonatorEngine::process_sample`/`ResonatorStack::process_sample` gain an `energy` arg; set the
  string drive before the oversampled loop), `plugins/lamath/src/dsp/voice/mod.rs` (pass
  `sources.energy` into `resonators.process_sample`). Incidental: update the few `process_sample`
  call sites that gain the `energy` arg (voice tests, the M3 `resonator_stack/tests.rs` no-alloc
  test) to pass `0.0`.
- **Reference behavior:** as above — `L_eff = one_way_delay / (1 + k·clamp(drive,0,drive_max))` with
  `drive` derived from `tension_drive` per the step-1 voicing; substitute `L_eff` for
  `prepared.one_way_delay` in the three `waves.*` calls in `String1d::process_sample`. `tension_drive`
  is set per host sample (constant across the 2× sub-samples) and reset to `0.0` in `String1d::reset`.
  The prepared-model cache and its key are unchanged (energy is not a cache input).
- **Change:** Add `tension_drive: f32` to `String1d` (default `0.0`) and `set_tension_drive(&mut
  self, drive: f32)`; apply the bounded delay-shortening in `process_sample`. Add
  `WaveguideResonator::set_tension_drive` forwarding to the string (Tube ignored — M5).
  `ResonatorEngine::process_sample(input, energy)`: in the Waveguide arm call
  `self.waveguide.set_tension_drive(energy)` before `self.oversampler.process(...)`; Mesh/Modal arms
  ignore `energy`. `ResonatorStack::process_sample(excitation, energy)`: pass `energy` to
  `resonator_a`/`resonator_b`. Voice: `self.resonators.process_sample(excitation, sources.energy)`.
- **Verify:** New `#[cfg(test)]` test in `string_1d.rs` driving `set_tension_drive` directly
  (deterministic, no full voice): (a) **increases with drive** — a render held at a higher drive has
  a measurably higher early `f0` than one at a lower drive; (b) **settles as energy decays** — under
  a drive envelope that starts high and decays to `0`, early-window `f0` > late-window `f0`; (c)
  **tuning unaffected at low drive** — at `drive == 0` throughout, `f0` matches the requested pitch
  within a few cents. Red→green: `set_tension_drive` and the tension term do not exist yet
  (compile-level red), and at zero drive the render is bit-identical to pre-M4 (existing String
  render/tuning/equivalence tests stay green — the inert-at-zero guarantee).

### 3. Stability/bounds and no-allocation guards  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs` (tests),
  `plugins/lamath/src/dsp/voice/resonator_stack.rs` (no-alloc test, or extend the M3 one).
- **Reference behavior:** M4 exit — "finite/bounded across the sweep; no-alloc." ADR-0001 requires
  no-alloc coverage on the new audio-thread work (the per-sample tension term + `set_tension_drive`).
  The scheme is bounded by construction (`/(1+k·drive)` with clamped non-negative drive only
  shortens the delay, staying within the fixed buffer), so the test asserts that property holds
  across an extreme energy + parameter sweep.
- **Change:** Add tests only (no production change unless the sweep surfaces a real
  finiteness/stability defect, in which case tighten the clamp — not the semantics).
- **Verify:** New `#[cfg(test)]` tests: (a) drive `String1d` with an extreme/noisy drive sequence
  (including very large and rapidly-changing values) across loop-gain/cutoff/frequency settings and
  assert the output stays finite and `peak_abs` bounded; (b) `assert_no_allocations` over an engine
  render of a waveguide-String patch that sets a non-zero tension drive each sample. Red→green: the
  bounded-under-extreme-drive contract is new (the drive path did not exist before).

### 4. Exit gate
- **File(s):** verification only; doc-surface update per the plan's cross-cutting constraint.
- **Reference behavior:** M4 exit — "`make ci` green; objective transient pitch-sharpening increases
  with drive, settling as the note decays; tuning unaffected at low drive; finite/bounded across the
  sweep; no-alloc." Plan cross-cutting: per-milestone CPU tracked by `make bench` at the 1–4 voice
  target (ADR-0015).
- **Change:** Add one curated `CHANGELOG.md` line (hard plucks now bloom — transient pitch-sharpening
  that relaxes; default patches at rest are unchanged). Run `make bench` to confirm the per-voice
  cost stays within the low-poly budget; record the figure.
- **Verify:** Run `make ci` (green) and show output. The step-2 objective tests and step-3 guards
  satisfy the behavioral exit clauses; `make bench` confirms the budget.

---

## Decision resolved

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 1 | String tension-modulation **depth** and **voicing**. | **RESOLVED: ≈ +40 cents peak (moderate), squared (energy²) curve**, `drive_max` clamp sized to keep the delay above a safe interpolation margin. Internal constants in `string_1d.rs`; not a user parameter in M4 (tunable later). |
