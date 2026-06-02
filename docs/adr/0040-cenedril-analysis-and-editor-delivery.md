# 0040 — Cenedril analysis and editor delivery

- Status: Accepted
- Date: 2026-06-02

## Context

Cenedril is a Windows-only passthrough **Visualizer** ([ADR-0023](0023-new-vsts-windows-only.md)):
the audio passes through bit-exact at zero declared latency, and the value is the editor — a
scrolling spectrogram (magnitude and reassigned), level/LUFS meters, and an analysis-signal panel
(voicing, speech presence, flux, HNR, pitch). All of that data is computed from the input on the
audio thread (allocation-free, [ADR-0001](0001-allocation-free-audio-thread.md)) or on an off-thread
worker, and must reach the editor, which runs on the host UI thread and may live out-of-process.

Two questions had real alternatives:

1. **How the editor reads the analysis.** VST3 separates the audio **processor** from the edit
   **controller**, and the repo's existing convention wires editor data only through typed
   `IConnectionPoint` messages — a model built for parameter/command traffic, not a ~94 frame/s
   spectrogram stream. The factory create-fns (`fn() -> ComPtr<FUnknown>`) cannot inject shared
   state into a separate controller.
2. **How reassignment is computed and carried.** The magnitude spectrogram needs only `|X|`, but the
   reassigned spectrogram needs the method-of-reassignment time/frequency derivatives (three forward
   STFTs per frame). Magnitude frames alone cannot produce reassignment, so the audio→editor
   hand-off has to carry more than magnitudes.

## Decision

**Cenedril is a single-component VST3** — one COM object implements `IComponent` +
`IAudioProcessor` + `IEditController` (VST3 `kSimpleModeSupported`). That one object owns the
analysis, so `createView` hands the editor clones of the audio thread's lock-free cells directly; the
editor renders from them with **no message marshaling and no shared-state handshake**:

- `FrameRing` — an SPSC ring of analysis frames (the spectrogram scroll history),
- `MeterCell` — an atomic level/LUFS + inline-speech snapshot,
- `SignalCell` — an atomic snapshot of the off-thread worker's voicing/pitch/flux/HNR,
- `SettingsCell` — the editor's persisted display settings.

The editor never names a realtime type: the plugin implements UI-crate traits (`SpectrogramSource`,
`ReassignedSource`, `MeterSource`, `SettingsStore`) over one source object, handed to the editor as
trait handles.

**Reassignment is a single unified audio-thread tap.** A forward-only three-window STFT analyzer
(`lindelion_dsp_utils::reassign::ReassignStft`, the method of reassignment) is the *only* spectral
tap; it emits per bin `(magnitude, frequency offset, time offset)`, and `FrameRing` carries all three
lanes. The magnitude spectrogram reads the magnitude lane (a free byproduct); the reassigned
spectrogram scatters all three. The off-thread worker (`AnalysisWorker`) runs the allocating
voicing/HNR analysis; the audio thread relays its latest snapshot into `SignalCell`.

## Alternatives

- **Two classes + VST3 message frame stream.** Keep separate processor/controller and marshal the
  frame/meter stream through `IConnectionPoint`. Convention-faithful, but pushes a high-rate stream
  through a channel built for control traffic — overhead and complexity for no benefit, since
  Cenedril has no parameters to mirror.
- **Two classes + in-process `Arc` handshake.** Pass the `Arc`s controller-ward via a boxed-pointer
  message on `connect()`. Simple but `unsafe` and only valid when host runs both in-process.
- **Reassignment via a second ring** (keep a separate WOLA magnitude tap + a reassignment-only ring,
  gated by a view-active flag). Leaves the magnitude path untouched but runs two spectral analyses on
  the audio thread, needs a second ring and a gate, and duplicates the magnitude.
- **Reassignment computed editor-side** (carry raw frames/complex spectra; FFT in the editor).
  Rejected: pushes FFT infrastructure into `lindelion-ui`, which is kept free of audio-thread DSP.
- **Off-thread reassignment worker.** A second worker thread for reassignment. Rejected: another
  thread plus frame-alignment complexity, with no benefit — the operator is allocation-free and cheap
  enough for the audio thread.

## Consequences

- The editor's read surface is uniform: it drains/reads Cenedril `Arc` cells only, never the audio
  thread, never blocking. Adding a readout means adding a cell + a trait, not a message type.
- One spectral analysis feeds both spectrogram views; magnitude is free and the old WOLA tap's wasted
  inverse FFT is gone.
- The audio thread does the spectral work (allocation-free; `assert_no_allocations` guards stay in
  `make ci`). The settings cell is the one piece **off** the audio thread (editor + state I/O only),
  so it is a plain `Mutex`, not a lock-free primitive.
- Single-component departs from the repo's separate-processor+controller convention; it matches
  Calóma's single-component packaging ([ADR-0020](0020-caloma-speech-vst-packaging.md)) and is the
  right shape for a parameter-free streaming visualizer.
- Changing the frequency scale or dB range rebuilds the display models (the scroll history resets);
  changing the color map only re-composes. These are editor-thread costs, never audio-thread.
