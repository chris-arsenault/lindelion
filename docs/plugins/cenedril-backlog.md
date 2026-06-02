# Cenedril - Backlog

Planned work for Cenedril. The implemented behavior is in [cenedril.md](cenedril.md).

## Verification

- **On-target Windows load-and-run.** Load the staged `Cenedril.vst3` (`make build-windows
  PLUGIN=cenedril`) in a Windows DAW or the Galad host and confirm it instantiates, passes audio
  through bit-exact at 0 latency, and the Vizia editor renders and is interactive: both spectrogram
  views, the meters, and the analysis panel update live; toggling view / scale / color map / range
  changes the display; settings restore across a save+reload. Soak for memory growth or glitches.
  The bundle cross-compiles + stages cleanly, but Windows behavior is unverified.

## Editor

- **Axis labels.** Add frequency-axis tick labels (Hz, log or linear) and a time/dB legend to the
  spectrogram so readings are quantitative, not just visual.
- **Mel frequency scale.** Add a Mel option to the frequency-scale selector alongside Log and Linear
  for speech-oriented inspection.
- **More color maps.** The selector carries Magma / Viridis / Grayscale; add further perceptually
  uniform maps (e.g. Inferno, Cividis) if a wider palette is wanted.
- **Live re-sync on host setState.** The editor reads persisted settings on open; reflect a
  host-driven `setState` that arrives while the editor is already open, without requiring a reopen.

## Analysis

- **Reassignment tuning.** Expose or refine the reassignment time-spread window (`Cmax`) and the
  flux normalization reference (`FLUX_REF`) once on-target rendering shows what reads best.
