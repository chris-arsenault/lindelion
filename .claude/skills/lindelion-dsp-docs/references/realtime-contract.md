# Lindelion realtime contract — DSP doc checklist

Before merging any DSP-module doc, confirm every item below. Quote the source line for each verified point.

## Required test coverage

- [ ] Audio-thread methods are wrapped by `assert_no_allocations!` from `lindelion-test-allocator`. Pre-allocate output buffers outside the closure.
- [ ] Note-on, voice-stealing, block-render, and sidechain paths are each tested separately if the module is part of a polyphonic instrument.
- [ ] Output is validated by `analysis::assert_all_finite` over a parameter sweep covering the documented range.
- [ ] Parameter sweep covers a representative grid (e.g., `[40, 110, 440, 1760, 4000] Hz` for frequency).

## Required code conventions

- [ ] Filter state is flushed each sample with `lindelion_dsp_utils::math::snap_to_zero`.
- [ ] Inputs are clamped with `finite_clamp` or validated at construction.
- [ ] Frequency parameters respect the Nyquist clamp — typically `fc ≤ fs · 0.49`.
- [ ] Hard caps on per-sample iteration are stated explicitly in the doc (e.g., `MAX_MODE_COUNT = 256`).

## Required doc content

- [ ] Allocation policy is stated explicitly: "Allocation-free; state lives in `Self`" or equivalent.
- [ ] Denormal flushing is named.
- [ ] `reset()` and `prepare(sample_rate, max_block)` semantics are described.
- [ ] Thread safety identifies which methods are RT-callable and which are not.
- [ ] SIMD status is stated: scalar default, or named architecture and intrinsic family.

## Required workspace gates

- [ ] No external audio fixture files are introduced. Test signals are synthesized in-memory via `tone()` plus seeded LCG noise.
- [ ] `make ci` passes (rustfmt, clippy `-D warnings`, file-size lint ≤ 600 lines, all tests).
- [ ] If plots were generated, source CSVs under `docs/plots/data/` are committed and `make ci`'s `git diff --exit-code` check passes.

## Failure modes that block merge

- Documenting allocation-freeness without an `assert_no_allocations!` test.
- Promising stability without naming the bound (feedback delays, resonant filters, and PLLs all diverge for some parameter range).
- Skipping `reset()` semantics — "what does this module do when I change sample rate or stop/start the transport?".
- Promising "realtime safe" as an unqualified claim — say which methods are RT-callable.
