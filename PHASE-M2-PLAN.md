# Phase M2 — Effort/energy bus spine (execution steps)

Expanded from milestone **M2** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M1], landed). Build the missing per-sample energy follower and the hybrid
effort/energy bus, with **no audible change yet** — M4–M8 are the consumers. Scope is the waveguide
family's modulation spine; `ModalBank` and `mesh_2d` behavior are untouched.

## What M2 builds

Per [ADR-0014](docs/adr/0014-dynamic-response-effort-energy-bus.md): a **hybrid** bus — *effort*
(player velocity/pressure/breath, from `ExpressionStream`) and *energy* (a per-sample measured
resonator-energy follower). ADR-0014: "The effort/energy bus becomes a new modulation
source/destination family layered on the existing `ModulationSources`/`ModulationDestination`
spine." Today the spine (`dsp/voice/modulation_state.rs`) computes `ModulationSources`
{amp_envelope, secondary_envelope, lfo, velocity, aftertouch, mod_wheel, brightness} in
`next_sources`, and the voice loop (`dsp/voice/mod.rs:276`) runs `next_sources` → builds excitation
→ `resonators.process_sample` → output. There is no per-sample energy follower
(`lindelion-dsp-utils` has only static `rms` over a slice).

**Design seam:** *effort* is a pure function of the already-present `ExpressionStream` fields,
computed in `next_sources`. *Energy* needs state that persists across samples and sees the resonator
output, so the `EnergyFollower` lives on `ModulationState`; the voice loop feeds it
`resonator_output` after `process_sample`, and the followed value is read into the next sample's
`ModulationSources` (a one-sample feedback delay — correct, and harmless while no consumer exists).

**Resolved from M1:** the prepared-model/energy-follower promotion decision was answered **keep
local** ([ADR-0003](docs/adr/0003-shared-core-extraction.md)); the `EnergyFollower` is therefore a
local Lamath type with a candidate-extraction note, not a `lindelion-dsp-utils` primitive.

Steps 1–3 compute the bus values into `ModulationSources` and are **decision-independent**. The M2
`[DECISION]` (user-routable vs fixed internal wiring) governs only **step 5** — whether the values
are exposed as user-selectable `ModulationSource` variants or stay internal for M4+ to read.

---

## Steps

### 1. Add a local per-sample energy/RMS follower primitive
- **File(s):** new `plugins/lamath/src/dsp/voice/energy_follower.rs`; register `mod energy_follower;`
  in `plugins/lamath/src/dsp/voice/mod.rs`.
- **Reference behavior:** [ADR-0014](docs/adr/0014-dynamic-response-effort-energy-bus.md)
  Consequences — "A new per-sample energy follower is required; none exists in
  `lindelion-dsp-utils` today (only static `rms` over a slice)." Re-derive the standard one-pole
  mean-square follower: `ms += k*(x*x - ms)`, RMS = `sqrt(ms)`, where `k` is a time-constant
  coefficient from the sample rate (mirror the `ScalarSmoother` coefficient form added in M1's
  `core.rs`: `k = 1/(tau_seconds*sample_rate)`, finite-clamped to [0,1]). Allocation-free, finite,
  bounded; not promoted to dsp-utils (resolved M1 decision; add an ADR-0003 candidate-extraction
  note).
