# 0022 — Windows realtime VST3 host application

- Status: Accepted
- Date: 2026-05-31

## Context

Lindelion's products are VST3 *plugins*, loaded by a third-party host (a DAW). One target use —
processing a live microphone through the Calóma speech chain and routing the result into a
meeting/streaming app — has no good off-the-shelf host: existing hosts are either heavyweight
DAWs or do not offer simple mic-in → plugin-chain → device-out with a virtual-device output.
This is the role `hot-mic` filled with bespoke built-in effects; the goal here is to fill it with
a host that runs *standard VST3 plugins* (Calóma and any third-party VST3) instead.

The workspace today is plugin-side only. `crates/lindelion-plugin-shell` and every
`plugins/*/src/vst3_entry` implement the guest side of the `vst3` crate (0.3.0): a plugin
exposes `IComponent`/`IAudioProcessor`/`IEditController` and *receives* a `ProcessData` callback
from a host. Nothing in the repo loads a plugin module, calls `GetPluginFactory`, drives
`IAudioProcessor::process` from a host loop, implements `IHostApplication`, or attaches an
`IPlugView` editor window. There is no audio-device I/O (`cpal`/`wasapi` are absent), and the UI
layer (`lindelion-ui`, Vizia/baseview) is macOS-only and plugin-editor-bound. VST3 *bundle*
builds are macOS-only by [ADR-0007](0007-macos-vst3-build-path.md); CI runs on Linux.

The host is therefore a new platform (Windows), a new binary type (a standalone application, not a
plugin), and a new side of the VST3 protocol (host, not guest).

## Decision

Build a standalone **general-purpose realtime VST3 host application for Windows** as a new
**`galad/` binary crate inside this workspace**, target-gated so it is excluded from the
Linux/macOS `make ci` path and never registered as a building member on those hosts.

- **General hosting, not a fixed chain.** The host loads and runs **arbitrary standard VST3
  plugins** in a single ordered serial chain (mic device → chain → output device), with full
  device management. Calóma and the Lindelion instruments are loaded as ordinary VST3s, with no
  special path. It is a single-channel signal host, not a multi-track mixer or routing graph.
- **Host side built on the raw `vst3` crate**, consistent with
  [ADR-0002](0002-no-plugin-framework.md): the host-side protocol (module scan/load,
  `GetPluginFactory`, instantiate `IComponent`/`IAudioProcessor`, drive `process`, implement
  `IHostApplication`/`IComponentHandler`, attach `IPlugView` via a child `HWND`, bridge parameters
  and opaque state) is written against the crate's COM bindings. No host framework is adopted.
- **Native WASAPI audio I/O**, exclusive-mode as the primary low-latency path with shared-mode as
  a device-compatibility fallback, on a lock-free realtime callback. Not `cpal`.
- **Vizia (winit standalone) for the host UI** (device pickers, chain editor, meters; plugin editors
  are hosted as separate native windows per [ADR-0024](0024-galad-ui-vizia.md)). The workspace already
  standardises on Vizia for plugin editors (`lindelion-ui`, baseview backend); the host uses Vizia's
  `winit` backend so the project runs **one UI framework**. Not `egui` — see
  [ADR-0024](0024-galad-ui-vizia.md), which supersedes this bullet's original `egui` choice.
- **Output routing into other applications** is achieved by selecting any output device, including
  a user-installed virtual cable (VB-CABLE / VoiceMeeter). Shipping a bespoke virtual audio driver
  is out of scope.

This does not change [ADR-0007](0007-macos-vst3-build-path.md): that governs Lindelion *plugin*
`.vst3` bundles (macOS, Apple tooling). The host is a separate Windows application target with its
own build, and is not produced or validated by the macOS bundle path or by `make ci`.

## Alternatives

- **Run the Lindelion chain in-process as a fixed standalone app (no VST3 hosting).** Far less
  code — effects are directly instantiable per [ADR-0013](0013-host-agnostic-effect-core.md) — but
  it would not run third-party VST3s or our own VST3 plugins, defeating the purpose. Rejected: the
  product is a general host.
- **A separate repository.** The host pulls Windows-only deps (WASAPI, Vizia/winit) the rest of the
  workspace does not need. But it reuses the workspace's `vst3` binding and DSP crates and shares
  the same `make ci` toolchain; target-gating keeps the Windows deps off the Linux/macOS build.
  Rejected in favour of an in-workspace `galad/` crate.
- **`cpal` for audio.** Cross-platform and least code, but on Windows it is WASAPI *shared*-mode
  only, capping latency, and adds an abstraction the host does not otherwise need. Rejected for a
  realtime mic tool; native WASAPI gives exclusive-mode latency and direct device control.
- **`egui` for the UI.** The host UI's *original* choice in this ADR (turnkey standalone, immediate-mode
  meters). Superseded by [ADR-0024](0024-galad-ui-vizia.md): Vizia's `winit` backend does standalone
  Windows apps fine (the earlier "Vizia is plugin-bound" claim was inaccurate), and keeping `egui`
  would run two UI frameworks on Windows. Rejected in favour of Vizia (winit).

## Consequences

- A new Windows build path enters the repo. It is target-gated (`cfg(windows)` / a dedicated
  workspace exclusion) so `make ci` on Linux/macOS is untouched; the host compiles and is verified
  on Windows. A Windows verification command is added when the crate is registered.
- The host owns a large amount of new, from-scratch host-side VST3 code with no prior art in the
  repo; the host-side protocol on the `vst3` 0.3.0 crate is unproven and is de-risked by an early
  spike phase.
- The host can run any VST3, so it becomes the live-mic delivery vehicle for Calóma once Calóma
  ships as a VST3, without coupling the two.
- Routing into meeting/streaming apps depends on a user-installed virtual cable; the host does not
  provide one.
