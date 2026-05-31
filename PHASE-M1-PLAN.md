# Phase M1 — Prepared-operator / control-rate refactor (execution steps)

Expanded from milestone **M1** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md). Scope is
the waveguide family only; `ModalBank` and `mesh_2d` are untouched. Run the steps in order, then
the companion execution prompt.

## What M1 actually changes

Today `String1d::process_sample`, `Tube1d::process_sample`, and `WaveguideBody::process_sample`
re-derive their full linear operator set **every sample** from a `WaveguideParams` value that is
already piecewise-constant (it only moves at control rate, behind the `resonator_stack` epsilon
gate in `configure_modulated` / `retune` / `set_waveguide_loop_gain`). The expensive derivations
are `core::loop_damping` (including the 96-point `measured_filter_peak` scan + biquad coeff
synthesis + group-/phase-delay probes), `dispersion::dispersion_profile`, `core::waveguide_geometry`,
`core::delay_tuning`, `TubeBoreProfile::from_params`, and `BodyProfile::from_params`.

M1 caches each leaf's derived model and recomputes it **only when its input params move**. Because
the cached recompute produces the identical arithmetic, constant-param renders are bit-for-bit
equivalent (the equivalence gate is therefore trivially within tolerance); only the *frequency* of
recompute drops from per-sample to control-rate. `mesh_2d` already derives at `configure` time and
is out of scope.