- **Change:** Define `EnergyFollower { mean_square: f32, coefficient: f32 }` with
  `new(sample_rate)`, `reset()`, and `observe(&mut self, sample: f32) -> f32` returning the current
  RMS. Sanitize the input (`finite_or`/`snap_to_zero` per the crate's `math` helpers) so a non-finite
  sample cannot poison the state. No other files.
- **Verify:** New `#[cfg(test)]` tests in `energy_follower.rs`: (a) feeding a constant-amplitude
  signal of known RMS converges the follower to that RMS within tolerance; (b) `reset` returns it to
  zero; (c) a non-finite input leaves it finite; (d) `assert_no_allocations` (via the crate's
  re-export) over an `observe` loop. Red→green: `EnergyFollower` does not exist yet (compile-level
  red for new code).

### 2. Compute the effort signal into `ModulationSources`
- **File(s):** `plugins/lamath/src/dsp/voice/modulation_state.rs`.
- **Reference behavior:** [ADR-0014](docs/adr/0014-dynamic-response-effort-energy-bus.md) — effort is
  "player effort (velocity, pressure, breath, from `ExpressionStream`)." `ExpressionStream`
  (`crates/lindelion-plugin-shell/src/events.rs`) carries `velocity` and `pressure` (the breath
  proxy for wind); `ModulationState` already mirrors these into `self.velocity`/`self.aftertouch`
  via `apply_expression_values`. Effort is derived from those existing fields — **no shared-crate
  change**. The blend is provisional: M8 (force-dependent driver) owns its refinement, so keep it
  minimal here.
- **Change:** Add `effort: f32` to the `ModulationSources` struct and populate it at all three
  construction sites (`static_sources`, `next_sources`, and the inline `ModulationSources` in
  `next_lfo_sample`) as `effort = finite_clamp(self.velocity.max(self.aftertouch), 0.0, 1.0, 0.0)`.
  No change to `source_value` yet (that is step 5, decision-gated).
- **Verify:** New `#[cfg(test)]` test: set known velocity/pressure on a `ModulationState`, call
  `next_sources`, assert `effort == max(velocity, pressure)` (and that pressure swelling above
  velocity raises it). Red→green: the `effort` field does not exist yet.

### 3. Wire the measured-energy follower into the voice loop  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/voice/modulation_state.rs` (hold the follower, expose energy),
  `plugins/lamath/src/dsp/voice/mod.rs` (feed it the resonator output — incidental wiring for this
  source).
- **Reference behavior:** Reuse-map — "the measured-energy half sampled in the `resonator_stack`
  per-sample loop." The voice loop (`mod.rs:276`) computes `sources = modulation.next_sources(...)`
  *before* `resonator_output = self.resonators.process_sample(excitation)` (mod.rs:290). So the
  follower is updated from `resonator_output` *after* `process_sample`, and its value is read into
  the *next* sample's `ModulationSources` — a one-sample feedback delay, which is the correct and
  expected coupling (and inert until a consumer exists).
- **Change:** Add `energy_follower: EnergyFollower` and a last-`energy: f32` to `ModulationState`
  (construct in `new`; `reset`/`clear`/`trigger` reset the follower and zero `energy`). Add
  `observe_energy(&mut self, sample: f32)` that updates the follower and stores the RMS in
  `self.energy`. Add an `energy: f32` field to `ModulationSources`, set from `self.energy` at the
  three construction sites. In `process_sample_with_live_excitation`, call
  `self.modulation.observe_energy(resonator_output)` immediately after `process_sample` returns.
- **Verify:** New `#[cfg(test)]` test at the `ModulationState` level: starting from rest, assert
  `next_sources().energy` is ~0; then feed a known steady signal through `observe_energy` for enough
  samples and assert `next_sources().energy` tracks that signal's RMS within tolerance. Red→green:
  `observe_energy`/`energy` do not exist yet.

### 4. **[DECISION]** Effort/energy bus: user-routable or fixed internal wiring — **RESOLVED**
- *(Lifted verbatim from the M2 phase `[DECISION]`: "Whether the effort/energy bus is user-routable
  in the mod matrix or fixed internal wiring (taste + UI surface).")* **RESOLVED: fixed internal
  wiring.** The bus values live in `ModulationSources` (steps 1–3); they are **not** exposed as
  user-selectable `ModulationSource` variants. M4–M8 read `sources.effort` / `sources.energy`
  directly via each effect's own depth control. The `ModulationSource` enum, codec, registry, and
  VST3/UI surface stay unchanged. Step 5 implements only the fixed-wiring branch.

### 5. Keep the bus internal (fixed-wiring): make `effort`/`energy` defensibly live  [depends on #4]
- **File(s):** `plugins/lamath/src/dsp/voice/modulation_state.rs` only. No change to
  `patch.rs`, `parameters/codecs.rs`, `parameters/registry.rs`, or the UI — the `ModulationSource`
  enum and the four "Mod N Source" parameters stay exactly as they are.
- **Reference behavior:** `modulation_sum_from` only sums **enabled** mod slots over the existing
  `ModulationSource` variants, so a bus that is *not* an enum variant cannot be routed and cannot
  affect audio — exactly the "no audible change when depths are zero" guard. Because no M2 code reads
  `sources.effort`/`sources.energy` yet (the consumers arrive in M4–M8), the fields would otherwise
  trip `dead_code` under `-D warnings`.
- **Change:** Leave `source_value` and the `ModulationSource` enum untouched. Ensure the new
  `effort`/`energy` fields on `ModulationSources` do not break `-D warnings`: prefer reading them in
  the step-2/step-3 tests (a `#[cfg(test)]` read keeps them live in test builds) **plus** a minimal
  non-test reader if clippy still flags them — e.g. a `pub(super) fn effort_energy(&self) ->
  (f32, f32)` accessor on `ModulationState` returning `(self.energy, effort)` that M4 will call, or a
  targeted `#[allow(dead_code)]` with a `// consumed from M4 (ADR-0014)` note. Pick the lightest
  option that compiles clean; do not add a fake consumer that changes audio.
- **Verify:** Test asserting the user-facing source surface is unchanged — the `ModulationSource`
  codec still reports `max == 5` / six entries (red→green only if the branch adds such an invariant;
  otherwise this is a guard that would fail had the routable branch been taken, documenting the
  decision). The substantive behavioral coverage is the step-2 effort and step-3 energy tracking
  tests, which read the fields and prove the bus is computed.

### 6. Exit gate (make ci, no-alloc, zero-depth regression)
- **File(s):** verification only; if a dedicated audio-thread no-alloc check for the new path is not
  already covered, add it under `plugins/lamath/src/dsp/voice/tests/` or alongside the engine
  no-alloc test.
- **Reference behavior:** M2 exit — "`make ci` green; bus tracks a known effort/energy input within
  tolerance; no-alloc coverage; no audible change yet when depths are zero (regression guard)."
  [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) requires the new audio-thread work
  (`observe_energy` + the follower, run every sample) to carry no-alloc coverage.
- **Change:** Add `assert_no_allocations` coverage exercising a voice render that now runs the energy
  follower each sample (note-on + `render`/`process_sample` block). Confirm a default patch (no
  effort/energy routing) renders identically to pre-M2 — the regression guard — via the existing
  render battery staying green (the bus values are computed but unconsumed).
- **Verify:** Run `make ci` and show the output (green). The step-1–3 tracking tests satisfy "bus
  tracks a known input within tolerance"; the no-alloc test satisfies ADR-0001; the unchanged render
  battery satisfies "no audible change when depths are zero." Per the plan's doc-surface
  cross-cutting constraint, add one curated `CHANGELOG.md` line only if the chosen branch is
  user-visible (the user-routable branch adds two selectable mod sources; fixed-wiring is internal
  and may warrant no CHANGELOG line).

---

## Decision resolved

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 4 | Effort/energy bus user-routable in the mod matrix, or fixed internal wiring. | **RESOLVED: fixed internal wiring** — bus stays internal `ModulationSources` fields; M4–M8 read `effort`/`energy` directly via per-effect depth controls; `ModulationSource` enum and VST3/UI surface unchanged. |
