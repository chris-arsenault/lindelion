# Backlog

Planned-but-not-built work for the Lindelion workspace. Each item is a positive assertion of intended future-state behavior.

Per-product backlogs cover product-specific work:

| Product | Backlog |
| ---- | ---- |
| Lamath | [plugins/lamath-backlog.md](plugins/lamath-backlog.md) |
| Glirdir | [plugins/glirdir-backlog.md](plugins/glirdir-backlog.md) |
| Linnod | [plugins/linnod-backlog.md](plugins/linnod-backlog.md) |

## Host integration

- Validate Lamath, Glirdir, and Linnod as VST3 bundles in Ableton and Logic on macOS.
- Add a CLAP adapter for Lamath, Glirdir, and Linnod alongside the existing VST3 entry points.
- Add an AU adapter for Lamath, Glirdir, and Linnod for Logic Pro X.

## Shared infrastructure

- Extract more VST3 controller and patch/library plumbing only when a repeated shape has at least two active consumers beyond the current shared parameter mirror, parameter formatting, state, factory, message helpers, patch filename policy, and sample-library recovery helpers. Candidate areas include patch mirror update flow, patch-to-processor message routing, and controller restart/status handling if those shapes remain duplicated after current product work settles.

## Performance and CI

- Add a self-hosted Linux runner for `make bench` with baseline storage and regression diffs.

## DSP documentation

- Document the remaining DSP modules: `WaveguideResonator`, `SynthEngine`, `Svf`, `DelayLine`, `FirstOrderAllpass`, smoothing types, onset detector, pitch detector, phrase analysis.
- Extend the `make docs` plot set: ModalBank parameter/brightness/strike-position sweeps and spectrum plots; per-preset comparisons; group-delay plots where relevant.
