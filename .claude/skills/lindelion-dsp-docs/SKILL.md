---
name: lindelion-dsp-docs
description: Use when documenting any DSP module in the Lindelion workspace — filters, resonators, oscillators, delays, envelopes, smoothers, onset/pitch detectors, PSOLA, analysis helpers. Encodes industry DSP doc conventions (signal flow, transfer function, frequency/impulse plots, realtime contract) together with Lindelion's existing patterns (no-alloc audio thread, `snap_to_zero`, `assert_all_finite`, synthetic-only test signals, 600-line file cap, `make ci`).
---

# Lindelion DSP Documentation

Prescribes the structure, math, diagrams, plots, and realtime callouts for documenting DSP code in this workspace. Built from JUCE / Faust / Julius O. Smith / Cytomic / RBJ conventions and tightened against Lindelion's own no-alloc, finite-output, synthetic-fixture contract.

## When this skill applies

Use when:

- Writing or substantially editing docs for a DSP module in `crates/lindelion-dsp-utils/`, `crates/lindelion-onset-detect/`, `crates/lindelion-psola/`, `crates/lindelion-pitch-detect/`, or any `plugins/*/src/dsp/`.
- Adding a new public DSP type (filter, resonator, oscillator, detector, transform).
- Extending `docs/plugins/<plugin>.md` with a new DSP section.
- A reviewer asks "what does this filter do?", "is this allocation-safe?", "what's the frequency response?", or "where does this formula come from?".

Do NOT use for non-DSP code (plugin shell, VST3 plumbing, parameter glue, UI), comment cleanups, or trivial renames.

## The skeleton

Every DSP module doc uses these sections, in this order. Omit a section only when it does not apply (a one-pole smoother has no useful pole-zero plot; a peak detector has no transfer function).

### 1. Purpose

One sentence. Name the technique, topology, and originator if standard.

> "Direct-Form II Transposed biquad with RBJ Audio EQ Cookbook coefficients (low/high/band/peak/shelf)."
> "Vadim Zavalishin TPT state-variable filter with trapezoidal integrators."
> "Modal resonator bank — N parallel second-order resonant filters, one per vibrational mode."

### 2. Theory

In order, each only when it adds information:

- **Difference equation** in `y[n] = …` form, using the Smith/scipy sign convention `y[n] = b0·x[n] + … − a1·y[n-1] − …`. Spell out the sign convention if there is any ambiguity.
- **Transfer function** `H(z) = N(z) / D(z)` for LTI systems.
- **Analog prototype** plus **discretization method**: BLT with prewarp `g = tan(π·fc/fs)`, matched-z, impulse invariance, TPT/ZDF, forward Euler. State which.
- **Stability** condition (pole radius < 1, loop-gain product < 1, etc.) and **causality** notes.
- **Valid parameter range**, including the Nyquist clamp.

Math goes in `$…$` (inline) and `$$…$$` (display). GitHub renders MathJax in Markdown. Do not put LaTeX in Rust docstrings — rustdoc does not render it. Use Unicode (ω, π, ∑, ζ, ω₀) inline in code comments.

### 3. Algorithm

Either runnable pseudocode or a signal-flow diagram. Variable names match the Rust code (`z1`, `z2`, `coefficient`, `frequency_hz`, `phase`, `loop_gain`). If pseudocode, keep it short and one-pass — the goal is to make the diagram-or-code map directly to the implementation file.

### 4. Parameters

Markdown table: name, type, units, range, default, smoothing or clamp behavior. Pull ranges from the actual `FloatRange` definition or constructor clamps in the code (e.g., `constants.rs`) — do not invent them. Always include units; never bare numbers.

```
| Name           | Type | Units   | Range      | Default | Notes                              |
| -------------- | ---- | ------- | ---------- | ------- | ---------------------------------- |
| `cutoff_hz`    | f32  | Hz      | 20–20000   | 1000    | Clamped to `[20, fs/2 · 0.49]`     |
| `q`            | f32  | none    | 0.5–24     | 0.707   | Butterworth ≈ 0.707                |
```

### 5. Response plots

Always include for any LTI system:

