# Lúmedir — Windows Speech-Coach VST3 — Implementation Plan

**Lúmedir** (working name; Quenya `lúme` "time/rhythm" + `-dir` "keeper") is a **Windows-only**
passthrough Speech-Coach VST3: audio passes through unchanged, and the value is feedback on
*delivery* — speaking **rate/cadence** (syllable nuclei), **pitch dynamism** (flat↔animated),
**pause structure**, and **clarity** — as a **live running readout** and an **end-of-session
summary**, scored against **configurable target bands**. It runs in the Galad host
([ADR-0022](docs/adr/0022-windows-vst3-host.md)) and Windows DAWs.

It is the same Windows-only passthrough-VST shell as Cenedril and **reuses Cenedril's M0/M1
platform foundation** (the Windows VST3 build path and the egui-in-`IPlugView` editor) — no new
ADR; it follows [ADR-0023](docs/adr/0023-new-vsts-windows-only.md). The work here is the
**delivery-metric layer**, which is entirely greenfield on top of the existing signal primitives.
Built properly: phases are ordered by dependency for a complete plugin, not by shipping speed.

## Confirmed decisions

- **Windows-only** VST3 + egui editor (ADR-0023), reusing **Cenedril M0/M1** (the Windows build
  path + egui-in-`IPlugView` editor). No new platform work, no new ADR.
- **Passthrough**, bit-exact, 0 latency, on `AudioPlugin` (mirror the Linnod/Cenedril scaffold).
- **All delivery metrics computed off-thread** (windowed) from `SignalAnalyzer` via the
  `AnalysisWorker`; the audio thread only passes audio through and feeds the worker. The editor
  reads snapshots.
- **WPM is acoustic:** speaking rate = syllable-nuclei rate; WPM = syllables/min ÷ a configurable
  syllables-per-word factor. **No ASR / transcription.**
- **Vocal reference = configurable fixed target bands** (e.g. rate band, min pitch-dynamism, max
  pause fraction). No reference recording.
- **Both** a live running readout **and** an end-of-session summary.
- **CI shape:** `plugins/coach` is a `make ci` member — the cross-platform delivery-analysis DSP
  and egui view logic are tested on Linux; only the `IPlugView` HWND embedding and the Windows
  bundle are `cfg(windows)`.

## Context / reuse map

*Reuse as-is:* `SignalAnalyzer` → `SignalSnapshot` via `AnalysisWorker` (off-thread, lock-free
`latest()`): `pitch_hz`/`pitch_confidence`, `voicing_state` (0 silence/1 unvoiced/2 voiced),
`onset_flux_high`, `spectral_flux`; the `SwiftF0` streaming f0 tracker (`PitchFrame`); the
`AudioPlugin` + `vst3_entry` + `lindelion-plugin-metadata` scaffold (mirror **Linnod**);
**Cenedril's M0/M1** Windows build path + egui editor stack. **Validation targets:**
`testdata/audio/FIXTURES.md` — per-fixture `syl/s` and `pstd` (semitones): slow 2.8 / fast 3.8
syl/s; flat **1.1** / animated **7.4** pitch-std; a pauses fixture.

*Build new (the delivery layer — entirely greenfield):* the **syllable-nuclei speaking-rate**
estimator (envelope-peak rate; only the target numbers exist today); **pitch dynamism** (windowed
voiced-f0 → semitone std); **pause structure** (run-length of silence); **clarity** (voicing-ratio
+ onset-sharpness aggregation); **WPM** (rate × factor); **target-band scoring**; **session
accumulation**; the egui **readout + summary** views.

*Source-of-truth ADRs:* [ADR-0023](docs/adr/0023-new-vsts-windows-only.md) (Windows-only, egui);
[ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) (audio thread allocation-free);
[ADR-0022](docs/adr/0022-windows-vst3-host.md) (the host it runs in). Reserved home:
`plugins/coach/README.md`.

## Cross-cutting constraints

- **Windows-only + egui** (ADR-0023), reusing Cenedril M0/M1; HWND embedding + bundle
  `cfg(windows)`; delivery DSP + view logic stay cross-platform and `make ci`-tested.
- **Audio thread allocation-free** (ADR-0001): passthrough + feed the worker only; every delivery
  metric is windowed off-thread; the editor reads snapshots, never the audio thread.
