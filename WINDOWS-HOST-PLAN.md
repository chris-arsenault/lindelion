# Windows Realtime Host — Placeholder Plan

> **Status: placeholder.** Enough to pick up on its own branch. Not a full implementation plan.

## Purpose
A small, low-latency realtime **VST3 host for Windows** that runs the speech-chain VST (and other
VST3s) on live microphone input and routes the result to an output device — including a virtual
device so it can feed meeting/streaming apps. This is hot-mic's original role (mic router + engine),
rebuilt to host standard VST3s instead of bespoke built-in plugins. No existing host fits the need
(simple, opinionated, mic-to-VST-chain, virtual output).

## MVP scope
- One input device → serial VST3 chain → one output device.
- WASAPI shared-mode first (exclusive + ASIO later); pick a block size / target latency.
- Load an ordered list of VST3s; per-plugin bypass; show plugin editors.
- Persist the host session (device selection + plugin list + per-plugin state).
- Minimal UI; input/output level meters.

## Key components
- **Audio engine:** device enumeration, WASAPI capture/render, ring buffers, the realtime callback,
  sample-rate/block handling. (Crate candidates: `cpal` or `wasapi`.)
- **VST3 host:** scan/instantiate/process VST3 (host-side of the `vst3` crate), parameter + state
  bridging, editor window hosting.
- **Graph:** serial chain for the MVP (the speech VST does the internal routing).
- **UI:** device pickers, plugin list, meters (toolkit TBD — Vizia/egui).

## Reuse / dependencies
- The `vst3` crate (host side) — same binding the plugins use.
- Possibly share metering with the Visualizer VST.
- **New platform:** the repo's VST3 path is macOS-only today (ADR-0007); this adds a Windows build
  + audio stack. Needs its own ADR + CI consideration.

## Open questions
- WASAPI shared vs exclusive vs ASIO; latency target.
- Virtual output device for routing into other apps (VB-Cable / a virtual driver / loopback).
- UI toolkit; how much of hot-mic's engine/threading model to mirror.
- Decoupled from the speech VST: the VST works in any DAW first; the host is additive.
