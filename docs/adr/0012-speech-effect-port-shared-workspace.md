# 0012 — Speech effect port shares the workspace

- Status: Accepted
- Date: 2026-05-30

## Context

The channel-strip effects from the `hot-mic` C# microphone processor are being ported to Rust.
This is a different product from the existing Lindelion instruments: it targets spoken word in
a meeting / oration context on Windows, where the instruments target wind-like musicality as
VST3 plugins in Ableton on macOS. The intent (speech clarity vs. musicality) and the eventual
runtime context differ.

What the two share is the expensive part. The DSP primitives (`lindelion-dsp-utils`), SwiftF0
pitch detection (`lindelion-pitch-detect`), the onset/flux engine (`lindelion-onset-detect`),
and the audio-fidelity test harness are the hardest assets to build correctly, and both
products depend on them. The question is whether the speech port lives in this workspace or a
separate repository.

## Decision

The speech effect port lives in the existing Lindelion workspace. Use-case-neutral foundations
(`lindelion-effect`, `lindelion-fidelity`, and the existing DSP/analysis crates) stay in
`crates/`; speech-specific effects and their analysis-signal derivation live in a dedicated
top-level `speech/` tree. Product and packaging divergence is isolated at the adapter layer
(see [ADR-0013](0013-host-agnostic-effect-core.md)).

## Alternatives considered

- **Separate repository.** Cleanest product separation, but the shared foundations must then be
  vendored or published as versioned crates and kept compatible across two repos — overhead
  paid on every fidelity improvement, which is exactly the shared work. Drift between two
  copies of the hardest code is the likely failure. Rejected.
- **Monorepo, speech effects flat in `crates/`.** Avoids a new top-level tree, but muddies the
  product boundary and makes a future split (by `git` subtree extraction) harder to draw.
  Rejected in favor of a dedicated `speech/` tree.
- **Defer the decision.** Starting without a home blocks scaffolding and the durable docs the
  port needs. Rejected.

## Consequences

- One CI path, one allocation-free contract, one fidelity harness serve both products.
  Improvements to shared DSP, pitch, and metrics benefit the instruments and the effects.
- Speech-specific tuning (defaults, thresholds, band centers, f0 ranges) must stay in the
  `speech/` crates and must not leak into the neutral `crates/` foundations. This is a review
  rule, enforced by keeping the shared crates use-case-neutral.
- The physical `crates/` vs `speech/` boundary, plus the host-agnostic core, keeps a future
  repository split cheap should the products diverge in toolchain, realtime contract, or
  release cadence.
- The workspace grows; CI time grows with the effect roster. Accepted as cheaper than
  cross-repo coordination.
