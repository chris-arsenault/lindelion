# Cenedril — Windows Visualizer VST3 — Implementation Plan

**Cenedril** (working name; Quenya/Sindarin "mirror, looking-glass") is a **Windows-only**
passthrough Visualizer VST3: audio passes through **bit-exact at zero declared latency**, and the
value is a rich **egui** editor — a spectrogram (STFT-magnitude, then **reassigned**), level/LUFS
meters, and analysis-signal readouts (voicing, speech presence, onset/flux, HNR). It runs in the
Galad host ([ADR-0022](docs/adr/0022-windows-vst3-host.md)) and Windows DAWs.

Per [ADR-0023](docs/adr/0023-new-vsts-windows-only.md), the new VSTs target **Windows only**. The
**Windows VST3 build path** and the **egui-in-`IPlugView` editor** are not deferred follow-ons —
they are the foundation (M0/M1), and they are reusable by the other new VSTs (Calóma, Coach).
Built properly: phases are ordered by dependency/risk for a complete plugin, not by shipping speed.

## Confirmed decisions

- **Windows-only** VST3 (ADR-0023): a Windows build + bundle path and an **egui** editor; never
  `lindelion-ui` (macOS-only).
- **Passthrough**, bit-exact, **0 declared latency** — analysis is a parallel tap, not in the
  signal path. On `lindelion-plugin-shell::AudioPlugin`, mirroring the Linnod scaffold.
- **Reassigned spectrogram is in scope** (a dedicated milestone after STFT-magnitude).
- **Realtime analysis on the audio thread** (STFT, peak/RMS, LUFS) → a **lock-free SPSC ring** of
  frames + a meter snapshot; the allocating `SignalAnalyzer` (voicing/HNR) runs on an **off-thread
  worker**. The editor renders from the ring/snapshots, never touching the audio thread.
- **CI shape:** `plugins/visualizer` is a `make ci` member — its cross-platform DSP/processor and
  egui view logic are tested on Linux; only the `IPlugView` HWND embedding and the Windows bundle
  are `cfg(windows)`.

## Context / reuse map

*Reuse as-is:* `StftProcessor` (`dsp-utils/src/stft.rs`, allocation-free, sqrt-Hann/75%/realfft)
for spectrogram magnitude; `peak_abs`/`rms`/`AudioWindowMetrics` for levels; `SignalAnalyzer` →
`SignalSnapshot` (voicing/pitch/onset/flux/HNR — runs off-thread, it allocates); the
`AudioPlugin` + `vst3_entry` (factory/processor/controller/messages) + `lindelion-plugin-metadata`
scaffold (mirror **Linnod**); the controller/typed-VST3-message path for audio→editor; **egui**
(shared with Galad).

*Build new:* the **Windows VST3 build/bundle path** (entry-point macro already has
`InitDll`/`ExitDll`; bundling is the gap); the **egui-in-`IPlugView`** editor stack; **LUFS**
(ITU-R BS.1770 K-weighting + integration — absent today); a **streaming lock-free SPSC ring** for
the scrolling spectrogram (the existing editor path caches a single latest value, insufficient for
a frame stream); the spectrogram / reassignment / meter widgets; the **passthrough MAIN-in→out bus
config** (no plain effect plugin exists yet).