- **Magnitude response** in dB on a log-frequency axis.

Strongly recommended:

- **Phase response** (degrees, shared x-axis).
- **Pole-zero plot** for IIR systems — shows stability margin.
- **Impulse response** for FIR / comb / reverb / resonator.

Topic-specific:

- **Group delay** — allpass, phaser, linear-phase FIR.
- **Step response** — env follower, smoother, slew limiter.
- **Spectrogram** — analyzer, PSOLA, pitch tracker, vocoder.
- **Parameter sweep** — one plot per varied param when Q / resonance / decay matter.
- **Aliasing / noise floor** — oscillator, nonlinearity, oversampler.

See the **Plots** section below for the production workflow.

### 6. Realtime contract

A bullet list. Cover, in order:

- **Allocation.** State explicitly: "Allocation-free; all state lives in `Self`." If `prepare()` allocates, say what and when.
- **Denormals.** Whether the module calls `lindelion_dsp_utils::math::snap_to_zero` on its filter state, and on which paths.
- **Reset.** What `reset()` clears. What `prepare(sample_rate, max_block)` does. Whether sample-rate changes invalidate state.
- **Thread safety.** Which methods are realtime-callable, which are not (parameter setters, etc.).
- **Bounded work.** Hard caps (e.g., `MAX_MODE_COUNT = 256`, max polyphony, max block size).
- **Finite output.** Confirm tests use `analysis::assert_all_finite` or equivalent over a parameter sweep.
- **SIMD / vectorization.** Scalar by default; if hand-tuned, name the architecture and intrinsic family.

### 7. Test coverage

Cross-link the tests that pin behavior. Lindelion has a strong convention here — use it.

- **No-alloc tests** wrapping audio-thread paths with `assert_no_allocations!` from `lindelion-test-allocator`. Pre-allocate output buffers outside the closure. Label sections (`"note_on"`, `"render_replace"`, `"voice_stealing_note_on"`).
- **Frequency-response tests** using `analysis::dft_magnitude_at` and `analysis::spectral_centroid_hz`.
- **Impulse-ring tests** — feed `δ[n]`, measure the dominant decay frequency, assert it matches the configured fundamental within tolerance.
- **Parameter-sweep finiteness** — `*_stays_finite_across_parameter_sweep` over a representative grid (e.g., `[40, 110, 440, 1760, 4000] Hz`).
- **Detection-quality tests** — for analyzers, follow the `plugins/glirdir/src/detection_quality_tests.rs` pattern: synthesize tones with `tone()`, noise with a seeded LCG, build `PitchContour` fixtures, assert cents tolerance with `assert_close_cents`.

Reference each test by full path (`lamath::dsp::engine::tests::note_on_and_render_do_not_allocate`) so it links from `docs/performance.md`.

### 8. Usage example

A minimal compileable Rust snippet using the module's actual public API. No pseudo-API.

### 9. References

External: RBJ Audio EQ Cookbook, Julius O. Smith CCRMA chapter, Cytomic technical paper, original DAFx / ICMC / IEEE paper. Cite by URL.

Internal: link to `docs/architecture.md`, `docs/performance.md`, related plugin docs, and the source file under `crates/` or `plugins/`.

## Diagrams — how to produce them

DSP block diagrams use a small standard symbol set: summing junction (circle with `+`, minus inputs labelled), gain triangle (label inside, point downstream), unit-delay box (`z⁻¹`), multiplier (`⊗`), branch point. Pick the tool by audience:

**Mermaid for high-level signal flow.** Renders inline on GitHub, fully text-diffable. Use for module-to-module flow and pipeline overviews. Standard symbols are approximated with Unicode.

````markdown
```mermaid
flowchart LR
    X((x[n])) --> S(("+"))
    S --> Y((y[n]))
    Y --> D["z⁻¹"] --> G["× a₁"] --> S
```
````

**Hand-authored SVG for textbook block diagrams.** Commit under `docs/diagrams/<module>.svg`. Use real adder circles, gain triangles, and `z⁻¹` boxes. Worth the effort for one or two hero diagrams per plugin (the canonical biquad direct-form, the modal bank topology, the waveguide loop).

