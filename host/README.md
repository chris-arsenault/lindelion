# Galad — Windows realtime VST3 host

This directory is the home of **Galad** (Cargo package `galad`), a standalone general-purpose
realtime VST3 host application for Windows: live microphone → an ordered chain of arbitrary VST3
plugins → output device, with full device management. It is the *host* side of VST3 (distinct from
the workspace's plugins, which are the guest side), built on the `vst3` crate's COM bindings, native
WASAPI audio I/O, and a **Vizia** (winit standalone) UI (ADR-0024).

It is a **workspace member**, **target-gated** (Windows-only) and **excluded from the Linux/macOS
`make ci` path** (ADR-0022): `make ci` runs `cargo {clippy,test} --workspace --exclude galad`, so the
host's Windows-only deps never enter that build. Current state: **M0–M6 complete** — session model
(M0), host-side VST3 protocol (M1), WASAPI engine (M2), live plugin chain with a lock-free hand-off
(M3), session persistence (M4), plugin-editor windows (M5), and the standalone Vizia host UI (M6).
**M7** (robustness, device hot-swap, plugin matrix, soak) follows.

## Host-side VST3 protocol (M1)

The host side of VST3 — the inverse of the plugins' guest side — lives in `src/vst3_host/`, written
directly against the raw `vst3` crate's COM bindings (ADR-0002, no host framework). The verified flow
for loading and driving one plugin:

1. **Load module** (`module.rs`): open the `.vst3` dynamic library (the DLL at
   `<Bundle>.vst3/Contents/x86_64-win/<Bundle>.vst3`), call `InitDll` if present, resolve
   `GetPluginFactory` → `ComPtr<IPluginFactory>` (kept alive alongside the `libloading::Library`).
2. **Host context** (`host_context.rs`): a `ComWrapper` exposing `IHostApplication` (names the host
   `"Galad"`) + `IComponentHandler`, handed to the plugin as `*mut FUnknown`.
3. **Instantiate** (`instance.rs`): enumerate the factory's classes (`countClasses`/`getClassInfo`),
   find the `"Audio Module Class"`, `createInstance(cid, IComponent::IID, …)` →
   `ComPtr<IComponent>`, `initialize(host)`, `cast::<IAudioProcessor>()`. `PluginInstance` deactivates
   and terminates on drop.
4. **Prepare + process** (`processing.rs`): `setBusArrangements(kStereo)` → `canProcessSampleSize` →
   `setupProcessing(ProcessSetup{ kRealtime, kSample32, … })` → `activateBus`/`setActive`/
   `setProcessing`, then build a `ProcessData` whose `AudioBusBuffers.channelBuffers32` point at
   host-owned buffers and call `process()`.
5. **Spike** (`spike.rs`, `galad spike <path.vst3>`): composes the above and reports the audio class
   name, reported latency, and silence/sine passthrough results.

The COM model is platform-neutral, so steps 2–4 are exercised in-process on Linux against a test
fixture plugin (`cargo test -p galad`); the real Windows `.vst3` module load (step 1 + `spike`) is
cross-compile-verified by `make host-windows-check` and run at runtime on Windows.

## Processing graph (M3)

`galad chain <in-id> <out-id> <plugin.vst3>...` drives a live chain: mic → an ordered serial chain of
VST3 plugins → output. The audio thread runs the chain inside the render callback (`audio/transport.rs`
`render_through` → `vst3_host/chain.rs` `ChainProcessor`), allocation-free and lock-free (ADR-0001).
Chain/parameter edits cross from the control thread via a lock-free prepared-graph swap
(`vst3_host/handoff.rs` `Handoff`): the control thread publishes a new `ChainProcessor`, the audio
thread swaps it in and parks the old one, and reclamation happens on the control thread — never the
audio thread.

## Session persistence (M4)

Each plugin's full configuration is its **opaque state** (`IComponent` get/setState, bridged through a
host memory `IBStream` in `vst3_host/bstream.rs`). `galad chain ... --save <file>` captures the session
on exit — selected devices + the ordered plugin list (path + per-slot bypass + opaque state) — to a
versioned TOML file (`session.rs`), and `galad session <file>` restores it (`vst3_host/session_runtime.rs`
`restore_chain`): same plugins, order, state, devices. Parameter editing is done in each plugin's own
editor window (M5), so Galad keeps no host-side parameter mirror.

## Plugin editor windows (M5)

