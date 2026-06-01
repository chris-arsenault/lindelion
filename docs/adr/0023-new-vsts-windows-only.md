# 0023 — New VSTs target Windows: Windows build path and Vizia editor

- Status: Accepted
- Date: 2026-05-31

## Context

The new VST3 plugins — Cenedril (the Visualizer), Calóma (the speech chain), and the Speech Coach
— exist to process a live microphone on **Windows**, run inside the Galad host
([ADR-0022](0022-windows-vst3-host.md)) or a Windows DAW. Windows is their only target.

The repo's plugin *bundle* path is the opposite today: [ADR-0007](0007-macos-vst3-build-path.md)
makes all `.vst3` bundles macOS-only and `xtask` bundles macOS only. That is the real gap to close.

The **editor stack is not** a deep gap. `crates/lindelion-ui` is built on **Vizia + baseview**,
which is cross-platform (baseview has Win32, AppKit, and X11 backends). Today `vizia` is a
**macOS-target dependency** and the Vizia view code (`glirdir_vizia::platform`, `vizia_controls`,
…) is `#[cfg(target_os = "macos")]`, so it builds only on macOS — *not* on Linux `make ci`, which
compiles only the platform-neutral `editor_surface` scaffolding. Bringing Vizia to Windows is
therefore: (1) add `vizia` (+ `raw-window-handle`) under a `[target.'cfg(target_os = "windows")']`
section, and (2) write the one new piece — a Windows `IPlugView`→`HWND` attach via baseview's
`ParentWindow`/`open_parented` (the same API the macOS attach uses). The macOS-only
drag-and-drop / clipboard / file-dialog code is *instrument* feature (Glirdir/Linnod sample
drag/export and patch files) that the passthrough analysis VSTs do not need. The whole
Vizia/baseview/skia stack **cross-compiles for `x86_64-pc-windows-msvc`** (verified via
cargo-xwin); the Windows editor code is target-gated like the macOS editors, so it is exercised by
the Windows build and host, not by Linux `make ci`.

Earlier planning encoded a macOS-first assumption the product never had — it treated Windows as "the
separate host project" and deferred a custom editor. That deferral of the **Windows build** is the
thing to withdraw. (A prior revision of this ADR over-corrected by also switching the
editor stack to egui on the false premise that `lindelion-ui` is macOS-native; that premise is
wrong — see above — and is withdrawn.)

## Decision

The new VSTs (Cenedril, Calóma, Speech Coach) are **Windows-only VST3 plugins**, built first-class
on the existing **Vizia editor stack**:

- **Windows VST3 build path.** Build each as a Windows DLL inside a `.vst3` bundle folder
  (`Contents/x86_64-win/`), with Windows bundling automation alongside the existing macOS path.
  The `export_vst3_entrypoints!` macro already provides the Windows `InitDll`/`ExitDll` entry
  points; the gap is the build/bundle automation, not the entry points.
- **Vizia editor stack (`lindelion-ui`), extended to Windows.** The new VSTs render their editors
  with **Vizia/baseview**, the same stack the macOS instruments use, embedded in the VST3
  `IPlugView` child `HWND`. This requires building the **Windows `IPlugView`→`HWND` baseview
  attach** — the one new editor piece — which is reusable across all three new VSTs and is also the
  path to bringing the macOS instruments to Windows later. The macOS-only drag-and-drop / clipboard
  / file-dialog code is not needed by the passthrough analysis VSTs and is left macOS-gated.
- **Audio core stays platform-neutral.** Processing remains on `lindelion-plugin-shell`'s
  `AudioPlugin` and the cross-platform DSP crates; only the build target and the editor-window
  attach are Windows-specific.

This does not change [ADR-0007](0007-macos-vst3-build-path.md) for the existing **instruments**
(Lamath, Linnod, Glirdir), which remain macOS VST3s. ADR-0023 governs the new VSTs and adds the
Windows side of the shared Vizia editor stack. It also does not change
[ADR-0022](0022-windows-vst3-host.md): the **Galad host** is a separate standalone *application*
and keeps **egui** for its app UI; that is a host-app choice, independent of the plugins' editor
stack.

## Alternatives

- **egui editor stack for the new VSTs.** Embeds via `egui-baseview` — i.e. the *same* baseview
  child-window layer Vizia uses, so it carries the identical host-window embedding risk and buys
  nothing there. Rejected: it discards the existing Vizia editor work, and it splits the **plugin**
  layer across two GUI frameworks (instruments on Vizia, new VSTs on egui) to match the *host*
  app — when the plugins are more naturally unified with each other on Vizia. egui remains the
  Galad *host* application's UI (ADR-0022); that is a separate target.
- **macOS-first, Windows deferred** (the earlier planning assumption). Rejected: the new VSTs are
  used only on Windows; a macOS build is unrunnable there, and deferring the Windows build leaves
  the product non-functional in its only deployment.
- **Generic host-parameter editor only** (no custom UI). Could expose a parameter-style plugin's
  controls, but not the visual plugins, whose value *is* custom rendering (Cenedril's spectrogram,
  the Coach's readouts — both expressible in Vizia). Rejected as the general approach. (As built,
  Calóma went further: it is self-contained with a custom Vizia editor and **no host parameters** —
  [ADR-0020](0020-caloma-speech-vst-packaging.md).)

## Consequences

- A Windows plugin build/bundle path enters the repo (xtask Windows bundling or a Windows
  equivalent), and a **Windows `IPlugView`→`HWND` baseview attach** is added to the Vizia editor
  stack; that attach (baseview rendering into a host-provided `HWND`) is the early spike risk —
  the same risk any baseview-embedded editor carries.
- The new VSTs reuse `lindelion-ui` and its components; all Lindelion **plugins** stay on one editor
  stack (Vizia), and the new Windows attach is a step toward bringing the macOS instruments to
  Windows. The Galad host stays on egui as a separate application.
- Calóma targets Windows with a Windows build and a Vizia editor; the earlier "Windows is the
  separate host project" framing and macOS-only editor deferral are withdrawn. (As built, Calóma is
  a single-component VST3 with no host parameters — [ADR-0020](0020-caloma-speech-vst-packaging.md).)
- The existing macOS instruments and ADR-0007 are unaffected; the repo now builds VST3s for two
  targets (macOS instruments, Windows new-VSTs) on the shared Vizia editor stack.
- These plugins run in the Galad host and in Windows DAWs; they are not produced or validated by
  the macOS bundle path or by `make ci`.