**ASCII art for inline Rust doc comments.** When an SVG can't render inside `///` docs:

```text
x[n] ──►(+)──┬──► y[n]
        ▲    │
        │    ▼
        └──[z⁻¹]──[× a₁]
```

Avoid `draw.io` / `excalidraw` round-trip files — their embedded JSON produces unreadable diffs. Reserve TikZ for the rare case where Mermaid and hand-SVG are both insufficient; do not add `texlive-*` to `make ci`.

## Plots — how to produce them

The recommended workflow keeps plot data inside the Rust test suite (so it cannot drift from the implementation) and renders SVGs in a separate `make docs` target (so `make ci` stays fast and Python-free for normal contributors).

**Pipeline**:

1. **Rust test emits CSV.** Add a test in the same crate that runs the DSP under a deterministic stimulus (impulse, log sweep, unit step, seeded sine) and writes the result to `docs/plots/data/<module>_<plot>.csv`. Use `StdRng::seed_from_u64(…)` for any randomness — never `OsRng` or wall-clock.
2. **Round values.** Format with `{:.6}` so platform floating-point variance does not churn the file.
3. **Commit the CSV.** It is both the plot's source of truth and a golden snapshot — diffs in PR review immediately reveal accidental DSP drift.
4. **`make ci`** runs the export tests and `git diff --exit-code docs/plots/data/`. Drift fails the build with no Python involved.
5. **`make docs`** (new target, opt-in) regenerates SVGs from CSVs via `python3 tools/dsp-plot/plot_*.py`. Pin matplotlib/scipy/librosa in `tools/dsp-plot/requirements.txt`. Run locally or on the docs-publish job, never on the hot CI path.

**Plot recipes**:

- **Frequency response** — Rust test runs an impulse through the module, FFTs the response, writes CSV `freq_hz,mag_db,phase_deg`. Python uses `semilogx` for magnitude and phase on a shared x-axis.
- **Pole-zero** — Rust test exports coefficients as CSV `b,a`. Python uses `scipy.signal.tf2zpk(b, a)` and scatters poles (`x`) / zeros (`o`) on the unit circle.
- **Impulse / step** — Rust runs `δ[n]` or `u[n]` through the module, writes CSV `time_s,value`. Python plots; use `set_yscale("log")` with a `np.abs(y) + 1e-12` floor for decay tails.
- **Spectrogram** — Rust synthesizes a seeded test signal (or, for analyzers, runs the pipeline on a synthetic input). Python uses `librosa.display.specshow` on `librosa.stft` for log-y; fall back to `matplotlib.pyplot.specgram` for quick-look.

If the doc author needs a design sketch before any Rust code exists, `scipy.signal.freqz(b, a)` from coefficients alone is acceptable. The canonical plot, the one that ships in the doc, must come from the Rust path.

Suggested directory layout (introduce only when the first plot needs it):

```
docs/
  diagrams/
    biquad-direct-form-1.svg
    modal-bank-topology.svg
  plots/
    lamath-resonator-freqz.svg
    lamath-resonator-pz.svg
    data/
      lamath-resonator-freqz.csv
      lamath-resonator-ba.csv
tools/
  dsp-plot/
    requirements.txt
    plot_freqz.py
    plot_pz.py
    plot_impulse.py
    plot_spectrogram.py
```

## Notation conventions

- Signals: `x[n]` (input), `y[n]` (output) — square brackets, discrete time.
- Sample rate: `fs` (or `f_s`). Period `T = 1/fs`. Nyquist `fs/2`.
- Frequency: prefer Hz with explicit `fs`. If using normalized frequency, **state which convention** — `rad/sample [0, π]`, `cycles/sample [0, 0.5]`, or MATLAB-style `[0, 1]` (Nyquist ≡ 1). Mixing these without saying is the most common DSP-doc bug.
- Transfer function: `H(z)`, `H(s)` for the analog prototype, `H(e^{jω})` for the frequency response.
- Coefficients: `b_k` feedforward, `a_k` feedback; `a0 = 1` after normalization. Use the `y[n] = b0·x[n] + … − a1·y[n-1] − …` sign convention.
- Levels: dB for ratios (`20·log10` amplitude, `10·log10` power); dBFS for absolute level (±1.0 = 0 dBFS, with the caveat that float has no hard clip).
- Units required in every parameter table — `seconds`, `ms`, `Hz`, `dB`, `0..1`, `octaves`. Never bare numbers.
- Q is dimensionless; bandwidth in octaves or Hz (state which); shelf slope `S` per the RBJ Cookbook.