`galad editor <plugin.vst3>...` opens each plugin's **own** native editor window — Galad instantiates
the plugin's `IEditController` (`vst3_host/editor_controller.rs`), connects it to the component,
`createView("editor")`, and attaches the resulting `IPlugView` into a raw Win32 child window
(`vst3_host/editor_window.rs`), with the host `IPlugFrame` (`editor_frame.rs`) handling plugin-driven
resizes. Multiple editors can be open at once (`editor.rs`). This is how plugin parameters are edited —
the host keeps no parameter mirror of its own.

## Host UI (M6)

`galad` with no subcommand (or `galad ui`) launches the standalone **Vizia** (winit) host window
(ADR-0024): device pickers (input/output), the plugin chain editor (add via a native file dialog,
reorder, bypass, open each plugin's editor, remove), input/output level meters, start/stop, and
session save/load. The UI is split for testability:

- **Framework-neutral core** (`src/ui/state.rs`, `src/ui/command.rs`) — `HostUiState` + `UiCommand`
  + `apply`, plain Rust, unit-tested on Linux (`cargo test -p galad`).
- **Vizia layer** (`src/ui/vizia_app.rs`, Windows-only) — a `Model` mirroring the state into reactive
  signals, the views, and the **controller**: it owns the live `AudioEngine` (M2/M3), the `EditorHost`
  (M5), and the loaded modules, and executes the effectful commands — start/stop, live chain edits
  republished through the M3 `Handoff`, plugin editors, and M4 session save/load. A Vizia timer pulls
  the meters off the audio thread via the lock-free seqlock (`src/audio/meter.rs`).

Meters are published from the audio thread with a wait-free single-writer seqlock and read on the UI
timer — never locking or blocking the realtime path (ADR-0001). The Vizia editor compiles only in the
Windows build; live UI behaviour is a Windows-runtime field check.

## Robustness (M7)

Galad contains the misbehaving-plugin failure modes it can reach **in process** (the deliberate
scope choice — an out-of-process plugin sandbox was considered and rejected as too heavy for a
lightweight host):

- **Load-time validation** (`vst3_host/validate.rs`): before a plugin is added to the chain it is
  probed end-to-end — instantiate the audio class, prepare it, process one silent block — and an
  incompatible plugin (no audio class) or one that errors mid-process is **rejected** at the add
  step rather than failing later on the audio thread.
- **Output sanitation** (`vst3_host/chain.rs`): the chain's output is finite-guarded, so a plugin
  emitting NaN/Inf can never propagate non-finite samples to the device (allocation-free, ADR-0001).
- **Device-fault recovery**: the realtime thread publishes an `EngineStatus` (`audio/engine_status.rs`);
  on a device-invalidated fault the engine exits and the UI's meter timer observes the fault, **stops
  and shows a notice** (the chosen recovery policy), so the user re-selects a device and restarts.

**Residual limit (in-process, by design):** a plugin that *aborts*, corrupts memory, or *hangs* in
its own (C++) code can still bring the host down — that class of failure is only fully isolable
out-of-process. Validation reduces the odds (a plugin that crashes during the probe is rejected
before it is ever added), but it is not a guarantee.

Real-plugin validation and the stability/leak soak are field activities, recorded in
[`PLUGIN-MATRIX.md`](PLUGIN-MATRIX.md).

## Verify

- **Windows build:** `make host-windows-check` — cross-compiles the `galad` binary (incl. the Vizia
  UI) for `x86_64-pc-windows-msvc` from Linux via
  [cargo-xwin](https://github.com/rust-cross/cargo-xwin) (`cargo install cargo-xwin`; the MSVC CRT/SDK
  is downloaded once, `XWIN_ACCEPT_LICENSE=1`). Vizia's skia renderer statically bundles ICU, which
  collides at link with the winit stack's monolithic `windows.0.52.0.lib` umbrella import lib; the
  build passes `/FORCE:MULTIPLE` so skia's static high-level ICU wins and only skia's intended system
  `icu.dll` primitives are imported (see the Makefile note). Runtime verification (live audio, plugin
  hosting, UI behaviour) happens on Windows.
- **Portable logic:** `cargo test -p galad` runs the host's platform-neutral tests (session model,
  meter math, the lock-free meter snapshot, the UI state/command model). A dedicated Windows CI runner
  is deferred until runtime checks are needed.

- Decision: [ADR-0022 — Windows realtime VST3 host application](../docs/adr/0022-windows-vst3-host.md)
- Implementation plan: [`GALAD-HOST-PLAN.md`](../GALAD-HOST-PLAN.md)
