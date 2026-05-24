# DSP doc skeleton

Use this nine-section template for every DSP-module doc. Omit a section only when it does not apply (a smoother has no useful pole-zero plot; a peak detector has no transfer function). Write each section in imperative voice; prefer concrete code references to prose.

## 1. Purpose

One sentence. Name the technique, the topology, and the originator if it is a standard form.

Examples:
- "Direct-Form II Transposed biquad with RBJ Audio EQ Cookbook coefficients (low/high/band/peak/shelf)."
- "Vadim Zavalishin TPT state-variable filter with trapezoidal integrators."
- "Modal resonator bank — N parallel second-order resonant filters, one per vibrational mode."

## 2. Theory

In order, omit any that do not add information:

- **Difference equation** in `y[n] = …` form, using the Smith/scipy sign convention `y[n] = b0·x[n] + … − a1·y[n-1] − …`.
- **Transfer function** `H(z) = N(z) / D(z)` for LTI systems.
- **Analog prototype** plus **discretization method**: BLT with prewarp `g = tan(π·fc/fs)`, matched-z, impulse invariance, TPT/ZDF, forward Euler. State which.
- **Stability** condition (pole radius < 1, loop-gain product < 1, etc.) and **causality** notes.
- **Valid parameter range**, including the Nyquist clamp.

Math goes in `$…$` (inline) and `$$…$$` (display). GitHub renders MathJax in Markdown.

## 3. Algorithm

Either runnable pseudocode or a signal-flow diagram (see `diagrams.md` for recipes). Variable names match the Rust code (`z1`, `z2`, `coefficient`, `frequency_hz`, `phase`, `loop_gain`). Keep it one-pass — the goal is direct correspondence with the implementation file.

## 4. Parameters

Markdown table: name, type, units, range, default, smoothing or clamp behavior. Pull ranges from `FloatRange` or constructor clamps in the actual source.

```
| Name           | Type | Units   | Range      | Default | Notes                              |
| -------------- | ---- | ------- | ---------- | ------- | ---------------------------------- |
| `cutoff_hz`    | f32  | Hz      | 20–20000   | 1000    | Clamped to `[20, fs · 0.49]`       |
| `q`            | f32  | none    | 0.5–24     | 0.707   | Butterworth ≈ 0.707                |
```

## 5. Response plots

Always include, for any LTI system:
- Magnitude response in dB on a log-frequency axis.

Strongly recommended:
- Phase response (degrees, shared x-axis).
- Pole-zero plot for IIR.
- Impulse response for FIR / comb / reverb / resonator.

Topic-specific:
- Group delay — allpass, phaser, linear-phase FIR.
- Step response — envelope follower, smoother, slew limiter.
- Spectrogram — analyzer, PSOLA, pitch tracker, vocoder.
- Parameter sweep — one plot per varied param when Q / resonance / decay matter.
- Aliasing / noise-floor — oscillator, nonlinearity, oversampler.

See `plots.md` for the Rust → CSV → matplotlib workflow.

## 6. Realtime contract

A bullet list. Cover, in order:

- **Allocation.** State explicitly: "Allocation-free; all state lives in `Self`." If `prepare()` allocates, name what and when.
- **Denormals.** Whether the module calls `lindelion_dsp_utils::math::snap_to_zero` on its filter state, and on which paths.
- **Reset.** What `reset()` clears. What `prepare(sample_rate, max_block)` does. Whether sample-rate changes invalidate state.
- **Thread safety.** Which methods are realtime-callable; which are not.
- **Bounded work.** Hard caps (e.g., `MAX_MODE_COUNT = 256`, max polyphony, max block size).
- **Finite output.** Confirm `analysis::assert_all_finite` covers it over a parameter sweep.
- **SIMD.** Scalar by default; if hand-tuned, name the architecture and intrinsic family.

## 7. Test coverage

Cross-link the tests that pin behavior. Use full module paths.

- No-alloc tests wrapping audio-thread paths with `assert_no_allocations!`. Pre-allocate output buffers outside the closure. Label sections.
- Frequency-response tests using `analysis::dft_magnitude_at` and `analysis::spectral_centroid_hz`.
- Impulse-ring tests — feed `δ[n]`, measure dominant decay frequency, assert it matches the configured fundamental within tolerance.
- Parameter-sweep finiteness — `*_stays_finite_across_parameter_sweep`.
- Detection-quality tests for analyzers — follow `plugins/glirdir/src/detection_quality_tests.rs`: synthesize tones with `tone()`, noise with a seeded LCG, build `PitchContour` fixtures, assert cents tolerance with `assert_close_cents`.

## 8. Usage example

A minimal compileable Rust snippet using the module's actual public API. Not pseudo-API.

## 9. References

External: RBJ Audio EQ Cookbook, Julius O. Smith CCRMA chapter, Cytomic technical paper, original DAFx / ICMC / IEEE paper. Cite by URL.

Internal: link to `docs/architecture.md`, `docs/performance.md`, related plugin docs, and the source file under `crates/` or `plugins/`.
