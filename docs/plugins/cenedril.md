# Cenedril - Current Implementation Spec

**Name:** Cenedril
**Name etymology:** Quenya/Sindarin "mirror, looking-glass".
**Target:** **Windows-only** VST3 passthrough **visualizer** (mic/track → unchanged audio + a rich
editor), for the Galad host ([ADR-0022](../adr/0022-windows-vst3-host.md)) or a Windows DAW. Per
[ADR-0023](../adr/0023-new-vsts-windows-only.md).
**Status:** Built through M0–M6. Bit-exact zero-latency passthrough with a single-component VST3 and a
Vizia editor (spectrogram + reassigned view, level/LUFS meters, analysis-signal panel, view/scale/
colormap/range controls, persisted settings). Linux `make ci` covers the cross-platform DSP, the
display models, and the editor build; `make build-windows PLUGIN=cenedril` cross-compiles and stages
`Cenedril.vst3` (editor included) via cargo-xwin. On-target Windows load-and-run is the one item not
yet verified.

The analysis/delivery architecture is [ADR-0040](../adr/0040-cenedril-analysis-and-editor-delivery.md);
the Windows-only / Vizia-editor decision is [ADR-0023](../adr/0023-new-vsts-windows-only.md). The
reassignment operator is documented at [../dsp/reassignment.md](../dsp/reassignment.md). Remaining
work lives in [cenedril-backlog.md](cenedril-backlog.md).

---

## 1. Concept and goals

Cenedril mirrors a signal back to you: the audio is a **bit-exact, zero-declared-latency
passthrough**, and the product is the editor. It shows what a signal *is* — a scrolling spectrogram
(magnitude or time-frequency **reassigned**), broadcast-style level/LUFS metering, and a panel of
speech/voice analysis signals — for inspecting a live microphone or a track in the Galad host or a
Windows DAW.

The analysis is a **parallel tap**: it reads the input only and never the output, so the passthrough
stays bit-exact regardless of what the editor displays.

## 2. Signal path

```
input ─┬─────────────────────────────► output   (bit-exact, 0 declared latency)
       └─► analysis tap (audio thread, allocation-free)
              ├─ ReassignStft → per-bin (magnitude, freq offset, time offset) → FrameRing
              ├─ peak / RMS / crest, BS.1770-4 LUFS, inline speech presence → MeterCell
              └─ mono mix → AnalysisWorker (off-thread) → voicing/pitch/flux/HNR → SignalCell
                                                                      editor reads the cells ◄┘
```

The tap is allocation-free on the audio thread ([ADR-0001](../adr/0001-allocation-free-audio-thread.md)).
A single forward-only three-window STFT (the method of reassignment) is the only spectral analysis;
the magnitude spectrogram reads the magnitude lane, the reassigned spectrogram scatters all three
lanes. The allocating voicing/HNR analysis runs on an off-thread worker; the audio thread relays its
latest snapshot into an atomic cell.

## 3. Analysis and the lock-free hand-off

The audio thread publishes into four shared cells the editor reads directly (no VST3 message
marshaling — [ADR-0040](../adr/0040-cenedril-analysis-and-editor-delivery.md)):

| Cell | Carries | Producer |
| ---- | ---- | ---- |
| `FrameRing` | SPSC ring of analysis frames — per bin `(magnitude, freq offset, time offset)`; the spectrogram scroll history | audio thread (`ReassignStft`) |
| `MeterCell` | peak / RMS / crest, BS.1770-4 momentary/short/integrated LUFS, inline speech presence | audio thread |
| `SignalCell` | voicing state/score, pitch + confidence, onset/spectral flux, HNR | audio thread (relays `AnalysisWorker::latest()`) |
| `SettingsCell` | persisted editor settings (off the audio thread; a `Mutex`) | editor + state I/O |

The shared lock-free primitives (`AtomicF32`, SPSC `SampleRing`) live in
`lindelion-dsp-utils::handoff`; `FrameRing` is frame-granular and builds on `AtomicF32`. The off-thread
worker is `lindelion-speech-signals::AnalysisWorker`.

## 4. Display models

Two platform-neutral, `make ci`-tested models in `lindelion-ui::cenedril_vizia` turn frames into a
scrolling intensity image:

- **`Spectrogram`** — magnitude per frame, max-binned onto the frequency axis.
- **`ReassignedSpectrogram`** — scatters each bin's energy at its **reassigned** row (frequency
  offset) and column (time offset), sharpening tones and transients; energy accumulates into recent
  "open" columns before they finalize into the scroll history.

Both share one frequency axis (`freq_row`: **log** or **linear**), one dB→intensity window
(`db_floor`/`db_ceil`), and one color map (`colormap_for`: **magma**, **viridis**, or **grayscale**).
The level/LUFS meters and the analysis panel render through the shared `lindelion-ui::vizia_meter::
meter_row` widget; each metric supplies its value→fill mapping and formatted text
(`cenedril_vizia::meters`).

## 5. VST3 boundary and the Vizia control surface

Cenedril is a **single-component** VST3: one COM object is processor + controller, so the editor
reads the analysis cells directly ([ADR-0040](../adr/0040-cenedril-analysis-and-editor-delivery.md)). The
audio buses are one stereo in → one stereo out; `getLatencySamples` is 0.

The Vizia editor (`lindelion-ui::cenedril_vizia`, Windows-gated, on the shared `IPlugView`→`HWND`
attach) draws the spectrogram with a Skia raster image and a 66 ms refresh, alongside the meter +
analysis panel. Its controls:

- **View** — Magnitude / Reassigned (both models stay live; switching is instant).
- **Frequency scale** — Log / Linear.
- **Color map** — Magma / Viridis / Grayscale.
- **dB range** — floor and ceiling sliders for the intensity window.

All four settings are the editor's only state; there are no host parameters.

## 6. Persistence

Editor settings (active view, frequency scale, color map, dB floor/ceiling) persist in plugin state
through `lindelion-plugin-shell`'s versioned TOML state format (`CenedrilEditorSettings` +
`TomlPatchFormat`, the shared normal-VST mechanism). The editor reads the settings cell on open and
writes it on each control change; `state()`/`load_state()` serialize the cell, so settings round-trip
across reload.

## 7. Gates (how the build is verified)

- **`make ci`** (Linux) — the reassignment operator and display models (frequency/dB/colormap
  mappings, reassignment sharpening, bounds), the lock-free cells (round-trip + allocation-free), the
  passthrough (bit-exact + allocation-free), and the single-component factory/editor wiring.
- **`make build-windows PLUGIN=cenedril`** (cargo-xwin) — the Vizia editor + controls + Windows
  bundle cross-compile and stage `Cenedril.vst3`.

## 8. Boundaries

Cenedril alters the audio only by passing it through unchanged; every reading is computed from a
parallel tap. The frequency axis, color map, and dB window are display choices on a fixed analysis —
the analysis itself (STFT size, LUFS standard, worker signals) is not user-configurable.
