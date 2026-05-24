# 0003 — Shared-core extraction policy

- Status: Accepted
- Date: 2026-05-23

## Context

When two plugin products need the same behavior, duplicated implementations erode quickly: they drift, accumulate product-specific overrides, and resist later extraction. Late extraction tends to be entangled with feature work and gets deferred. The Glirdir/Lamath reuse work demonstrated the cost of postponing extraction.

## Decision

When two real consumers need the same behavior, extract the stable core to a `crates/lindelion-*` crate immediately unless there is a concrete technical blocker. Shared crates contain host-neutral behavior, typed DTOs, validation, reusable state machines, and reusable dispatch/projection logic. Product crates keep product policy local: patch paths, parameter stepping, runtime targets, apply-policy enums, message payloads, UI slots, file naming, MIDI context projection, and product DSP behavior.

## Alternatives considered

- **Extract speculatively at single-consumer time.** Over-engineers boundaries that don't yet exist; the abstraction often turns out wrong when the second consumer arrives.
- **Defer extraction to a later cycle.** In practice, the cycle never arrives and duplication compounds. The Glirdir/Lamath reuse remediation showed how expensive this becomes.
- **Always inline; never share.** Loses correctness wins from shared validation, normalization, and test coverage.

## Consequences

- Single-consumer code that looks generic stays local with a note marking it as a candidate extraction.
- Removing duplication means deleting or rewiring the old local pattern, not wrapping it. If a local variant survives extraction, it needs a product-specific reason.
- Detection heuristics, pitch-frame DTOs, capture state, and audio-expression mapping live in shared crates because they have two or more consumers.
- Product DSP internals (modal banks, waveguide topologies) stay local until a second product needs the same algorithm.
