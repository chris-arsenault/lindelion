# 0004 — Parameter registry as single source of truth

- Status: Accepted
- Date: 2026-05-23

## Context

Plugin parameters need consistent treatment across host automation, normalized↔plain conversion, formatting for the host's display, patch get/set, apply policy, runtime targets, smoothing metadata, and editor binding. Hand-wired alternatives drift across these surfaces, causing subtle mismatches such as editor controls that don't trigger host automation or patch values that round-trip incorrectly.

## Decision

A typed parameter registry per plugin owns every host-exposed parameter's metadata. The registry handles host metadata, normalized/plain conversion, formatting, patch get/set, apply policy, runtime target, smoothing metadata, and editor-surface metadata. Enum-valued parameters use typed codecs, not paired free functions per direction. Editor controls derive from the registry — visible controls never maintain a separate parameter ID list.

## Alternatives considered

- **Hand-wired per-surface code.** Drift between host automation, patch I/O, and the editor. Rejected after the resonator parameter architecture debt fix.
- **Procedural macro generation.** Tempting but obscures the surface. Deferred — the registry surface is small enough that explicit code is more debuggable.
- **Runtime dictionary keyed by string.** Loses compile-time type safety and complicates apply policy.

## Consequences

- Every host parameter has exactly one binding and roundtrips patch get/set.
- DSP constants and formulas with semantic meaning live behind named constants or small parameter structs. Tuning behavior has one definition site and tests that pin intentional numerical behavior.
- Editor surfaces prove that every visible control resolves to a binding and every required group has visible controls (see Architectural Tests in `docs/architecture.md`).
- Detection heuristics live in shared configuration/profile structs when the detector is shared; hardcoded musical thresholds are acceptable only when they are truly product policy.
