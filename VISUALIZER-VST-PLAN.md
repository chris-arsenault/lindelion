# Visualizer VST — Placeholder Plan

> **Status: placeholder.** Enough to pick up on its own branch. Not a full implementation plan.

## Purpose
A standalone **analysis/visualizer VST3** that displays what is happening to the signal:
spectrogram, level/loudness meters, and the analysis signals (voicing, presence, onset/cadence,
HNR). Ports hot-mic's metering/visualization layer (`Metering.md`, `Spectrogram-Rendering.md`,
`Reassignment.md`, `Spectral-Features.md`). Audio passes through unchanged; the value is the
display. Usable in any host alongside the speech-chain VST, or embedded in the Windows host.

## Scope
- Passthrough effect VST3 (audio unchanged, zero/declared latency) with a rich editor.
- Spectrogram (optionally reassigned for sharper time-frequency), with selectable frequency scale.
- Level + integrated-loudness (LUFS) meters; peak/RMS/crest.
- Analysis-signal readouts: voicing state/score, speech presence, onset/spectral flux, HNR.

## Reuse / dependencies
- `lindelion-speech-signals` `SignalAnalyzer` (voicing/onset/HNR) — same one the chain uses.
- `lindelion-dsp-utils` STFT + the spectral analysis; hot-mic's reassignment/feature specs.
- `lindelion-ui` (Vizia editors) for rendering; `lindelion-plugin-shell` + `vst3` for the shell.

## Open questions
- Real-time render performance (spectrogram + reassignment can be costly); render off the audio
  thread from a snapshot ring.
- Which views ship first (spectrogram + meters vs the full feature panel).
- Whether to share a metering core with the Windows host.
