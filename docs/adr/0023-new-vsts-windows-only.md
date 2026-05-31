# 0023 — New VSTs target Windows: Windows build path and egui editor

- Status: Accepted
- Date: 2026-05-31

## Context

The new VST3 plugins — Cenedril (the Visualizer), Calóma (the speech chain), and the Speech Coach
— exist to process a live microphone on **Windows**, run inside the Galad host
([ADR-0022](0022-windows-vst3-host.md)) or a Windows DAW. Windows is their only target.

The repo's plugin platform is the opposite today. [ADR-0007](0007-macos-vst3-build-path.md) makes
all `.vst3` bundles macOS-only and `xtask` bundles macOS only. `crates/lindelion-ui` (the Vizia
editor stack) is `#[cfg(target_os = "macos")]`, objc2/AppKit-bound. Earlier planning encoded a
macOS-first assumption the product never had: `CALOMA-VST-PLAN.md` says "Windows is the separate
host project" and defers a custom editor ("generic param list first; Vizia editor as a follow-on").
That assumption is wrong for these plugins — a Windows host cannot run a macOS `.vst3` binary, and
the visual plugins (Cenedril's spectrogram, the Coach's readouts) require custom rendering that a
generic host-parameter list cannot provide. The Windows build path and a Windows editor are not
deferrable follow-ons; they are foundational to these plugins existing at all.

## Decision

The new VSTs (Cenedril, Calóma, Speech Coach) are **Windows-only VST3 plugins**, built first-class:

- **Windows VST3 build path.** Build each as a Windows DLL inside a `.vst3` bundle folder
  (`Contents/x86_64-win/`), with Windows bundling automation alongside the existing macOS path.
  The `export_vst3_entrypoints!` macro already provides the Windows `InitDll`/`ExitDll` entry
  points; the gap is the build/bundle automation, not the entry points.
- **egui editor stack.** The new VSTs render their editors with **egui**, embedded in the VST3
  `IPlugView` child `HWND`. Not `lindelion-ui` (macOS/objc2-bound). This unifies the plugin
  editors with the Galad host (also egui) on one Windows-native, cross-platform-capable UI stack.
- **Audio core stays platform-neutral.** Processing remains on `lindelion-plugin-shell`'s
  `AudioPlugin` and the cross-platform DSP crates; only the build target and the editor stack are
  Windows-specific.

This does not change [ADR-0007](0007-macos-vst3-build-path.md) for the existing **instruments**
(Lamath, Linnod, Glirdir), which remain macOS VST3s on the Vizia editor stack. ADR-0023 governs
the new VSTs.

## Alternatives

- **macOS-first, Windows deferred** (what `CALOMA-VST-PLAN.md` and the placeholder plans assumed).
  Rejected: the new VSTs are used only on Windows; a macOS build is unrunnable there, and deferring
  the Windows build leaves the product non-functional in its only deployment.
- **Extend `lindelion-ui` (Vizia/baseview) to Windows.** baseview supports Windows, but
  `lindelion-ui` is macOS-native (objc2/AppKit) and would need a substantial port, and it keeps two
  editor stacks (the macOS instruments' and the host's egui) diverging. Rejected for egui, which is
  Windows-native, has no macOS coupling, and is already the host's stack.
- **Generic host-parameter editor only** (no custom UI). Works for Calóma's parameters but not for
  the visual plugins, whose value *is* custom rendering. Rejected as the general approach.

## Consequences

- A Windows plugin build/bundle path enters the repo (xtask Windows bundling or a Windows
  equivalent), and an **egui-in-`IPlugView`** editor stack is introduced; its host-window
  embedding (egui rendering into a parent `HWND`) is an early spike risk.
- `CALOMA-VST-PLAN.md` is corrected: Calóma targets Windows with a Windows build and an egui
  editor; its "Windows is the separate host project" framing and macOS-only editor deferral are
  withdrawn.
- The existing macOS instruments and ADR-0007 are unaffected; the repo now builds VST3s for two
  targets (macOS instruments, Windows new-VSTs), each with its own editor stack.
- These plugins run in the Galad host and in Windows DAWs; they are not produced or validated by
  the macOS bundle path or by `make ci`.
