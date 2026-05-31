# 0020 — Speech effects ship as a single VST3 (Calóma)

- Status: Accepted
- Date: 2026-05-31

## Context

The 20 ported speech effects are host-agnostic, with packaging deliberately deferred
([ADR-0013](0013-host-agnostic-effect-core.md)). Two problems block a shippable form:

- Each worker-driven effect embeds its own `AnalysisWorker` (SwiftF0 voicing / onset flux / HNR),
  so a chain of them runs the same heavy analysis N times per block.
- A plugin-per-effect model would reintroduce arbitrary effect ordering and a cross-plugin
  analysis bus — the complexity the port set out to avoid.

The product target is spoken-word clarity, used live (meetings/oration), where the signal flow has
a *correct* order — so an opinionated chain with sane gain staging is desirable, not limiting.

## Decision

Package the speech effects as a single VST3, **Calóma** (Quenya `cala` "bright/clear" + `óma`
"voice" → "clear voice"):

- An internal **serial chain** of the effect modules, with gain-staging nodes at defined points.
- **Signal order is a 3-valued parameter** — three curated chain topologies, each with an
  end-to-end-tuned **default parameter set**. Selecting an order loads that order's defaults
  (orders are starting points).
- **Patches are normal VST patches** (`plugin-shell` `TomlPatchFormat` ↔ `PluginState`): a patch
  captures the order value **plus** the full parameter tuning (per-effect params + per-slot
  enable); loading a patch sets both. The order lives *inside* the patch — they are not orthogonal.
- **Compute-once analysis:** one shared `AnalysisWorker` computes the `SignalSnapshot` per block;
  the chain injects it into the slots that consume it. No per-effect workers, no cross-effect bus.
  The neutral `lindelion-effect` trait stays signals-free (ADR-0013); signal injection is a
  speech-layer concern on the consuming crates.
- NN effects (DFN3 denoiser, Silero voice gate) run **inline** ([ADR-0018](0018-nn-inference-allocation.md));
  they are chain slots, not analysis consumers.
- **Default parameter sets are chosen by an offline end-to-end tuning harness** — the full chain
  run on the spoken-word fixture battery, scored by objective metrics (loudness target,
  matched-pair SNR, dereverb, clarity, low coloration) under hard no-artifact constraints — and
  committed per order, with a regression guard.
- VST3 via `lindelion-plugin-shell` + the `vst3` crate (no new framework — ADR-0002); macOS build
  path ([ADR-0007](0007-macos-vst3-build-path.md)).

This resolves the contingent shared-analysis milestone (M4 of the port plan). **ConvolutionReverb
is dropped from the product** (it adds reverberation, against the clarity goal; Dereverberation is
the speech tool). The Windows realtime host, the Visualizer VST, and the Speech Coach VST are
separate projects (placeholder plans at the repo root; backlog).

## Alternatives considered

- **One VST per effect.** Reintroduces arbitrary ordering and a cross-plugin analysis bus — the
  exact complexity the port avoided. Rejected.
- **Standalone app first.** Deferred — the VST runs in any host (a DAW now, the custom host later),
  so the app is additive, not a prerequisite.
- **Keep per-effect analysis workers.** N redundant SwiftF0 runs per block in a chain. Rejected in
  favor of one shared worker (this is what M4 anticipated).
- **Fully orthogonal order × patch.** Rejected — the order is a parameter carrying a default
  tuning, so a patch must capture both; treating them as independent loses the "select order →
  load its defaults" behavior.
- **Hand-tuned defaults.** Rejected in favor of the objective end-to-end tuning harness, so
  defaults are measured and reproducible.

## Consequences

- One shared analysis worker replaces the per-effect workers; the signal-injection refactor
  changes the worker-consumer crates' shape and retires the test-only `sync-analysis` feature
  (tests inject known snapshots instead).
- A large static parameter surface (the order parameter + ~16 effects' parameters). The order
  parameter reloads defaults when changed — program-change-like behavior on a normal parameter.
- ConvolutionReverb is not part of the product (revisit only for deliberate room simulation).
- Latency is the sum of active slots (DFN3's 1920 + STFT frames dominate); a fixed-max strategy
  keeps host delay-compensation stable across order/bypass changes.
- Default parameter sets are reproducible and committed; a regression guard fails if a future
  effect change degrades a shipped default below its quality floor.
