# Phase M3 — 2× oversampled nonlinear inner-loop harness (execution steps)

Expanded from milestone **M3** of [DYNAMIC-RESPONSE-PLAN.md](DYNAMIC-RESPONSE-PLAN.md)
([depends on M1], landed). Build the shared substrate every later nonlinear stage runs on: a
reusable, fixed-buffer, allocation-free 2× oversampling wrapper around the waveguide-family
resonator core, identity-equivalent within filter tolerance at zero nonlinearity, with a small
reported latency.

## What M3 builds (and the seam)

Per [ADR-0016](docs/adr/0016-oversampled-nonlinear-inner-loop.md): run the nonlinear resonator inner
loop at **2× the host sample rate**, wrapped in a reusable allocation-free stage with half-band
up/down filters, identity-equivalent within filter tolerance when nonlinearity is zero, from the
start. The half-band filters add a **small fixed latency that must be reported** so hosts compensate.

**Scope — waveguide family only.** The plan is emphatic: "Every milestone applies to the
waveguides," and "The Modal resonator stays as-is … no refactor." So oversampling wraps the
**String/Tube/Mesh** cores; **`ModalBank` keeps running at the host rate, untouched.**

**Seam — inside `ResonatorEngine`** (`plugins/lamath/src/dsp/voice/resonator_stack.rs`).
`ResonatorEngine` already owns `modal`, `waveguide` (+`waveguide_params`), and `mesh`, dispatched by
`kind` in `process_sample`. M3 builds the **waveguide/mesh** sub-resonators at `2*sample_rate`,
leaves `modal` at `sample_rate`, and routes only the waveguide/mesh branches through an
`Oversampler2x`. Everything above the engine — the `ResonatorStack` routing, series conditioner,
body-color exciter, structural-transition crossfade, modulation, excitation, energy follower, and
output stage — stays at host rate and is unchanged. This makes modal exactly byte-identical (so the
`render_modal_response` equivalence battery is untouched) and avoids any structural-cadence rework.

**Location (no new decision):** the half-band filter and `Oversampler2x` are kept **local in
Lamath** with a candidate-extraction note, consistent with the resolved M1/M2 promotion calls and
[ADR-0003](docs/adr/0003-shared-core-extraction.md)'s single-consumer default. (The plan tags no M3
`[DECISION]`, and the ModalBank-vs-global question is resolved by "every milestone applies to the
waveguides" — so none is surfaced.)

## Equivalence note

The existing waveguide battery (`comparison_tests.rs`, `waveguide.rs`/`string_1d.rs`/`tube_1d.rs`
render tests) calls `WaveguideResonator`/`ModalBank` **directly**, not through `ResonatorEngine`, so
it is unaffected by the engine-level oversampling and stays green. M3's identity-equivalence claim is
therefore proven by a **new** test that compares the same `WaveguideResonator` rendered through the
`Oversampler2x` at 2× against the bare resonator at 1×, within M1's tolerance shape
(`gain_fitted_rms_difference` + per-harmonic decay from `render_metrics`/`analysis`).

---

## Steps

### 1. Add a half-band FIR filter primitive
- **File(s):** new `plugins/lamath/src/dsp/voice/oversampler.rs`; register `mod oversampler;` in
  `plugins/lamath/src/dsp/voice/mod.rs`.
- **Reference behavior:** [ADR-0016](docs/adr/0016-oversampled-nonlinear-inner-loop.md) — "half-band
  up/down filters," fixed-size, allocation-free, "small fixed latency." Re-derive the canonical 2×
  half-band: a symmetric linear-phase windowed-sinc low-pass with cutoff at quarter sample rate
  (`Fs/4`); the sinc zeros land on the even off-center taps, giving the half-band structure. Length
  `L = 2*HALF + 1` (pick a concrete `HALF`, e.g. 11–15, for a small latency; document it as tunable
  per ADR-0016 "revisit only if a stage's aliasing test fails at 2×"). Group delay is `HALF` samples
  at the filter's clock. Coefficients computed once at construction into a fixed `[f32; L]`
  (allocation-free); window with Hamming/Blackman; normalize unity DC gain.
