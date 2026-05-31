# Galad — Windows Realtime VST3 Host — Implementation Plan

**Galad** (working name) is a standalone, general-purpose **realtime VST3 host application for
Windows**: a live microphone device → an ordered chain of **arbitrary standard VST3 plugins** →
an output device, with full device management. It runs third-party VST3s and the Lindelion
plugins (Calóma, Lamath, …) identically, with no special path. It is a single-channel signal host
— one input, one serial chain, one output — **not** a multi-track mixer or routing graph. The
packaging decision and rejected alternatives are recorded in
[ADR-0022](docs/adr/0022-windows-vst3-host.md). Out of scope: shipping our own virtual audio
driver (routing into other apps is done by selecting a user-installed virtual cable).

This is a personal tool built properly — phases are ordered by **dependency and risk for a
complete system**, not by shipping speed. There is no "minimal" stand-in version; each phase
delivers real, full capability.

## Confirmed decisions

- **General VST3 host**, hosting arbitrary standard VST3 plugins — not a fixed in-process chain.
- **Single channel:** one input device → one serial VST3 chain → one output device, with device
  management. Not a mixer/graph.
- **Audio:** native **WASAPI**, exclusive-mode primary (low latency) + shared-mode fallback, on a
  lock-free realtime callback. Not `cpal`.
- **Host-side VST3** built on the raw `vst3` crate's COM bindings — no host framework (ADR-0002).
- **UI:** `egui`. Not Vizia.
- **Home:** a new in-workspace **`host/`** binary crate, **target-gated (Windows-only)** and
  excluded from the Linux/macOS `make ci` path.
- **Routing into other apps:** select any output device, including a user-installed virtual cable.

## Context / reuse map

*Reuse as-is:*
- `vst3` 0.3.0 (crates.io) — COM type bindings. Today only the **guest** side is exercised
  (`crates/lindelion-plugin-shell/src/vst3/*`, `plugins/*/src/vst3_entry/*`); the **host** side
  uses the same crate but is written from scratch.
- `lindelion-dsp-utils` — metering/analysis primitives (level, loudness, STFT) for the UI meters.
- `ProcessSetup`/`ProcessContext`/`AudioBuffer` (`lindelion-plugin-shell::process`) — a data-model
  reference for sample-block I/O shapes (the host re-expresses these against raw VST3 `ProcessData`).

*Build new (no prior art in repo):*
- Host-side VST3 protocol: module scan/load, `GetPluginFactory`, class enumeration, instantiate
  `IComponent`/`IAudioProcessor`/`IEditController`, bus/processing setup, `process` driving,
  `IHostApplication`/`IComponentHandler`, `IPlugView` host-attach, parameter + opaque-state bridge.
- WASAPI audio engine: device enumeration, format negotiation, exclusive/shared, lock-free RT
  duplex callback, ring buffers.
- egui host UI; session persistence; the lock-free control↔audio hand-off for chain/param edits.

