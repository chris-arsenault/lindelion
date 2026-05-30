# ADR-0016: Global 2x Oversampled Nonlinear Inner Loop

## Status

Accepted

## Date

2026-05-30

## Context

The dynamic-response effects ([ADR-0014](0014-dynamic-response-effort-energy-bus.md)) — energy-
dependent delay modulation (tension modulation), finite-amplitude bore steepening, and geometric
mesh nonlinearity — generate new harmonics whose energy can fold back as aliasing, and whose
feedback can grow unstable, under hard drive at the host sample rate. The realtime path must remain
allocation-free ([ADR-0001](0001-allocation-free-audio-thread.md)), so any oversampling buffers
must be fixed-size and owned per voice.

## Decision

Run the nonlinear resonator inner loop at **2x the host sample rate**, wrapped in a reusable,
allocation-free oversampling stage with half-band up/down filters, from the start of the program.
The wrapper is identity-equivalent within filter tolerance when nonlinearity is zero, so existing
linear patches keep their character. All nonlinear stages share this one substrate rather than each
choosing its own rate.

## Consequences

- One shared oversampling harness (program milestone M3) serves every nonlinear stage; nonlinear
  schemes are written against the oversampled clock.
- Fixed 2x internal buffers are sized at voice construction; no audio-thread allocation.
- The resonator core costs roughly 2x, funded by the prepared-operator refactor
  ([ADR-0014](0014-dynamic-response-effort-energy-bus.md)) and the low-polyphony budget
  ([ADR-0015](0015-expressive-low-polyphony-budget.md)).
- The half-band filters add a small fixed latency that must be reported in the plugin's latency so
  hosts compensate.
- Per-effect objective aliasing tests still run, but against the oversampled baseline rather than
  deciding the rate.

## Alternatives

- **Per-effect oversampling decided by each phase's aliasing test.** A reasonable
  data-driven option, rejected in favor of one uniform substrate: a single rate keeps the nonlinear
  stages composable and avoids per-effect rate divergence.
- **Base-rate only with clamps and energy-conserving schemes.** Cheapest, rejected: audible
  aliasing on extreme drive is exactly the artifact the dynamic effects would otherwise introduce.
- **4x oversampling.** Deferred: 2x is the cost/quality balance for the targeted nonlinearities;
  revisit only if a stage's aliasing test fails at 2x.