- **Change:** Define a `HalfbandFir` holding the fixed coefficient array and a fixed ring/state
  buffer, with `new()`, `reset()`, and a `process(sample) -> f32` (single-rate convolution) — or the
  two polyphase entry points the `Oversampler2x` needs (see step 2), whichever is simpler to test in
  isolation. No other files besides the module registration.
- **Verify:** New `#[cfg(test)]` tests: DC/passband gain is unity (a constant in → same constant out
  after fill); a half-Nyquist-and-above tone is attenuated (stop-band); `reset` zeros the state;
  `assert_no_allocations` over a `process` loop. Red→green: `HalfbandFir` does not exist yet
  (compile-level red for new code).

### 2. Add the allocation-free `Oversampler2x` wrapper  [depends on #1]
- **File(s):** `plugins/lamath/src/dsp/voice/oversampler.rs`.
- **Reference behavior:** [ADR-0016](docs/adr/0016-oversampled-nonlinear-inner-loop.md) — a reusable
  2× stage wrapping the core: upsample one host sample to two (zero-stuff + half-band interpolation,
  ×2 gain to preserve level), run the core twice at 2×, downsample the two core outputs to one host
  sample (half-band decimation + take-one). Fixed buffers sized at construction; no audio-thread
  allocation. Total path latency referred to host rate is the up-filter + down-filter group delay
  (`≈ HALF` host samples for matched half-bands) — expose it as a constant for step 4.
- **Change:** Define `Oversampler2x` owning the interpolation and decimation `HalfbandFir`s and a
  fixed 2-sample scratch, with `new()`, `reset()`, `process(&mut self, input: f32, core: impl
  FnMut(f32) -> f32) -> f32`, and a `pub(crate) const LATENCY_SAMPLES: u32` (host-rate). No new
  dependencies.
- **Verify:** New `#[cfg(test)]` tests: with an **identity** core (`|x| x`), a held input reproduces
  itself within filter tolerance after the reported latency (transparency); with a linear one-pole
  core, oversampled output matches the same one-pole run at base rate within tolerance; `reset`
  clears; `assert_no_allocations` over a `process` loop. Red→green: `Oversampler2x` does not exist
  yet.

### 3. Run the waveguide/mesh engine cores at 2× through the oversampler  [depends on #2]
- **File(s):** `plugins/lamath/src/dsp/voice/resonator_stack.rs` (the `ResonatorEngine` struct,
  `new`, `process_sample`).
- **Reference behavior:** `ResonatorEngine::new(sample_rate)` (resonator_stack.rs ~265) builds
  `modal`, `waveguide`, and `mesh` all at `sample_rate`; `process_sample` (resonator_stack.rs ~353)
  matches `kind` and calls the sub-resonator directly. The waveguide/mesh cores tune from the
  sample rate they were built with (Hz-based: `core::loop_damping`/`delay_tuning`,
  `MeshResonator::configure`), so building them at `2*sample_rate` keeps pitch and decay correct
  while doubling their internal clock. `ModalBank` must stay at `sample_rate` (untouched reference;
  plan §"The Modal resonator stays as-is"). `configure`/`retune`/`set_waveguide_loop_gain` pass Hz
  and ratios, so they are rate-agnostic and need no change.
- **Change:** Build `waveguide` and `mesh` at `2.0 * sample_rate` in `ResonatorEngine::new`
  (leave `modal` at `sample_rate`); add an `oversampler: Oversampler2x` field (`new()` in the
  constructor; `reset()` wherever the engine clears, i.e. `clear`). In `process_sample`, route the
  `Waveguide` and `Mesh` arms through `self.oversampler.process(input, |x| …core…)` while `Modal` and
  `Silent` stay as-is. (Disjoint-field borrow of `self.oversampler` vs the closure's
  `self.waveguide`/`self.mesh` is the one mechanic to get right — split the borrow with a small
  helper if the checker objects; do not change behavior.)
