# Phase M5 — Tube/brass finite-amplitude steepening (execution steps)

Expanded from milestone **M5** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M2, M3], both landed; sibling of M4). The second nonlinear stage: energy-dependent
bore steepening so a loud bore turns *brassy* — the spectrum brightens with playing energy and
mellows when soft. Scope is `tube_1d` only; String (M4), Mesh (M6), and `ModalBank` are untouched.

## Reference behavior (re-derive before editing)

**Finite-amplitude steepening (brass; Bilbao, Hirschberg/Myers/Campbell — plan §Reference
research).** In a brass bore the wave speed depends on local pressure (crests travel faster than
troughs), so a loud wave *steepens* as it propagates and develops upper harmonics — the brassy
"cuivré". The accumulated effect over a round trip is modelled in a digital waveguide as a
**nonlinearity applied to the circulating bore wave once per round trip whose strength rises with
the wave amplitude (measured energy)**. It generates upper harmonics — the M3 2× oversampler exists
precisely to keep those alias-controlled — while leaving the loop length (and so the fundamental
tuning) unchanged.

**Where it plugs in (post-M1/M3).** `Tube1d::reflected_sample` already applies the bore nonlinearity
at the **mouth (left) boundary**: `soft_saturate(filtered, drive)` with `drive =
clamp(params.loop_nonlinearity, 0, 1)` — a *static* per-note character. M5 makes that drive
**energy-dependent**: the static `loop_nonlinearity` is the base, and the measured-energy bus adds a
dynamic steepening term on top (the `[DECISION]` owns how they combine). At zero energy the drive
equals `loop_nonlinearity`, so the bore is bit-identical to pre-M5. The steepening is a memoryless
waveshaper on the boundary sample, so the fundamental period (tuning) is preserved by construction.