*Source-of-truth files & ADRs:* [ADR-0022](docs/adr/0022-windows-vst3-host.md) (host platform,
scope, rejected alternatives); [ADR-0002](docs/adr/0002-no-plugin-framework.md) (raw `vst3`, no
framework); [ADR-0007](docs/adr/0007-macos-vst3-build-path.md) (governs *plugin* bundles only, not
the host); [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (the allocation-free
realtime discipline, applied to the host's audio callback). Reserved home: `host/README.md`.

## Cross-cutting constraints

- **Target-gated, Windows-only.** `host/` never enters the Linux/macOS `make ci` build; its
  Windows-only deps (WASAPI, `egui`) never leak into shared crates (ADR-0022).
- **Realtime callback is allocation-free and lock-free.** No allocation, locking, or blocking on
  the audio thread; chain edits and parameter changes cross from the control thread via a
  lock-free, prepared hand-off (the ADR-0001 deadline-safety discipline, applied host-side).
- **General hosting, no special-casing.** Lindelion VST3s load through the same path as any
  third-party VST3.
- **Single channel.** One input → one serial chain → one output; no submixing or routing graph.

Exit gate for every phase: **`make ci` stays green on Linux/macOS (host excluded)** *and* the
host's own Windows build + tests are green (the exact Windows verify command is set in M0).

## Milestones

### M0 — `host/` crate, Windows build path, and the host session model
Stand up the target-gated `host/` binary crate and the core data model with no audio or VST3 yet.
- Register `host/` as a workspace member gated so it builds only on Windows and is excluded from
  the Linux/macOS `make ci` path; wire the Windows verify command.
- Define the host session model: selected input/output devices, the ordered plugin-chain (slots
  with path + per-slot bypass + opaque per-plugin state blob), and app settings — serializable.
- **[DECISION]** confirm the product name (Galad) and the Windows verification command / whether a
  Windows CI runner is added now or later.
- Exit: `host/` builds on Windows; `make ci` unchanged on Linux/macOS; the session model
  round-trips (serialize → deserialize) in tests.

### M1 — Host-side VST3 protocol spike (highest risk; de-risk first) [depends on M0]
Prove the `vst3` 0.3.0 crate can drive the host side, offline, before any live audio.
- Scan/load a `.vst3` module, `GetPluginFactory`, enumerate classes, instantiate
  `IComponent`/`IAudioProcessor` (+ `IEditController`), set up buses + `ProcessSetup`, and call
  `process()` — first on a Lindelion VST3 we control, then a third-party one.
- Implement the minimal `IHostApplication`/`IComponentHandler` the plugins require.
- **[DECISION]** if the crate cannot host cleanly: fork the binding, drop to raw `windows`/COM, or
  choose another path — your call, surfaced with findings.
- Exit: a headless run loads a real VST3 and processes a known signal correctly (e.g. silence
  stays silent, a sine passes with expected gain/latency); the host-side API shape is documented.

### M2 — WASAPI audio engine (full duplex, exclusive + shared) [depends on M0]
The complete native audio path, passthrough first (no plugins) to validate latency end-to-end.
- Enumerate input + output devices; negotiate format/sample-rate/buffer; exclusive-mode primary
  with shared-mode fallback; lock-free realtime duplex callback (capture → ring → render);
  glitch-free start/stop and device selection.
- **[DECISION]** round-trip latency target + buffer-size policy (exclusive vs shared per device).
- **[DECISION]** mono-vs-stereo handling for the single chain (mic is often mono; plugins stereo).
- Exit: live mic → output passthrough is glitch-free at the chosen buffer size; device
  enumerate/select works; measured round-trip latency is recorded.

### M3 — Processing graph: drive the VST3 chain from the audio callback [depends on M1, M2]
Join the host-side VST3 (M1) to the audio engine (M2) under the realtime discipline.
- The RT callback drives an ordered chain of instantiated VST3 processors in sequence; a lock-free
  control→audio hand-off applies chain edits (add/remove/reorder/bypass) and parameter changes by
  swapping a prepared graph — no locks/allocation on the audio thread. Aggregate + report latency.
- Exit: live mic → a chain of ≥2 real VST3 plugins → output, glitch-free; per-slot bypass equals
  identity; chain edits apply without dropouts; the audio thread is proven allocation/lock-free.

### M4 — Parameter + state bridging and session persistence [depends on M3]
Make plugin state and the whole session durable.
- Mirror plugin parameters host-side (read/automate where applicable); capture/restore each
  plugin's opaque state (`IComponent` get/setState); persist the session (devices + ordered plugin
  list + per-plugin state + bypass) to disk and restore on launch.
- Exit: a saved session round-trips exactly across a restart (same plugins, order, state, devices).

### M5 — Plugin editor window hosting (`IPlugView` / child HWND) [depends on M3]
Host plugins' native editor windows.
- Create/attach/resize/close a plugin `IPlugView` in a child `HWND`; focus handling; multiple
  open editors.
- Exit: a hosted plugin's editor opens, edits parameters live (audible), resizes, and closes
  cleanly; opening/closing an editor does not glitch audio.

### M6 — egui host UI [depends on M3, M5]
The full host surface, with meters rendered off the audio thread.
- Input/output device pickers; the plugin chain (scan folder, add/remove/reorder/bypass, open
  editor); input/output level + loudness meters from an audio-thread snapshot ring; start/stop;
  session save/load.
- Exit: every host capability is driveable from the UI; meters are accurate and never block or
  read on the audio thread.

### M7 — Robustness, validation, real-world soak [depends on M4, M6]
Make it survive the real world.
- Device hot-swap/disconnect recovery; plugin scan + incompatible/misbehaving-plugin handling that
  cannot crash the host; error surfaces; validation across a matrix of real third-party VST3s plus
  the Lindelion VST3s (Calóma once it ships). Stability/leak soak.
- Exit: survives device unplug/replug; a misbehaving plugin does not take down the host; a
  documented plugin matrix loads/processes/edits correctly; no leaks over a soak run.

### Decisions needing your input
| Where | Decision you own |
| ----- | ---------------- |
| M0 | Confirm the product name **Galad**; choose the Windows verify command / whether to add a Windows CI runner now. |
| M1 | If `vst3` 0.3.0 cannot host cleanly: fork the binding vs. raw `windows`/COM vs. another path. |
| M2 | Round-trip **latency target** + exclusive-vs-shared buffer policy. |
| M2 | **Mono-vs-stereo** channel handling for the single chain. |

## Handoff

This plan is the single source of truth, at milestone altitude. To execute, run `plan-phase` on
**M0** to expand it into red→green steps, then the EXECUTE-PHASE companion prompt; expand one
milestone at a time, just before running it. M1 (the host-side VST3 spike) is the load-bearing
risk — expanding and running it early validates the whole approach.