**Design choice (re-derived from the plan's Context/reuse map):** seat the cache at each leaf as a
params-keyed dirty-check (`Option<(InputParams, Prepared)>`, recompute when incoming `!=` cached).
This is the leaf-level realization of "the epsilon gate is the natural seat for the
`PreparedResonatorModel` cache": recompute frequency equals control rate, and it correctly covers
*all three* param-update paths (`configure`, `retune`, `set_waveguide_loop_gain`) without threading
a prepared model through `WaveguideResonator::process_sample`'s signature. The per-sample residue is
a ~9-field `f32` equality compare — negligible against the 96-point scan it replaces.

---

## Steps

### 1. Cache `BodyProfile` in `WaveguideBody`  [DECISION]
- **File(s):** `plugins/lamath/src/dsp/waveguide/body.rs`
- **Reference behavior:** `BodyProfile::from_params(sample_rate, params)` (body.rs:67) is a pure
  function of `sample_rate` + the `WaveguideParams` fields it reads (`style`, `frequency_hz`,
  `loop_filter_cutoff`, `boundary_reflection`). `process_sample` (body.rs:37) calls it and pushes
  the four coefficient sets into `highpass`/`lowpass`/`low_resonance`/`high_resonance` every sample;
  `set_coefficients` mutates coeffs only, never the biquad state, so skipping it when the profile is
  unchanged preserves filter memory and output exactly.
- **Change:** Add a cached `Option<(WaveguideParams, BodyProfile)>` (or the minimal `WaveguideParams`
  subset the profile reads) to `WaveguideBody`. In `process_sample`, recompute the profile and call
  the four `set_coefficients` only when the incoming params differ from the cached key; otherwise
  reuse the cached profile and skip the `set_coefficients` calls. Clear/repopulate the cache in
  `reset`. No change to the filtering math, gains, or `snap_to_zero` usage.
- **Verify:** New `#[cfg(test)]` test renders an impulse with **constant** params and asserts the
  profile is derived exactly once (expose a `#[cfg(test)]` recompute counter or a "prepared
  generation" accessor; **note** the `#[derive(PartialEq)]` on `WaveguideBody` — exclude the
  counter from equality via a hand-written/adjusted impl or a skipped field). Red before the cache
  (no such accessor → fails to compile; or counter == sample_count), green after (counter == 1).
  The existing `body.rs` tests (`body_renders_finite_decaying_impulse`, etc.) must stay green —
  that is the leaf equivalence guard.
- **[DECISION]** *(lifted from the M1 phase decision — it governs where every prepared struct in
  steps 1–3 is defined, so resolve it here first.)* Promote the prepared-model types (`BodyProfile`
  cache here, `PreparedStringModel`/`PreparedTubeModel` in steps 2–3) to `crates/lindelion-dsp-utils`
  now, or keep them local in Lamath. **Recommended: keep local** — single consumer today, and
  [ADR-0003](docs/adr/0003-shared-core-extraction.md) says single-consumer code that looks generic
  stays local with a candidate-extraction note. The energy follower's promotion is a separate M2
  decision; do not promote it here.

### 2. Cache the prepared string operators in `String1d`  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs`
- **Reference behavior:** `String1d::process_sample` (string_1d.rs:75) currently recomputes, every
  sample: `core::loop_damping`, `dispersion::dispersion_profile`, `core::waveguide_geometry`, and
  `core::delay_tuning` (whose `delay_offset_samples` folds in `damping.filter_delay_samples` and
  `dispersion_profile.delay_compensation_samples`), then derives `one_way_delay`, sets the
  termination coefficients (`damping.coefficients` + identity), and computes
  `endpoint_reflection_gain(damping.loop_gain)`. All inputs are fields of the incoming
  `String1dParams`/`WaveguideParams` (constant between control updates). `reflected_sample` consumes
  `dispersion_profile` + `reflection_gain` per side.
- **Change:** Define a local `PreparedStringModel` holding the per-sample-invariant derivations
  (`damping: LoopDamping`, `dispersion_profile: DispersionProfile`, `geometry: WaveguideGeometry`,
  `one_way_delay: f32`, `reflection_gain: f32`). Add `Option<(String1dParams, PreparedStringModel)>`
  to `String1d`; recompute (and re-`set_coefficients` on `terminations`) only when the incoming
  `String1dParams` differs from the cached key. `process_sample` reads the cached model for the
  boundary/pickup/reflection math; the `WaveguideBody` call still passes `waveguide_params` (the body
  caches independently from step 1). Repopulate/clear the cache in `reset`. No change to the
  traveling-wave push/excitation/dispersion math.
- **Verify:** New `#[cfg(test)]` test renders constant-param audio and asserts the prepared string
  model is derived once (recompute counter == 1), red→green as in step 1. Existing `string_1d.rs`
  tests (`string_1d_renders_finite_decaying_audio`, tuning-across-sample-rates, dispersion-tuning,
  non-finite recovery) stay green = leaf equivalence + tuning preservation.

### 3. Cache the prepared bore operators in `Tube1d`  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/waveguide/tube_1d.rs`
- **Reference behavior:** `Tube1d::process_sample` (tube_1d.rs:39) recomputes every sample:
  `core::loop_damping`, `TubeBoreProfile::from_params(sample_rate, params, damping.loop_gain)`,
  `core::waveguide_geometry`, the mouth and damping `core::filter_phase_delay_samples`, and
  `core::delay_tuning` (offset `1.0 + 0.5*(mouth_phase_delay + damping_phase_delay)`), then
  `one_way_delay`, and sets `boundary_filters` coefficients to (`profile.mouth_loss`,
  `damping.coefficients`). `reflected_sample` and the pickup mix consume `profile`. Note the tube
  forces `style = Tube` into its params before deriving.
- **Change:** Define a local `PreparedTubeModel` (`damping: LoopDamping`, `profile: TubeBoreProfile`,
  `geometry: WaveguideGeometry`, `one_way_delay: f32`). Add
  `Option<(WaveguideParams, PreparedTubeModel)>` to `Tube1d`, keyed on the style-normalized params;
  recompute (and re-`set_coefficients` on `boundary_filters`) only on change. `process_sample` reads
  the cached model; the `WaveguideBody` call is unchanged (body caches per step 1). Repopulate/clear
  in `reset`. No change to the reflection/excitation/pickup math.
- **Verify:** New `#[cfg(test)]` constant-param test asserts the prepared bore model is derived once
  (counter == 1), red→green. Existing `tube_1d.rs` tests (`tube_1d_renders_finite_decaying_audio`,
  `tube_1d_tuning_matches_requested_pitch_across_matrix`, boundary/non-finite tests) stay green.

### 4. Add init-converged scalar smoothing on the continuous physical inputs  [depends on #2, #3]
- **File(s):** `plugins/lamath/src/dsp/waveguide/string_1d.rs`,
  `plugins/lamath/src/dsp/waveguide/tube_1d.rs`
- **Reference behavior:** The plan sub-bullet: "Keep cheap scalar smoothing on the physical inputs
  so control-rate recompute does not zipper." Today params jump discretely at control boundaries; the
  cache (steps 1–3) preserves that. Smoothing turns a step into a short per-sample ramp of the
  cached key, so the prepared model (loop filter / gain / level / timbre) glides instead of stepping.
  Re-derived constraint: **do not smooth `frequency_hz` or the strike/pickup positions** — those set
  tuning and excitation-injection timing, which downstream milestones (e.g. M4 exit: "tuning
  unaffected") require to track the requested value exactly. Smooth only the continuous timbral/level
  scalars: `loop_gain`, `loop_filter_cutoff`, `loop_filter_resonance`, `dispersion` (string),
  `boundary_reflection` (tube).
- **Change:** Add one-pole scalar smoothers (reuse `OnePoleLowpass`/a cheap coefficient from
  `lindelion-dsp-utils`, or an inline one-pole) for the named scalars in each leaf, **initialized
  converged to the first params seen** (first `process_sample` after `reset` snaps state to target).
  Feed the smoothed scalars into the params used as the cache key. During a transition the key moves
  every sample, so the step-1–3 dirty-check recomputes per sample until it settles — expected and
  bounded.
- **Verify:** New `#[cfg(test)]` test feeds a mid-render step change in `loop_gain` (and/or
  `loop_filter_cutoff`) and asserts the effective/smoothed value (test accessor) approaches the new
  target over multiple samples rather than in one — red before smoothing (instant jump), green after.
  A second assertion renders **constant** params and asserts bit-identical output vs the step-1–3
  result (init-converged smoother must be inert at steady state), guarding the equivalence gate.

### 5. Add waveguide-path no-allocation coverage
- **File(s):** `plugins/lamath/src/dsp/waveguide/measurement_tests.rs` (or a new
  `#[cfg(test)]` module under `waveguide/`); uses the already-declared
  `lindelion-test-allocator` dev-dependency and the crate's `assert_no_allocations` re-export.
- **Reference behavior:** [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) requires every
  audio-thread path to carry no-alloc coverage. The existing engine no-alloc test
  (`engine/tests.rs:300`) exercises **Modal** resonators only; the refactored String/Tube
  `process_sample` path — now allocating a cache `Option` at construction, not on the audio thread —
  has none.
- **Change:** Add a test that constructs a `WaveguideResonator`, primes it once (first
  `process_sample` populates the caches outside the asserted region), then wraps a steady-state
  render block of both `WaveguideStyle::String` and `WaveguideStyle::Tube` in
  `assert_no_allocations("waveguide_render", || { ... })`. Include a block where a smoothed param is
  mid-transition to prove per-sample recompute is also alloc-free.
- **Verify:** Test is green (regression guard per ADR-0001; no behavioral red→green — the path was
  alloc-free before too, but was uncovered). Stated explicitly so the executor does not expect a
  behavioral failure first.

### 6. Equivalence + cost gate (close-out)
- **File(s):** none (verification only); if a tolerance-based equivalence assertion is wanted beyond
  the existing battery, add it to `plugins/lamath/src/dsp/comparison_tests.rs`.
- **Reference behavior:** M1 exit criteria — `make ci` green; output equivalence to pre-refactor
  within tolerance (RMS + per-harmonic decay) across the existing render battery; `make bench` shows
  reduced per-sample cost.
- **Change:** None beyond optional test. The existing render battery
  (`comparison_tests::ab_render_metrics_compare_waveguide_styles_against_modal_presets`, the
  `waveguide.rs`/`string_1d.rs`/`tube_1d.rs`/`body.rs` render tests) is the equivalence battery;
  Design A's memoized arithmetic makes it bit-identical, so it stays green unchanged.
- **Verify:** Run `make ci` (green). Run `make bench` and confirm `waveguide/string_512` and
  `waveguide/tube_512` drop materially vs the pre-M1 baseline (capture the baseline numbers before
  step 1, or from `git stash`, for the comparison). Confirm the step-5 no-alloc test runs in `make
  ci`. Record the bench delta in the CHANGELOG/architecture doc update that lands with the milestone
  (per the plan's doc-surface cross-cutting constraint).

---

## Decisions surfaced for you

| Step | Decision | Resolution |
| ---- | -------- | ---------- |
| 1 | Promote prepared-model types to `lindelion-dsp-utils` now, or keep local in Lamath. | **RESOLVED: keep local** in `plugins/lamath/src/dsp/waveguide/*` with a candidate-extraction note ([ADR-0003](docs/adr/0003-shared-core-extraction.md)). |
