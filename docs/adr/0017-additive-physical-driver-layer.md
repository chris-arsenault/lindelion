# ADR-0017: Additive Physical-Driver Layer

## Status

Accepted

## Date

2026-05-30

## Context

Excitation today is sample and sidechain playback scaled by a velocity gain (`VoiceExcitation`,
`LiveExcitationBlock`, and the sidechain latch in `dsp/excitation.rs`). There is no physical
driver, so the force-dependent character of a reed, lip, bow, pick, or hammer is absent. For wind
instruments and for the picked-versus-strummed axis on strings, that driver-under-force behavior is
where most of the dynamic identity comes from — the clarinet bore and the guitar body are
comparatively passive.

## Decision

Add force-dependent physical driver models (reed/lip pressure-flow, bow stick-slip, pick/hammer
contact) as a **new selectable driver source** feeding the resonator, driven by the effort/energy
bus ([ADR-0014](0014-dynamic-response-effort-energy-bus.md)). The existing sample and sidechain
excitation remains as one driver option among others; physical drivers and samples can coexist or
blend.

## Consequences

- A new driver stage sits between excitation selection and the resonator input; the patch gains a
  driver-type selector and per-driver parameters.
- Existing sample and sidechain patches are unchanged — sample playback is simply one driver among
  several, preserving the current workflow and the sidechain path.
- Driver nonlinearity runs inside the oversampled inner loop
  ([ADR-0016](0016-oversampled-nonlinear-inner-loop.md)) where it interacts with the resonator at
  the higher rate.
- Each driver archetype gains an objective test of its force behavior (for example reed oscillation
  threshold versus pressure, or contact brightness versus strike force).

## Alternatives

- **Drivers shape the existing excitation signal (force-dependent filtering of the sample or
  sidechain).** Rejected as the primary model: filtering a recorded sample cannot produce reed
  regime changes or stick-slip motion. Retained as a possible blend mode.
- **Replace sample excitation entirely with physical drivers.** Rejected: it drops working
  behavior, the sample-library workflow, and the sidechain excitation path that other Lamath
  features depend on.
