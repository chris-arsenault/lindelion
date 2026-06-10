# Galad — Windows realtime VST3 host

**Galad** (Cargo package `galad`) is a standalone general-purpose realtime VST3 host application for
Windows: live microphone → an ordered chain of arbitrary VST3 plugins → output device, with full
device management. It is the *host* side of VST3 (distinct from the workspace's plugins, which are the
guest side), built on the `vst3` crate's COM bindings, native WASAPI audio I/O, and a **Vizia** (winit
standalone) UI ([ADR-0024](../docs/adr/0024-galad-ui-vizia.md)). It loads Lindelion VST3s and
third-party VST3s through the same path; it is a single-channel signal host, not a mixer or routing
graph ([ADR-0022](../docs/adr/0022-windows-vst3-host.md)).

It is a workspace member, target-gated (Windows-only) and excluded from the Linux/macOS `make ci` path:
`make ci` runs `cargo {clippy,test} --workspace --exclude galad`, so the host's Windows-only deps never
enter that build. The platform-neutral COM and host logic is tested in-process on Linux; the Windows
shell is cross-compile-verified and runtime-verified on Windows.

## Host-side VST3 protocol

The host side of VST3 — the inverse of the plugins' guest side — lives in `src/vst3_host/`, written
directly against the raw `vst3` crate's COM bindings (no host framework,
[ADR-0002](../docs/adr/0002-no-plugin-framework.md)). Loading and driving one plugin:

1. **Load module** (`module.rs`): open the `.vst3` dynamic library (the DLL at
   `<Bundle>.vst3/Contents/x86_64-win/<Bundle>.vst3`), call `InitDll` if present, resolve
   `GetPluginFactory` → `ComPtr<IPluginFactory>` (kept alive alongside the `libloading::Library`).
2. **Host context** (`host_context.rs`): a `ComWrapper` exposing `IHostApplication` (names the host
   `"Galad"`) + `IComponentHandler`, handed to the plugin as `*mut FUnknown`.
3. **Instantiate** (`instance.rs`): enumerate the factory's classes, find the `"Audio Module Class"`,
   `createInstance` → `ComPtr<IComponent>`, `initialize(host)`, `cast::<IAudioProcessor>()`.
   `PluginInstance` deactivates and terminates on drop.
4. **Prepare + process** (`processing.rs`): `setBusArrangements(kStereo)` → `canProcessSampleSize` →
   `setupProcessing` → `activateBus`/`setActive`/`setProcessing`, then build a `ProcessData` whose
   `AudioBusBuffers.channelBuffers32` point at host-owned buffers and call `process()`.

The COM model is platform-neutral, so steps 2–4 run in-process on Linux against a test fixture plugin
(`cargo test -p galad`); the real Windows `.vst3` module load is cross-compile-verified by
`make host-windows-check` and exercised at runtime on Windows. `galad spike <path.vst3>` composes the
above and reports the audio class name, reported latency, and silence/sine passthrough results.

## Audio engine

Native WASAPI duplex (`src/audio/`): a lock-free realtime callback runs capture → ring → render,
exclusive-mode primary with shared-mode fallback. The host runs the chain at the **device's** sample
rate (`device_sample_rate`, a `GetMixFormat` probe) and declares that rate to each plugin via
`setupProcessing` — it performs no sample-rate conversion, and input and output devices share a rate.
The default system input/output are pre-selected on launch.

## Processing graph and live chain edits

The audio thread runs the chain inside the render callback (`audio/transport.rs` `render_through` →
`vst3_host/chain.rs` `ChainProcessor`), allocation-free and lock-free
([ADR-0001](../docs/adr/0001-allocation-free-audio-thread.md)). Chain edits cross from the control
thread via a lock-free prepared-graph swap (`vst3_host/handoff.rs` `Handoff`): the control thread
publishes a new `ChainProcessor`, the audio thread swaps it in and parks the old one, and reclamation
happens on the control thread.

The controller keeps a **persistent pool of plugin instances** (`PoolSlot`), and the `ChainProcessor`
is an *ordering* over the pool — it holds `Arc<PluginInstance>` clones, not the instances themselves.
Reorder/bypass/add/remove rebuild only the ordering over the same live instances, so plugin state
(parameters and transient DSP state) is preserved across edits; an instance is torn down only when its
pool slot and every referencing chain are gone, always on the control thread
([ADR-0025](../docs/adr/0025-galad-chain-edit-state-pool.md)).

## Session persistence

Each plugin's full configuration is its **opaque state** (`IComponent` get/setState, bridged through a
host memory `IBStream` in `vst3_host/bstream.rs`). A session is the selected devices + the ordered
plugin list (path + per-slot bypass + opaque state) + scanned folders, persisted as versioned TOML
(`session.rs`). Save captures each plugin's state from the live pool on the UI thread (gapless);
`restore_pool` (`vst3_host/session_runtime.rs`) loads the modules back, instantiates, and restores
state. Parameters are edited in each plugin's own editor window, so the host keeps no parameter mirror.

## Plugin editor windows

