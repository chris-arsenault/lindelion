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
  run on a spoken-word fixture battery, scored by objective metrics (matched-pair SNR, dereverb,
  clarity, coloration) under hard no-artifact constraints — and committed per order. (As built, gain
  staging is left at unity and loudness is the user's to set; see Outcome.)
- VST3 via `lindelion-plugin-shell` + the `vst3` crate (no new framework — ADR-0002), as a
  **single-component, Windows-only** plugin whose **Vizia editor is the sole control surface (no
  host parameters)** ([ADR-0023](0023-new-vsts-windows-only.md)).

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
- **No host-automatable parameters:** the plugin is self-contained and its Vizia editor is the sole
  control surface (order, per-slot enable/intensity, input/output level), writing lock-free shared
  state the audio thread reads ([ADR-0023](0023-new-vsts-windows-only.md)). Selecting an order loads
  that order's committed defaults (program-change-like).
- ConvolutionReverb is not part of the product (revisit only for deliberate room simulation).
- Latency is the sum of active slots (DFN3's 1920 + STFT frames dominate); a fixed-max strategy
  keeps host delay-compensation stable across order/bypass changes.
- Default parameter sets are reproducible and committed (`make tune-defaults`); the per-order
  full-chain fidelity gates (`make test-models`) fail if a future effect change degrades a shipped
  default (noise worsened, dereverb lost, clipping, latency drift).

## Outcome (as built, M0–M6)

Implemented as a **single-component Windows VST3 with a Vizia control editor** ([ADR-0023](0023-new-vsts-windows-only.md));
see the [spec](../plugins/caloma.md). The implementation refined two things from this decision:

1. **The control surface is the editor, not host parameters.** Calóma surfaces no host automation
   (`getParameterCount() == 0`); the editor writes lock-free `SharedControls` the DSP reads. (The
   "large static parameter surface" this ADR first imagined did not ship.)
2. **Defaults are tonal/dynamics-tuned at unity gain.** Loudness is *measured* by the harness but
   **not** normalized to a fixed LUFS target — chasing one on raw, pause-y speech proved brittle
   (only the most-processed order reached it; forcing the others there required extreme limiting that
   degraded noise reduction and dereverb). A default is a robust starting point; the limiter caps
   peaks and the user sets level. The aggregate-score regression was replaced by robust per-claim
   fidelity gates.
