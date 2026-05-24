# 0006 — Typed UI commands

- Status: Accepted
- Date: 2026-05-23

## Context

Vizia editor surfaces communicate with the controller and processor through a command channel. The plugin-shell convention had been to encode commands as float command codes — a common shortcut that loses type safety, obscures intent, and creates ambiguous decoding sites.

## Decision

UI commands are typed `UiCommand` values. Primitive encodings (float codes) are allowed only behind one adapter layer required by the UI/host bridge. Patch save/load/export, sample ingest, sample-slot assignment, slot clearing, and telemetry requests flow through reusable editor services in `lindelion-ui`. Product VST3 editors are thin host adapters: attach/detach lifecycle, controller callback projection, DTO conversion.

## Alternatives considered

- **Float command codes throughout.** Loses type safety and creates fragile decoding. Rejected.
- **A separate UI IPC framework.** Premature given the Vizia binding fits the typed-enum approach cleanly.
- **String-keyed messages.** Pays serialization cost on every UI tick.

## Consequences

- `lindelion-ui` owns the typed `UiCommand` set plus reusable editor services. Product editors compose these.
- File-dialog selection may remain host- or UI-specific, but action handling is shared.
- After Linnod ships, the workspace re-evaluates whether common widgets/services and product compositions should split into separate crates.