- **Acoustic only** — no ASR; **fixed target bands** — no reference recording.
- **Estimators validated against the `FIXTURES.md` targets** (slow/fast/flat/animated/pauses) —
  they must rank and land near those numbers within tolerance.

Exit gate for every phase: **`make ci` green** (Linux: cross-platform delivery DSP + view logic)
*and* the plugin builds + behaves correctly as a **Windows VST3** in a Windows host.

## Milestones

### M0 — Coach plugin scaffold on the shared Windows platform [depends on Cenedril M0, M1]
A working (silent) Windows plugin that feeds the analysis worker.
- Register `plugins/coach` reusing Cenedril's Windows VST3 build path + egui editor stack;
  passthrough processor (bit-exact, 0 latency); feed the `AnalysisWorker`; add bundle metadata; a
  placeholder editor.
- **[DECISION]** confirm the name **Lúmedir**; where the delivery estimators live — promoted into
  the shared `lindelion-speech-signals` analysis layer (reusable) vs plugin-local.
- Exit: `make ci` green; builds as a Windows `.vst3` that loads in the Galad host / a Windows DAW,
  passes audio bit-exact at 0 latency, and the worker yields `SignalSnapshot`s.

### M1 — Speaking-rate (syllable-nuclei) estimator [depends on M0]
The largest greenfield piece.
- Envelope-peak / syllable-nuclei detector → syllables/min over a sliding window (off-thread);
  WPM = rate ÷ configurable syllables-per-word factor.
- Exit: the estimator ranks the slow (2.8) vs fast (3.8) vs clean fixtures correctly and lands near
  their `FIXTURES.md` `syl/s` targets within tolerance; WPM derives from it.

### M2 — Pitch dynamism + pause structure [depends on M0]
- Windowed voiced-f0 → standard deviation in semitones (flat↔animated); pause structure from
  `voicing_state` silence runs (pause fraction + count/length).
- Exit: pitch-dynamism ranks flat (1.1) vs animated (7.4) strongly and lands near the `FIXTURES.md`
  `pstd` targets; pause metrics are correct on the pauses fixture.

### M3 — Clarity + the delivery snapshot [depends on M1, M2]
- Clarity score from voicing ratio + onset sharpness (`onset_flux_high`); assemble the complete
  delivery snapshot (rate/WPM, dynamism, pauses, clarity) streamed to the editor.
- Exit: the clarity metric behaves sensibly across fixtures; a complete delivery snapshot is
  produced and consumed off the audio thread.

### M4 — egui editor: live running readout [depends on M1, M2, M3]
- The running-readout view (on Cenedril's egui stack): rate/WPM, pitch-dynamism, pause, clarity —
  live gauges updating from snapshots off the audio thread.
- Exit: the live readout renders in a Windows host and updates correctly from the snapshot stream.

### M5 — Session summary + target-band scoring [depends on M4]
- Session accumulation (start/stop; aggregate over the session) + an end-of-session summary view;
  configurable target bands with live + session scoring (in/out of band).
- **[DECISION]** session-boundary semantics (manual start/stop vs auto-on-speech) and the default
  target bands.
- Exit: a session summarizes correctly; target-band scoring flags in/out-of-band; bands + factor
  persist in plugin state.

### M6 — Persistence + Windows host validation [depends on M5]
- Persist settings (target bands, syllables-per-word factor, view prefs); validate in the Galad
  host + a Windows DAW; soak for stability/leaks.
- Exit: settings round-trip across reload; loads/runs/displays correctly in Galad and a Windows
  DAW; no leaks over a soak run.

### Decisions needing your input
| Where | Decision you own |
| ----- | ---------------- |
| M0 | Confirm the name **Lúmedir**; where the delivery estimators live (shared `lindelion-speech-signals` vs plugin-local). |
| M5 | Session-boundary semantics (manual vs auto-on-speech); the default target bands. |

## Handoff

This plan is the single source of truth, at milestone altitude. It **depends on Cenedril's M0/M1**
(the shared Windows VST3 build path + egui editor); build those first. Then run `plan-phase` on
**M0** to expand it into red→green steps, then the EXECUTE-PHASE companion prompt; expand one
milestone at a time. The delivery estimators (M1–M3) are the substance — each is validated against
the `FIXTURES.md` delivery targets.