- **Verify:** New `#[cfg(test)]` equivalence test (in `resonator_stack.rs` or a waveguide test
  module): render a `WaveguideResonator` (String, then Tube) through an `Oversampler2x` at 2× and the
  same params/excitation through the bare resonator at 1×; assert the tails match within M1's
  tolerance shape — `gain_fitted_rms_difference` normalized small AND per-harmonic decay close
  (reuse `render_metrics`/`analysis`). This is the "identity-equivalent within filter tolerance vs
  M1 output" gate. Add `assert_no_allocations` over an engine render of a waveguide patch. Red→green:
  before the change the oversampled path does not exist; after, equivalence holds and no allocation
  occurs. Existing engine/voice render tests (loose finite/decaying/no-alloc) must stay green.

### 4. Report the oversampler latency to the host  [depends on #2]
- **File(s):** `plugins/lamath/src/vst3_entry/processor.rs` (`getLatencySamples`), plus whatever
  minimal re-export makes `Oversampler2x::LATENCY_SAMPLES` reachable from `vst3_entry` (e.g. a
  `pub(crate)` const surfaced through the dsp module).
- **Reference behavior:** [ADR-0016](docs/adr/0016-oversampled-nonlinear-inner-loop.md) — "The
  half-band filters add a small fixed latency that must be reported in the plugin's latency so hosts
  compensate." `getLatencySamples` (processor.rs:287) currently returns `0`. The added latency is the
  constant host-rate group delay from step 2; it is the same for every waveguide voice and `0` for
  modal-only voices, so the plugin reports the constant (the maximum a voice can introduce).
- **Change:** Return `Oversampler2x::LATENCY_SAMPLES` (the host-rate constant) from
  `getLatencySamples` instead of `0`.
- **Verify:** New `#[cfg(test)]` test asserting `getLatencySamples` (or the underlying constant
  surfaced for test) equals the oversampler's reported latency and is non-zero. Red→green: it returns
  `0` before, the reported constant after.

### 5. Quantify the ~2× core cost with a bench
- **File(s):** `plugins/lamath/benches/waveguide.rs` (extend), and `[[bench]]` wiring in
  `plugins/lamath/Cargo.toml` only if a new bench target is added (prefer extending the existing
  `waveguide` group).
- **Reference behavior:** M3 exit — "`make bench` quantifies the ~2× core cost." The existing
  `waveguide/string_512` and `waveguide/tube_512` benches drive `WaveguideResonator::process_sample`
  at base rate. The oversampled cost is two core calls plus the half-band up/down filters per host
  sample.
- **Change:** Add oversampled variants (e.g. `string_512_2x`, `tube_512_2x`) that drive the same
  resonator through an `Oversampler2x`, so the bench reports base-rate vs oversampled side by side.
  Keep the existing base-rate benches for comparison.
- **Verify:** `make bench` (or `cargo bench -p lamath --bench waveguide`) runs and reports the new
  oversampled timings at roughly 2×+ the base-rate ones; record the figure for the milestone's doc
  update. (Bench targets compile-check under `make ci`'s `bench-smoke`.)

### 6. Exit gate
- **File(s):** verification only; doc-surface update per the plan's cross-cutting constraint.
- **Reference behavior:** M3 exit — "`make ci` green; identity-equivalent (within filter tolerance)
  at zero nonlinearity vs M1 output; no-alloc coverage; latency reported; `make bench` quantifies the
  ~2× core cost."
- **Change:** None beyond verification. Add one curated `CHANGELOG.md` line (the reported latency is
  user-visible — hosts now see and compensate a small plugin latency) and, if `architecture.md`
  asserts the resonator runs at host rate anywhere, update that current-state assertion.
- **Verify:** Run `make ci` and show the output (green). The step-3 equivalence + no-alloc tests, the
  step-4 latency test, and the step-5 bench together satisfy every exit clause.

---

## No decision surfaced

M3 carries no `[DECISION]`: the ModalBank-vs-global-oversampling question is resolved by the plan's
"every milestone applies to the waveguides" + "Modal resonator stays as-is" (→ oversample
waveguide/mesh only), and the local-vs-shared location follows the resolved M1/M2 pattern and
ADR-0003 (→ keep local). If the step-3 equivalence test cannot be met within M1's tolerance at the
chosen half-band length, that is a filter-tuning task inside M3 (raise `HALF`/adjust the window),
not a semantics change — surface it before altering the equivalence target.