## Lindelion realtime checklist

Before merging any DSP doc, confirm:

- [ ] Audio-thread methods are covered by a test wrapped in `assert_no_allocations!` from `lindelion-test-allocator`.
- [ ] Filter state is flushed with `snap_to_zero` to suppress denormals.
- [ ] Inputs are clamped with `finite_clamp` or validated at construction; parameter ranges respect `fs/2`.
- [ ] Output is validated with `assert_all_finite` over a parameter sweep.
- [ ] Hard caps on per-sample iteration are stated explicitly (e.g., `MAX_MODE_COUNT = 256`).
- [ ] No external audio fixtures — test signals are synthesized via `tone()` and seeded-LCG noise.
- [ ] `make ci` passes (rustfmt, clippy `-D warnings`, file-size lint ≤ 600 lines, all tests). Do not bypass.
- [ ] If plots were generated, the CSVs under `docs/plots/data/` are committed and `make ci`'s diff check passes.

## Anti-patterns

- **Mixing normalized-frequency conventions** without naming which one.
- **Using dBFS for filter gain** — dBFS is an absolute level; filter gain is a ratio (dB).
- **Naming coefficients without naming the topology** — Direct-Form I, II, Transposed-II, SVF, TPT, lattice all share `H(z)` but have different numerical and modulation behavior.
- **BLT designs with no prewarp callout** — `g = tan(π·fc/fs)` is required, not optional.
- **"Realtime safe"** as an unqualified claim — say which methods are RT-callable and which aren't.
- **Black-box block diagrams** — unlabeled edges, non-standard symbols, missing gains.
- **Plotting only DC and Nyquist** — show intermediate frequencies and the resonance peak.
- **External fixture audio files** — Lindelion synthesizes test signals.
- **LaTeX in rustdoc** (`$…$` does not render). Put math in `docs/`, use Unicode inline in code.
- **Allocating in `process()`** — and don't pretend an unmeasured allocation isn't there. Wrap it in `assert_no_allocations!`.
- **Promising stability without naming the bound** — feedback delays, resonant filters, and PLLs all diverge for some parameter range.

## References

External:

- [Audio EQ Cookbook (Bristow-Johnson)](https://www.w3.org/TR/audio-eq-cookbook/)
- [Julius O. Smith — Introduction to Digital Filters](https://ccrma.stanford.edu/~jos/filters/)
- [Cytomic technical papers (Andy Simper)](https://cytomic.com/technical-papers/)
- [JUCE `dsp::` API reference](https://docs.juce.com/master/group__juce__dsp.html)
- [Faust libraries](https://faustlibraries.grame.fr/)
- [scipy.signal reference](https://docs.scipy.org/doc/scipy/reference/signal.html)
- [Ross Bencina — Real-time audio programming 101](http://www.rossbencina.com/code/real-time-audio-programming-101-time-waits-for-nothing)
- [GitHub Markdown math (MathJax)](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/writing-mathematical-expressions)

Internal:

- `docs/architecture.md` — workspace structure and realtime principles.
- `docs/performance.md` — the canonical realtime contract.
- `crates/lindelion-test-allocator/src/lib.rs` — `assert_no_allocations!`.
- `crates/lindelion-dsp-utils/src/math.rs` — `snap_to_zero`, `finite_clamp`, `finite_or`.
- `crates/lindelion-dsp-utils/src/analysis.rs` — `assert_all_finite`, `dft_magnitude_at`, `spectral_centroid_hz`.
- `plugins/glirdir/src/detection_quality_tests.rs` — canonical synthetic-fixture pattern.
- `plugins/lamath/src/dsp/engine.rs` — canonical no-alloc test pattern (`note_on_and_render_do_not_allocate`).
