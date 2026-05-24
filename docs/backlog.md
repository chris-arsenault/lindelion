# Backlog

Planned-but-not-built work for the Lindelion workspace. Each item is a positive assertion of intended future-state behavior.

Per-product backlogs cover product-specific work:

| Product | Backlog |
| ---- | ---- |
| Lamath | [plugins/lamath-backlog.md](plugins/lamath-backlog.md) |
| Glirdir | [plugins/glirdir-backlog.md](plugins/glirdir-backlog.md) |
| Linnod | [plugins/linnod-backlog.md](plugins/linnod-backlog.md) |

## Host integration

- Validate Lamath and Glirdir as VST3 bundles in Ableton and Logic on macOS.
- Add a CLAP adapter for Lamath and Glirdir alongside the existing VST3 entry points.
- Add an AU adapter for Lamath and Glirdir for Logic Pro X.

## Linnod product

- Implement the Linnod melodic sample-slicer product.

## Shared infrastructure

- Extract a shared VST3 controller plumbing layer once three consumers exist (Lamath, Glirdir, Linnod): default normalized arrays, patch mirrors, parameter info formatting, plain/string conversion, handler restart, and patch-to-processor message flow.
- Extend `lindelion-onset-detect` so `ComplexFlux` and `SpectralSparsity` are distinct algorithms in `ConfiguredOnsetDetector` rather than falling through to `SuperFlux` behavior.

## Performance and CI

- Add a self-hosted Linux runner for `make bench` with baseline storage and regression diffs.
- Wire `bench-smoke` into the regular `make ci` path so bench files cannot rot without paying the full Criterion runtime.

## DSP documentation

- Wire the Rust test → CSV → matplotlib pipeline so DSP module docs in `docs/dsp/` can include actual response plots. Per-module §5 currently lists expected plots with "Pending" status.
- Document the remaining DSP modules: `WaveguideResonator`, `SynthEngine`, `Svf`, `DelayLine`, `FirstOrderAllpass`, smoothing types, onset detector, pitch detector, phrase analysis.
