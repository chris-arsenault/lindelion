---
name: lindelion-dsp-docs
description: Use whenever documenting any DSP module in Lindelion — filters, resonators, oscillators, delays, envelopes, smoothers, onset/pitch detectors, PSOLA, analysis routines. Triggers on phrases like "document the resonator", "write docs for the filter", "add a perf section to the modal bank", "audit DSP docs", or any documentation edit under `crates/lindelion-*` or `plugins/*/src/dsp/`. Applies both industry DSP-doc conventions (block diagrams, transfer functions, plots, math notation) and Lindelion's allocation-free realtime contract.
---

# Lindelion DSP Module Documentation

This skill produces documentation for DSP modules in the Lindelion workspace. It combines industry DSP-doc conventions (block diagrams, transfer functions, frequency/impulse plots, math notation) with Lindelion's allocation-free realtime contract.

## Procedure

When invoked, work in this order. Do not skip steps.

### 1. Open the references you need

| Always | [references/skeleton.md](references/skeleton.md), [references/notation.md](references/notation.md), [references/realtime-contract.md](references/realtime-contract.md) |
| When the doc needs a diagram | [references/diagrams.md](references/diagrams.md) |
| When the doc needs response plots | [references/plots.md](references/plots.md) |
| When you want a complete example to mirror | [references/worked-example.md](references/worked-example.md) — finished doc for `OnePoleLowpass` |

For mechanical scaffolding of a new doc file, run `scripts/new-dsp-doc.sh <module-name>`.

### 2. Pick the section set for the module type

| Module type | Required sections (from skeleton.md) | Required plots |
| ---- | ---- | ---- |
| Filter (IIR/FIR/SVF/TPT) | All 9 | Magnitude + phase + pole-zero |
| Resonator (modal, waveguide) | All 9 | Impulse response + parameter sweep |
| Oscillator | 1–4, 6–9 | Aliasing / noise-floor plot |
| Envelope / smoother / slew | 1–4, 6–9 (skip pole-zero) | Step response |
| Delay / comb / allpass | All 9 | Impulse response |
| Onset / pitch detector | 1–4, 6–9 | Spectrogram of seeded synthetic input + detection-quality table |
| PSOLA / transform | All 9 | Spectrogram + time-domain |

### 3. Fill each section using the skeleton templates

Apply the inline prohibitions below at the moment you write the relevant section.

### 4. Cross-link tests

For every audio-thread method, link the `assert_no_allocations!` test that pins the behavior. If the test does not exist, write the test first and then document it. Use the full test path: `lamath::dsp::engine::tests::note_on_and_render_do_not_allocate`.

### 5. Verify

Run the Lindelion realtime checklist in [references/realtime-contract.md](references/realtime-contract.md). Confirm `make ci` passes.

## Prohibitions (apply inline as you write)

**Theory section:**
- Do NOT mix normalized-frequency conventions without naming which. State `rad/sample [0, π]`, `cycles/sample [0, 0.5]`, or MATLAB-style `[0, 1]` explicitly.
- Do NOT give biquad coefficients without naming the topology (Direct-Form I/II, Transposed-II, SVF, TPT, lattice). They share `H(z)` but differ numerically.
- Do NOT design with BLT without calling out prewarp `g = tan(π·fc/fs)`.
- Do NOT use dBFS for filter gain. dBFS is absolute level; filter gain is a ratio in dB.

**Algorithm section:**
- Do NOT use variable names that differ from the Rust code.
- Do NOT inline LaTeX `$…$` in Rust docstrings — rustdoc does not render it. Use Unicode (ω, π, ∑, ω₀) in code; put LaTeX math only in `docs/`.

**Parameters section:**
- Do NOT omit units. Every entry has `Hz`, `dB`, `ms`, `seconds`, `octaves`, or `0..1`.
- Do NOT invent ranges. Pull from `FloatRange` or constructor clamps in the actual source.

**Response Plots section:**
- Do NOT plot only DC and Nyquist. Show intermediate frequencies and the resonance peak.
- Do NOT generate plots from unseeded randomness. Use `StdRng::seed_from_u64(...)`.
- Do NOT commit `*.svg` without committing the source CSV under `docs/plots/data/`.

**Realtime Contract section:**
- Do NOT claim "realtime safe" without naming which methods are RT-callable.
- Do NOT promise allocation-freeness without an `assert_no_allocations!` test that pins it.
- Do NOT skip stating the Nyquist clamp on cutoff parameters.

**Test cross-links:**
- Do NOT cite tests by partial name. Use the full module path.
- Do NOT introduce external audio fixture files. Synthesize test signals with `tone()` + seeded LCG noise.

## Quick reference

| Need | Recipe |
| ---- | ---- |
| Document a new biquad filter | skeleton.md §1–§9; plots: magnitude + phase + pole-zero; source: `lindelion-dsp-utils/src/filters.rs` |
| Document a modal resonator bank | skeleton.md all; plots: impulse + parameter sweep; source: `plugins/lamath/src/dsp/modal.rs` |
| Document a one-pole smoother | skeleton.md §1–§4, §6–§9; plot: step response; pole-zero N/A |
| Document an onset detector | skeleton.md §1–§4, §6–§9; plots: spectrogram + detection-quality table |
| Add a perf section to an existing doc | skeleton.md §6; cite numbers from `docs/perf/<crate>.md` |
| Audit an existing DSP doc | run [references/realtime-contract.md](references/realtime-contract.md) checklist; quote violations by file:line |
| Scaffold a new doc file | `scripts/new-dsp-doc.sh <module-name>` |

## References

- [references/skeleton.md](references/skeleton.md) — nine-section template.
- [references/notation.md](references/notation.md) — variable naming, units, sign conventions.
- [references/diagrams.md](references/diagrams.md) — Mermaid / hand-SVG / ASCII recipes.
- [references/plots.md](references/plots.md) — Rust → CSV → matplotlib workflow.
- [references/realtime-contract.md](references/realtime-contract.md) — Lindelion realtime safety checklist.
- [references/worked-example.md](references/worked-example.md) — complete doc for `OnePoleLowpass`.
- Canonical workspace convention: `~/repos/ahara/REPO-DOCS.md`.
- Lindelion realtime contract: `docs/performance.md`.
- Lindelion architecture: `docs/architecture.md`.
