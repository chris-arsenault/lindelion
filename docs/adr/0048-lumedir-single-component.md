# 0048 — Lúmedir is a single-component VST3

- Status: Accepted
- Date: 2026-06-02

## Context

Lúmedir ([ADR-0023](0023-new-vsts-windows-only.md)) is a passthrough speech-coach VST3 whose value
is a live editor readout of delivery metrics. The metrics are computed off the audio thread by a
`DeliveryWorker`; the editor must read the worker's published snapshots to draw its gauges.

Lúmedir was first scaffolded as a **two-class** VST3 (a separate `IComponent`/`IAudioProcessor`
object and a separate `IEditController` object), mirroring the macOS instruments (Linnod), where the
controller exists to expose host-automatable parameters. But Lúmedir surfaces **no host
parameters** — its Vizia editor is the sole control surface (the same self-contained design Calóma
adopted, [ADR-0020](0020-caloma-speech-vst-packaging.md)). In a two-class plugin the controller and
processor are distinct COM objects with no shared Rust ownership, so the editor (created by the
controller via `createView`) has no path to the worker that lives in the processor. The M4 live
readout forced the question of how the editor reaches the worker's snapshots.

## Decision

Lúmedir is a **single-component VST3**: one COM object implements `IComponent` + `IAudioProcessor` +
`IProcessContextRequirements` + `IEditController`. `getControllerClassId` returns `kNotImplemented`
so the host queries `IEditController` on the component, and the factory registers a single audio-
module class.

Because the processor *is* the controller, `createView` and the audio processing share one object.
The editor reads the worker's delivery snapshots directly through a cloned, lock-free
`DeliveryReader` (an `Arc` of the worker's published atomic cells) and edits the live config through
a shared `SharedConfig` — no message marshaling, no shared-state handshake. This matches Cenedril and
Calóma, the other no-host-parameter analysis VSTs.

## Alternatives

- **Two-class with `IConnectionPoint`/`IMessage` marshaling.** Keep the separate controller and push
  each delivery snapshot from the processor to the controller over the VST3 message channel at the
  editor refresh rate. Rejected: it serializes a snapshot per tick across two COM objects, has no
  precedent in the repo, depends on host-specific `IConnectionPoint` behaviour, and is harder to keep
  off the audio thread cleanly — all to preserve a controller that exists only to forward
  `createView`.
- **Keep two-class because the instruments are.** Rejected: the instruments are two-class *because
  they have host-automatable parameters*; that rationale does not apply to a no-parameter analysis
  VST. Unifying the three new VSTs on single-component is simpler and lower-overhead on every target.

## Consequences

- The editor reads live worker output with no marshaling; the audio path stays allocation-free
  (ADR-0001) — the worker reads shared config with `Relaxed` atomic loads off the audio thread.
- All three new no-parameter VSTs (Cenedril, Calóma, Lúmedir) share the single-component shape; the
  separate `LumedirVst3Controller` class is removed.
- The host sees one class that answers both `IComponent` and `IEditController`; persistence flows
  through the component's `getState`/`setState`.
- This is specific to no-host-parameter plugins. A future Lúmedir feature that needs
  host-automatable parameters would revisit this (a controller earns its place when it mediates host
  parameters).