**Energy plumbing (extend M4's).** M4 threads `energy` from the voice through
`ResonatorEngine::process_sample(input, energy)` → `WaveguideResonator::set_tension_drive(energy)` →
`String1d` only. M5 forwards the same bus to the tube. Rename the forwarder to reflect that the bus
now serves two effects: `WaveguideResonator::set_energy_drive(drive)` calls
`self.string.set_tension_drive(drive)` (M4, unchanged) **and** `self.tube.set_steepening_drive(drive)`
(new); update the one `ResonatorEngine` caller. `set_steepening_drive` stores the drive (default
`0.0` → inert) on `Tube1d`, set once per host sample (constant across the 2× sub-samples).

## Tuning-metric note (tube)

Wideband autocorrelation on a noise-burst-excited bore is unreliable (M3 saw spurious ~160 Hz on a
196 Hz tube). Because M5 changes *only* the boundary waveshaper drive (not the delay), tuning is
structurally identical between a quiet and a loud render — so the tuning-preservation test compares
the **quiet vs loud render's `f0` to each other** (they must match within a few cents), which is
robust to the estimator's absolute bias, rather than to a Hz target.

---

## Steps

### 1. **[DECISION]** Default steepening depth and its interaction with `loop_nonlinearity` — **RESOLVED**
- *(This is the milestone's `[DECISION]`: "Default steepening depth and its interaction with
  `loop_nonlinearity`.")* **RESOLVED: additive combination, strong depth ≈ 0.8, squared curve.**
  Internal constants in `tube_1d.rs` (no new user parameter in M5 — fixed internal wiring per M2;
  exposing a patch control is later scope):
  - **Interaction:** additive — `effective_drive = clamp(loop_nonlinearity + STEEPEN_DEPTH·drive_term,
    0, 1)`. `loop_nonlinearity` stays the static base brassiness; energy adds dynamic steepening on
    top, so a `loop_nonlinearity = 0` bore is mellow when soft and brightens only when played hard.
  - **Depth + voicing:** `STEEPEN_DEPTH ≈ 0.8` (pronounced cuivré bloom — near-full saturation at peak
    energy even from a clean base). Squared, normalized curve `drive_term = clamp((energy/E_REF)², 0,
    STEEPEN_MAX_DRIVE)` with `E_REF ≈ 0.15` (matching M4's energy scale); `STEEPEN_MAX_DRIVE ≈ 1.0`
    so the dynamic term caps near `depth` and `effective_drive` stays ≤ 1 for `soft_saturate`.

> **As-built note (operator revised during execution, with two user decisions).** The plan's
> in-loop `soft_saturate` could *not* brighten the bore: at the bore's low circulating level it
> barely engages, and stronger in-loop saturation compresses the fundamental and kills the sustain.
> Root cause (surfaced by the user's question "if the lowpass filters the harmonics, why is it
> there?"): the bore's round-trip lowpasses are the physical **frequency-dependent reflection** —
> lows reflect and circulate, highs **radiate out the bell** — and the model was *discarding* the
> radiated highs as loop loss instead of routing them to the output. Resolved design (user-approved):
> (a) **in-loop amplitude-dependent dispersion allpass** at the mouth — unity magnitude (loop-stable,
> sustain intact), nonlinear amplitude-driven coefficient steepens the wavefronts (cumulative); plus
> (b) an **energy-gated bell-radiation output path** — a highpass on the bell-incident wave radiates
> the steepening harmonics to the output. The static `loop_nonlinearity` `soft_saturate` is preserved
> unchanged. Inert at zero energy (existing tube tests bit-identical); on a clean sustained tone the
> centroid rises ~+21% at full drive with the fundamental tuning preserved.

### 2. Tube steepening scheme + energy drive plumbing  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/tube_1d.rs` (the scheme + `steepening_drive`
  state), `plugins/lamath/src/dsp/waveguide.rs` (rename `set_tension_drive` →
  `set_energy_drive`, forward to the tube as well as the string),
  `plugins/lamath/src/dsp/voice/resonator_stack.rs` (update the one `ResonatorEngine` caller to
  `set_energy_drive`). The rename is the plan-specified plumbing refactor for serving two effects
  off one bus; behavior for the String path is unchanged.
- **Reference behavior:** as above — in `Tube1d::reflected_sample` (left/mouth side) compute the
  effective waveshaper drive from `params.loop_nonlinearity` combined with the stored
  `steepening_drive` per the step-1 voicing, then `soft_saturate(filtered, effective_drive)`
  (the existing bounded shaper). `steepening_drive` is set per host sample and reset to `0.0` in
  `Tube1d::reset`. The mouth boundary is the once-per-round-trip site, and the prepared-model cache
  key is unchanged (energy is not a cache input).
- **Change:** Add `steepening_drive: f32` to `Tube1d` (default `0.0`) and `set_steepening_drive(&mut
  self, drive)`; replace the fixed `drive` in `reflected_sample`'s left branch with the
  energy-combined `effective_drive`. Add `WaveguideResonator::set_energy_drive` (forwards to both
  string and tube); rename the call site in `ResonatorEngine`. Reset clears `steepening_drive`.
- **Verify:** New `#[cfg(test)]` test in `tube_1d.rs` driving `set_steepening_drive` directly: (a)
  **brightens with drive** — a burst-excited tube rendered at higher energy has a measurably higher
  early-window spectral centroid (or `sampled_high_frequency_ratio`) than at lower/zero energy; (b)
  **tuning preserved** — the quiet and loud renders' estimated `f0` match each other within a few
  cents (per the tuning-metric note). Red→green: `set_steepening_drive` and the energy-combined drive
  do not exist yet (compile-level red), and at zero drive the render is bit-identical to pre-M5
  (existing Tube render/tuning/equivalence tests stay green — the inert-at-zero guarantee).

### 3. Stability/bounds and no-allocation guards  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/waveguide/tube_1d.rs` (tests),
  `plugins/lamath/src/dsp/voice/resonator_stack/tests.rs` (no-alloc test for a Tube patch).
- **Reference behavior:** M5 exit — "bounded; no-alloc." ADR-0001 requires no-alloc coverage on the
  new audio-thread work (the per-sample drive combine + `set_steepening_drive`). `effective_drive` is
  clamped to `[0,1]` and `soft_saturate` is bounded, so the bore stays bounded under any energy.
- **Change:** Tests only (no production change unless the sweep surfaces a real
  finiteness/stability defect, in which case tighten the clamp — not the semantics).
- **Verify:** New `#[cfg(test)]` tests: (a) drive `Tube1d` with an extreme/noisy/non-finite
  steepening drive across several frequency/cutoff/boundary settings and assert the output stays
  finite and `peak_abs` bounded; (b) `assert_no_allocations` over an engine render of a **Tube**
  waveguide patch that sets a non-zero steepening drive each sample (the M4 no-alloc test uses a
  String/default config). Red→green: the bounded-under-extreme-drive contract on the tube is new.

### 4. Exit gate
- **File(s):** verification only; doc-surface update per the plan's cross-cutting constraint.
- **Reference behavior:** M5 exit — "`make ci` green; spectral centroid / high-frequency ratio rises
  measurably with drive; fundamental tuning preserved; bounded; no-alloc." Plan cross-cutting:
  per-milestone CPU tracked by `make bench` at the 1–4 voice target (ADR-0015).
- **Change:** Add one curated `CHANGELOG.md` line (loud bores now brighten/turn brassy and mellow
  when soft; bores at rest unchanged; tuning unchanged). Run `make bench` to confirm the per-voice
  cost stays within the low-poly budget; record the figure.
- **Verify:** Run `make ci` (green) and show output. The step-2 brightness/tuning tests and step-3
  guards satisfy the behavioral exit clauses; `make bench` confirms the budget.

---

## Decision resolved

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 1 | Tube steepening depth/voicing and interaction with `loop_nonlinearity`. | **RESOLVED: additive, strong depth ≈ 0.8, squared curve.** `effective_drive = clamp(loop_nonlinearity + 0.8·clamp((energy/0.15)², 0, 1), 0, 1)` into the existing bounded `soft_saturate`. Internal constants in `tube_1d.rs`; not a user parameter in M5 (tunable later). |
