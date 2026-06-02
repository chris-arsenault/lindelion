# Lúmedir - Current Implementation Spec

**Name:** Lúmedir
**Name etymology:** Quenya `lúme` ("time/rhythm") + `-dir` ("keeper") — "keeper of timing".
**Target:** **Windows-only** passthrough VST3 **analysis** plugin (mic → unchanged audio + delivery
feedback), for the Galad host ([ADR-0022](adr/0022-windows-vst3-host.md)) or a Windows DAW. Per
[ADR-0023](adr/0023-new-vsts-windows-only.md).
**Status:** Built through M0–M6. Single-component VST3 ([ADR-0048](adr/0048-lumedir-single-component.md))
with a Vizia editor. Linux `make ci` and `make test-integration` validate the DSP, the VST3-boundary
load/run, persistence, scoring, and the soak/stability checks; `make build-windows` cross-compiles
and stages `Lumedir.vst3` via cargo-xwin. The editor's on-screen rendering is confirmed on a Windows
host (see [README](../../plugins/lumedir/README.md#windows-host-validation-user-performed)).

This document describes the behavior implemented in the workspace today. The Windows-only / Vizia
decision is [ADR-0023](adr/0023-new-vsts-windows-only.md); the single-component packaging is
[ADR-0048](adr/0048-lumedir-single-component.md); remaining work lives in the
[backlog](../backlog.md).

---

## 1. Concept and goals

Lúmedir passes audio through **bit-exact at zero latency** and delivers value as *feedback on
delivery* — how someone is speaking, not what they say. It scores four delivery dimensions against
configurable target bands and presents them as a live readout plus an end-of-session summary.

Design principles:

- **Acoustic, not linguistic.** Speaking rate is the **syllable-nuclei** rate (an envelope-peak
  cadence proxy); WPM derives from it via a configurable syllables-per-word factor. There is **no
  ASR / transcription**.
- **Fixed configurable target bands**, not a reference recording. Each metric is scored against a
  band the user can edit; defaults are grounded in public-speaking norms (§5).
- **All metrics computed off the audio thread.** The audio thread only mirrors input to output and
  feeds a mono mix to an off-thread `DeliveryWorker`; the editor reads published snapshots. The
  audio path does not allocate or block (ADR-0001).
- **Self-contained and host-agnostic.** Lúmedir surfaces **no host-automatable parameters**; its
  Vizia editor is the sole control surface, and settings persist in the plugin state. Because it has
  no host parameters, it is a single-component VST3 ([ADR-0048](adr/0048-lumedir-single-component.md)).
- **Speech-tuned.** Metric windows, band centers, and defaults are tuned for spoken word and kept
  out of the shared `crates/` foundations.

## 2. Signal path

```
stereo in ──────────────────────────────► stereo out      (bit-exact, 0 latency)
          └─ mono mix ─► DeliveryWorker (off-thread)
                           SignalAnalyzer ─► SignalSnapshot
                           DeliveryAggregator ─► DeliverySnapshot ─► lock-free atomics
                                                                       ▲
                              editor polls latest snapshot (~15 fps) ──┘
```

- The audio thread copies each input channel to its output sample-for-sample (no averaging, no
  sanitization) and pushes a mono mix into a lock-free `SampleRing`
  (`lindelion_dsp_utils::handoff`). Reported latency is 0.
- The worker thread drains the ring in ~frame-sized chunks, runs the shared `SignalAnalyzer`
  (`lindelion-speech-signals`) and the `DeliveryAggregator`, and publishes both the underlying
  `SignalSnapshot` and the assembled `DeliverySnapshot` through per-field atomic cells.
- The editor holds a cloneable `DeliveryReader` (a clone of the worker's `Arc<Shared>`) and reads
  the latest snapshot on the UI thread — never the audio thread.

## 3. Delivery metrics

Assembled in `DeliveryAggregator` (`plugins/lumedir/src/delivery.rs`) from four estimators; see
[docs/dsp/delivery-metrics.md](../dsp/delivery-metrics.md) for the algorithms, windows, and
validation against the speech fixtures.

| Metric | Source | Meaning |
| ---- | ---- | ---- |
| Speaking rate (syl/s) | `speaking_rate.rs` | Syllable-nuclei rate over a sliding window (envelope-peak cadence proxy). |
| Words/min | `speaking_rate.rs` | `rate × 60 ÷ syllables_per_word` (the worker reads the live factor each publish). |
| Pitch dynamism (semitones) | `pitch_dynamism.rs` | Std-dev of windowed voiced f0 in semitones (flat ↔ animated). |
| Pause fraction / count | `pause_structure.rs` | Fraction of the window in silence and the number of silence runs. |
| Clarity (0..1) | `clarity.rs` | Voicing ratio combined with onset sharpness. |

## 4. Editor (Vizia)

The editor (`lindelion-ui::lumedir_vizia`) renders on the shared Vizia/baseview stack, embedded in
the VST3 `IPlugView` child `HWND` on Windows (the shared `vizia_window::ViziaWindowEditor` attach).
It polls the `DeliveryReader` on a ~15 fps timer and shows:

- **Live readout** — six gauges (rate, WPM, dynamism, pause fraction, pause count, clarity) through
  the shared `vizia_meter::meter_row`.
- **In/out-of-band status strip** — one chip per metric, coloured by `BandStatus` (the plugin scores
  each live sample against the current bands).
- **Session** — manual Start/Stop driving a `SessionAccumulator` (UI-side), with an end-of-session
  summary (per-metric mean + fraction of samples in band).
- **Target bands** — steppers editing the syllables-per-word factor and each band edge.

The metric→bar-fill mappings, value formatters, `BandStatus`, and `SessionAccumulator` are
platform-neutral and `make ci`-tested; the Vizia view itself is compiled by the Windows build and
confirmed on a Windows host.

## 5. Configuration, scoring, and state

The coaching config (`plugins/lumedir/src/config.rs`) is the syllables-per-word factor plus the
per-metric `TargetBands`. Defaults, grounded in public-speaking guidance:

| Band | Default | Grounding |
| ---- | ---- | ---- |
| Speaking rate | 120–160 WPM (= 3.0–4.0 syl/s at the 1.5 factor) | Toastmasters / broad consensus for effective delivery. |
| Pitch dynamism | ≥ 3.0 semitones (floor) | Conversational F0 variability ~2–4 st; monotone reads below. |
| Pause fraction | 0.10–0.30 | Articulation-dominant speech with strategic pauses; below ≈0.10 rushed, above ≈0.30 halting. |
| Clarity | ≥ 0.6 (floor) | Calibrated to Lúmedir's internal voicing/onset composite (not an external metric). |

- **Scoring.** `TargetBands` scores a value as `BandStatus::{InBand, Below, Above}` (floor-only bands
  never read `Above`). The `BandStatus` enum lives in `lindelion-ui::lumedir_vizia` (the editor
  boundary type); the scoring logic lives in `config.rs`.
- **Live editing.** The editor edits a lock-free `SharedConfig` (atomic f32 fields); the worker reads
  the factor each publish, so WPM tracks factor edits without respawning. Single-writer (the UI
  thread), `Relaxed` ordering — no audio-thread locks (ADR-0001).
- **Persistence.** `state()`/`load_state()` serialize `LumedirConfig` through the shared
  versioned-TOML patch format (`config_io.rs`, format version 1); a malformed or foreign state is
  ignored and the current config is kept.

## 6. Packaging and build

- **Single-component VST3** — one COM object implements `IComponent` + `IAudioProcessor` +
  `IEditController`, so `createView` and the audio processing share an object and the editor reads
  the worker directly ([ADR-0048](adr/0048-lumedir-single-component.md)).
- **Windows build.** `make build-windows WINDOWS_PLUGINS=lumedir` cross-compiles the MSVC-ABI DLL
  and stages `Lumedir.vst3` (`Contents/x86_64-win/`) via cargo-xwin ([ADR-0023](adr/0023-new-vsts-windows-only.md)).
- **Validation.** The cross-platform DSP, the VST3-boundary load/run (factory → `IComponent`/
  `IAudioProcessor` → `process`), config persistence, band scoring, and session accumulation run in
  `make ci`; the delivery-aggregator bounded-allocation soak and the off-thread worker stability soak
  run in `make test-integration`. The editor's visual rendering is confirmed on a Windows host.
