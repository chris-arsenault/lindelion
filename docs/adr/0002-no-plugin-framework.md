# 0002 — No general-purpose plugin framework

- Status: Accepted
- Date: 2026-05-23

## Context

Plugin frameworks (JUCE, nih-plug, iPlug2) provide an abstraction layer between product code and host SDKs (VST3, AU, CLAP). Each framework brings its own parameter model, UI conventions, lifecycle assumptions, and toolchain footprint. Lindelion needs host adaptation but the product surface (resonators, capture, expression mapping, voice management) is well served by Rust and the existing Vizia UI layer.

## Decision

Lindelion does not depend on a general-purpose plugin framework. The workspace's own `crates/lindelion-plugin-shell` is the abstraction layer. Host SDK adapters live in `crates/lindelion-plugin-shell/src/vst3/` and per-plugin `plugins/*/src/vst3_entry/`.

## Alternatives considered

- **nih-plug.** Rust-native and ergonomic, but ties Lindelion to its parameter/UI model and lifecycle assumptions. Mixing with the existing Vizia editor surfaces would require ongoing reconciliation.
- **JUCE.** C++ adds toolchain complexity, a large FFI surface, and a UI model that conflicts with Vizia. Build-time cost is significant.
- **iPlug2.** Similar concerns to JUCE.

## Consequences

- Lindelion owns the VST3 adapter code. Adding CLAP or AU later means writing an additional adapter in `lindelion-plugin-shell` plus per-product `*_entry/` modules.
- The parameter registry, voice manager, MIDI normalization, patch I/O, and editor service layers remain product-agnostic and host-agnostic.
- Onboarding cost includes learning the workspace's own conventions rather than a third-party framework. This is recovered through tighter control over realtime safety, UI, and parameter behavior.
- Critical rule in `AGENTS.md`: do not add JUCE, nih-plug, or iPlug2 without explicit architecture change.
