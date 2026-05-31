# 0013 — Host-agnostic effect core

- Status: Accepted
- Date: 2026-05-30

## Context

The eventual packaging of the speech effects is undecided. They could ship as a standalone
application (as `hot-mic` does today), as a single VST with a prebaked signal flow, or as one
VST per effect. All three are plausible, and the choice should not be forced now. The
implementation must support any of them without rework.

The existing instrument plugins reach the host through `lindelion-plugin-shell`'s `AudioPlugin`
contract, which depends on `vst3`. Building the effects on that contract would couple them to
VST3 and to a host model before the packaging is chosen.

## Decision

The effect core — the `lindelion-effect` trait and every effect crate under `speech/` — depends
only on the pure-DSP crates (`lindelion-dsp-utils`, `lindelion-pitch-detect`,
`lindelion-onset-detect`). It does not depend on `lindelion-plugin-shell`, `vst3`, or
`lindelion-ui`. Host packagings are separate adapter layers that depend on the core, never the
reverse. The trait exposes only neutral primitives: a plain sample-block `process`, indexed
float parameters, opaque byte-blob state, and latency in samples. Effects make no assumption
about chain position, neighbors, routing, channel count, or sample rate beyond what arrives at
initialization.

## Alternatives considered

- **Build on `lindelion-plugin-shell`'s `AudioPlugin`.** Reuses the instruments' host
  boundary, but couples the effects to VST3 and pulls `vst3` into every effect crate, closing
  off the standalone-app and per-effect-VST options. Rejected.
- **Pick a packaging now (e.g. a single VST) and design to it.** Simpler short-term, but bakes
  a routing and host model into the effects that the other two packagings would have to undo.
  Rejected as premature.
- **A shared compute-once analysis bus across effects.** A host-like coupling between effects
  (a plugin's signals depend on its chain position) — the pattern that made `hot-mic`'s
  analysis routing hard to maintain. Kept out of the core; any shared analysis is an optional
  composition layer, gated on benchmarks. Rejected as a core dependency.

## Consequences

- `lindelion-effect` is a distinct contract from `lindelion-plugin-shell`'s `AudioPlugin`: the
  host-agnostic effect-processor boundary versus the VST3-coupled instrument boundary.
- All three packagings stay open. Adding a VST3, standalone-app, or per-effect wrapper is
  additive adapter work that does not touch the effects.
- Effects self-derive their analysis signals; any shared analysis stays an optional layer,
  usable by every packaging, never a host feature.
- Some wrapper boilerplate is deferred to whichever packaging is chosen later. Accepted as the
  cost of keeping the choice open.
