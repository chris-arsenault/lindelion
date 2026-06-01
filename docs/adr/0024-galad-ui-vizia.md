# 0024 — Galad host UI uses Vizia (winit standalone), not egui

- Status: Accepted
- Date: 2026-06-01

## Context

[ADR-0022](0022-windows-vst3-host.md) chose **egui** for the Galad host UI and rejected Vizia, on the
grounds that "Vizia … is macOS/baseview-bound and coupled to the plugin-editor model; not a fit for a
Windows standalone app." That reasoning conflated two things: how the workspace *currently configures*
Vizia, versus what Vizia can actually do.

The workspace depends on `vizia` with `default-features = false, features = ["baseview"]` — only the
**baseview** backend, which embeds a view into a host-provided window (the plugin-editor case). But
Vizia also ships a **`vizia_winit`** backend (its *default* feature) for standalone applications, which
runs on Windows via `winit` + `glutin` + **skia**. This repo already cross-compiles skia for Windows
through `cargo-xwin` (the `make build-windows` target carries a skia-specific link case-fix). So a
standalone Vizia Windows app is a proven, available path here — the ADR-0022 rejection was inaccurate.

Meanwhile the project standardises on Vizia for **every** plugin editor (`lindelion-ui`; macOS today,
the new Windows VSTs per [ADR-0023](0023-new-vsts-windows-only.md)). Keeping egui for the host means
**two UI frameworks shipping on Windows at once** (Vizia for each plugin editor, egui for the host) —
a lasting cost for a single-maintainer project.

## Decision

The Galad host UI uses **Vizia with the `winit` standalone backend**.

- The host is a standalone Vizia application (`vizia_winit::Application`), target-gated Windows-only,
  cross-compiled via `cargo-xwin` (skia, like the plugins).
- This makes Vizia the project's **single UI framework**: plugin editors use Vizia/baseview (guest,
  embedded), the host uses Vizia/winit (standalone). The two share the framework, not code —
  `lindelion-ui` is baseview/plugin-editor-bound, so Galad's UI is new code regardless.
- Plugin editor windows remain **separate native windows** (the `IPlugView`→child-`HWND` hosting from
  [ADR-0022](0022-windows-vst3-host.md) M5), opened from the host UI — they are not re-rendered by
  Vizia. The host UI is the control surface (device pickers, chain editor, meters, start/stop, session
  save/load).
- This **supersedes the `egui` bullet of [ADR-0022](0022-windows-vst3-host.md)**; the rest of ADR-0022
  stands.

## Alternatives

- **egui / eframe (the original ADR-0022 choice).** Turnkey standalone native app, immediate-mode
  (trivial live meters), mature for tool UIs. Rejected: it adds a **second** UI framework on Windows
  alongside Vizia and a new wgpu/glow render stack, for a control surface Vizia handles well. The
  reuse benefit of "one framework" outweighs egui's lower per-screen friction for a solo project. The
  immediate-mode meter ergonomics are real but addressed by a small reactive meter model.
- **Keep egui only if Vizia-winit proves painful.** Considered as a hedge; not adopted — Vizia-winit on
  Windows (skia/cargo-xwin) is already exercised by the workspace's build path, so the risk is low. If
  the standalone path does prove unworkable during the Windows field check, revisit with a new ADR.

## Consequences

- Galad gains a target-gated `vizia = { features = ["winit"] }` dependency (Windows) and pulls the
  skia stack into its Windows build (already built for the plugins). `egui`/`eframe` are not added.
- The host UI is **reactive/retained-mode**: live meters bind to a model updated from an audio-thread
  snapshot (lock-free, off the audio thread per [ADR-0001](0001-allocation-free-audio-thread.md)),
  rather than drawn per-frame from raw values.
- The framework-neutral host logic (meter snapshot ring, UI state model, command/event model) is
  testable on Linux; the Vizia views + the standalone app are cross-compile-verified, with the live UI
  a Windows field check — consistent with the rest of Galad.
