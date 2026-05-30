# ADR-0015: Expressive Low-Polyphony Per-Voice Budget

## Status

Accepted

## Date

2026-05-30

## Context

The dynamic-response core ([ADR-0014](0014-dynamic-response-effort-energy-bus.md)) — energy-
modulated waveguides, a two-way-coupled body, geometric mesh nonlinearity, and a 2x oversampled
inner loop ([ADR-0016](0016-oversampled-nonlinear-inner-loop.md)) — costs far more per voice than a
static resonator. The per-voice budget decides which physics is affordable, so it must be fixed
before the nonlinear work begins rather than discovered mid-implementation.

## Decision

Target **expressive low polyphony (1–4 voices)**. Spend the budget on per-voice physical richness
rather than voice count: the full nonlinear schemes, including the 2D mesh geometric nonlinearity
and the two-way-coupled body, run per voice.

## Consequences

- Voice allocation and stealing are tuned for a small voice count; per-voice CPU headroom is large
  and is the resource the nonlinear core spends.
- Mesh nonlinearity and oversampling are always-on by default rather than quality-gated; a
  user-facing quality control may still exist but is not required to keep the instrument playable.
- Lamath positions alongside expressive, low-voice physical instruments rather than high-count
  sample-style polysynths.
- `make bench` coverage must track per-voice CPU at the 1–4 voice target so regressions in the
  nonlinear path are caught against a realistic load.

## Alternatives

- **Moderate (~8) or high (16+) polyphony.** Rejected for this program: either would force the
  nonlinear physics down to cheap approximations (especially the mesh) and defeat the project's
  goal of a genuinely dynamic, physically grounded instrument. Higher polyphony can be revisited
  later as a quality-scaled mode without changing the core decision.