Galad opens each plugin's **own** native editor window: it instantiates the plugin's `IEditController`
(`vst3_host/editor_controller.rs`), connects it to the component, `createView("editor")`, and attaches
the resulting `IPlugView` into a raw Win32 child window (`vst3_host/editor_window.rs`), with the host
`IPlugFrame` (`editor_frame.rs`) handling plugin-driven resizes. Multiple editors can be open at once.

## Host UI

`galad` with no subcommand (or `galad ui`) launches the standalone **Vizia** (winit) host window
([ADR-0024](../docs/adr/0024-galad-ui-vizia.md)): device pickers, the plugin chain editor (add via a
native file dialog or a folder scan, reorder, bypass, open each plugin's editor, remove),
input/output level meters, start/stop, and session save/load. The UI is split for testability:

- **Framework-neutral core** (`src/ui/state.rs`, `src/ui/command.rs`) — `HostUiState` + `UiCommand`
  + `apply`, plain Rust, unit-tested on Linux.
- **Vizia layer** (`src/ui/vizia_app.rs`, Windows-only) — a `Model` mirroring state into reactive
  signals, the views, and the **controller** that owns the engine, the editor host, and the instance
  pool, and executes the effectful commands. A Vizia timer pulls the meters off the audio thread via a
  wait-free single-writer seqlock (`src/audio/meter.rs`) — never locking or blocking the realtime path.

## Robustness

Galad contains the misbehaving-plugin failure modes it can reach **in process**
([ADR-0026](../docs/adr/0026-galad-in-process-plugin-containment.md)):

- **Load-time validation** (`vst3_host/validate.rs`): a plugin is probed end-to-end — instantiate,
  prepare, process one silent block — before it is added, so an incompatible plugin (no audio class)
  or one that errors mid-process is rejected at the add step rather than failing on the audio thread.
- **Output sanitation** (`vst3_host/chain.rs`): the chain output is finite-guarded, so a plugin
  emitting NaN/Inf cannot propagate non-finite samples to the device.
- **Device-fault recovery**: the realtime thread publishes an `EngineStatus` (`audio/engine_status.rs`);
  on a device-invalidated fault the engine exits, the meter timer observes it, and the host stops and
  shows a notice so the user can re-select a device and restart.
- **Crash diagnostics** (`diagnostics.rs`): startup installs a Rust panic hook and, on Windows, an
  unhandled native-exception filter. Fatal panics and SEH crashes write a final reason line to
  `%LOCALAPPDATA%\Galad\galad.log` before Windows terminates the process.

A plugin that aborts, corrupts memory, or hangs in its own code can still bring the in-process host
down; full crash isolation is out of scope. Real-plugin validation and the stability/leak soak are
field activities, recorded in [`PLUGIN-MATRIX.md`](PLUGIN-MATRIX.md).

## Verify

- **Windows build:** `make host-windows-check` cross-compiles the `galad` binary (incl. the Vizia UI)
  for `x86_64-pc-windows-msvc` from Linux via [cargo-xwin](https://github.com/rust-cross/cargo-xwin)
  (`cargo install cargo-xwin`; the MSVC CRT/SDK is downloaded once, `XWIN_ACCEPT_LICENSE=1`). Vizia's
  skia renderer statically bundles ICU, which collides at link with the winit stack's monolithic
  `windows.0.52.0.lib` umbrella import lib; the build passes `/FORCE:MULTIPLE` so skia's static
  high-level ICU wins and only skia's intended system `icu.dll` primitives are imported (see the
  Makefile note).
- **Release build:** the release chain builds every distributable `--release` into a **separate
  in-repo target dir** (`./target-release`, gitignored; `$(LINDELION_RELEASE_TARGET_DIR)`) so the
  artifacts live in the repo where you can grab them — not a hidden home-dir cache — and release-
  profile cache invalidation never touches the day-to-day caches (`./target` for `make ci`/tests, and
  the iteration cache `./target-build`). `make release` dispatches by host OS: on Linux/Windows,
  `make release-windows` builds `galad.exe` **and** the Windows VST3 plugin bundles (into
  `./target-release/bundles/`); on macOS, `make release-macos` builds the instrument bundles.
  (`make host-windows-release` builds just `galad.exe`.) Output:
  `./target-release/x86_64-pc-windows-msvc/release/galad.exe`. The exe links
  the MSVC CRT dynamically, so the target needs the **VC++ 2015–2022 Redistributable (x64)** (present
  on most Windows installs); every other import is a system DLL and skia/ICU are statically linked.
  For a fully standalone exe, add `-C target-feature=+crt-static` if skia's prebuilt CRT linkage
  allows.
- **Portable logic:** `cargo test -p galad` runs the host's platform-neutral tests (session model,
  meter math, the lock-free meter snapshot, the chain/pool, validation, and the UI state/command
  model). Live behaviour (audio, plugin hosting, UI) is verified on Windows.

Decision: [ADR-0022 — Windows realtime VST3 host application](../docs/adr/0022-windows-vst3-host.md).