*Source-of-truth ADRs:* [ADR-0023](docs/adr/0023-new-vsts-windows-only.md) (Windows-only, egui,
build path); [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (audio thread
allocation-free); [ADR-0022](docs/adr/0022-windows-vst3-host.md) (the host it runs in). Reserved
home: `plugins/visualizer/README.md`.

## Cross-cutting constraints

- **Windows-only build + egui editor** (ADR-0023); `IPlugView` HWND embedding + Windows bundle are
  `cfg(windows)`; the DSP and egui view logic stay cross-platform and `make ci`-tested.
- **Audio thread allocation-free** (ADR-0001): RT analysis (STFT/levels/LUFS) on the audio thread
  into the lock-free ring; `SignalAnalyzer` off-thread; no locks/allocation in `process()`.
- **Bit-exact, zero-latency passthrough** — the analysis tap never delays or alters the audio.
- **The editor reads snapshots only** — never the audio thread, never blocking.

Exit gate for every phase: **`make ci` green** (Linux: cross-platform DSP + egui view logic) *and*
the plugin builds + behaves correctly as a **Windows VST3** in a Windows host (the Windows verify
command is set in M0).

## Milestones

### M0 — `plugins/visualizer` crate, Windows VST3 build path, passthrough processor
The Windows foundation and a working (silent) plugin.
- Register `plugins/visualizer` (cross-platform DSP/processor + egui view logic in `make ci`;
  `IPlugView` HWND embedding + bundle `cfg(windows)`); add `VISUALIZER_VST3_BUNDLE_METADATA`.
- Build the **Windows `.vst3` bundle path** (the repo's first; reusable per ADR-0023): Windows DLL
  → `.vst3` folder layout, Windows bundling automation alongside the macOS path.
- Passthrough processor: MAIN stereo audio-in → MAIN stereo audio-out, bit-exact, **0 latency**.
- **[DECISION]** confirm the name **Cenedril**; the Windows build/verify command.
- Exit: `make ci` green; builds as a Windows `.vst3` that loads in the Galad host / a Windows DAW
  and passes audio through bit-exact at 0 latency.

### M1 — egui-in-`IPlugView` editor (Windows editor foundation; risk) [depends on M0]
Prove the reusable Windows editor stack before building views on it.
- Render an egui editor inside the VST3 `IPlugView` child `HWND` on Windows (egui-baseview or a
  win32/wgpu surface): create/attach/resize/close, focus, parameter readback — with a trivial view
  (e.g. an input meter) to validate the embedding. This is the egui editor stack of ADR-0023.
- **[DECISION]** the egui-in-`IPlugView` integration path (egui-baseview vs raw win32/wgpu) if the
  spike forces the choice.
- Exit: a hosted Cenedril editor opens in a Windows host, renders egui, resizes, closes; no audio
  glitch on open/close.

### M2 — Realtime analysis core + lock-free snapshot hand-off [depends on M0]
The allocation-free audio-thread analysis and the stream to the editor.
- Audio thread, allocation-free: `StftProcessor` (magnitude frames), peak/RMS, and **LUFS**
  (BS.1770 K-weighting + integration, built new) → a **lock-free SPSC ring** of frames + a meter
  snapshot. An **off-thread worker** runs `SignalAnalyzer` → `SignalSnapshot`s.
- **[DECISION]** the LUFS set/standard (BS.1770-4 momentary / short-term / integrated).
- Exit: `process()` proven allocation-free (`assert_no_allocations`); a consumer reads a correct
  stream of STFT frames + meters from the ring; the worker yields `SignalSnapshot`s — all tested
  cross-platform in `make ci`.

### M3 — Spectrogram view (STFT-magnitude) [depends on M1, M2]
The core display.
- Render a scrolling STFT-magnitude spectrogram in the egui editor from the frame ring:
  log-frequency scale, dB color map, time scroll — off the audio thread.
- Exit: a live spectrogram renders correctly in a Windows host from the ring; validated against
  known signals (steady sine → horizontal line; sweep → diagonal; silence → floor).

### M4 — Reassigned spectrogram [depends on M3]
The sharper time-frequency view.
- Add time-frequency **reassignment** (method-of-reassignment via STFT time/frequency derivatives)
  as a selectable view alongside the magnitude spectrogram.
- Exit: the reassigned view measurably sharpens a known multi-component / transient signal vs the
  magnitude view; output is correct and bounded.

### M5 — Meters + analysis-signal readouts [depends on M2, M3]
The rest of the editor surface.
- Level (peak/RMS/crest) + LUFS (momentary/short/integrated) meters; the analysis-signal panel
  (voicing state/score, speech presence, onset/flux, HNR) from the worker's `SignalSnapshot`s.
- Exit: meters are accurate (validated against reference levels and a known-LUFS signal) and update
  from snapshots off the audio thread; the signal panel ranks the spoken-word fixtures correctly
  (voiced vs unvoiced; flat vs animated).

### M6 — Persistence + Windows host validation [depends on M3, M4, M5]
Make it durable and prove it in the real host.
- Persist editor settings (active view, frequency scale, color map, ranges) in plugin state;
  validate the full plugin in the **Galad host** and a Windows DAW; soak for stability/leaks.
- Exit: settings round-trip across reload; the plugin loads/processes/displays correctly in Galad
  and a Windows DAW; no leaks over a soak run.

### Decisions needing your input
| Where | Decision you own |
| ----- | ---------------- |
| M0 | Confirm the name **Cenedril**; the Windows build/verify command. |
| M1 | egui-in-`IPlugView` integration path (egui-baseview vs raw win32/wgpu), if the spike forces it. |
| M2 | The **LUFS** set/standard (BS.1770-4 momentary / short-term / integrated). |

## Handoff

This plan is the single source of truth, at milestone altitude. To execute, run `plan-phase` on
**M0** to expand it into red→green steps, then the EXECUTE-PHASE companion prompt; expand one
milestone at a time. **M0 (Windows VST3 build path) and M1 (egui-in-`IPlugView`) are the
load-bearing Windows-platform foundation** — reusable by Calóma and the Coach — and de-risk the
whole new-VST direction; run them first.
