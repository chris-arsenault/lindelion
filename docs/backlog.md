# Backlog

Planned-but-not-built work for the Lindelion workspace. Each item is a positive assertion of intended future-state behavior.

Per-product backlogs cover product-specific work:

| Product | Backlog |
| ---- | ---- |
| Lamath | [plugins/lamath-backlog.md](plugins/lamath-backlog.md) |
| Glirdir | [plugins/glirdir-backlog.md](plugins/glirdir-backlog.md) |
| Linnod | [plugins/linnod-backlog.md](plugins/linnod-backlog.md) |

## Speech effects port

Port the `hot-mic` channel-strip effects into Rust under `speech/`, tuned for spoken word.
Detailed milestones and the reuse/benchmark strategy live in
[HOTMIC-PORT-PLAN.md](../HOTMIC-PORT-PLAN.md).

- Add the host-agnostic `lindelion-effect` trait crate and the shared `lindelion-fidelity`
  general-signal test harness, validated end-to-end against a gain effect.
- Add the shared speech-tuned gap-fillers the music primitives lack: a peak/RMS envelope
  follower, a saturation shaper, and an extracted allocation-free STFT.
- Add the Tier 1 classic time-domain effects: gain, noise gate, compressor, limiter, de-esser,
  high-pass filter, 5-band EQ, saturation, upward expander, dynamic EQ.
- Add the `speech/signals` analysis-signal derivation library and two-axis benchmarks (N×
  redundant CPU and allocation/realtime safety) that gate any shared analysis context.
- Add the Tier 2 spectral effects: FFT noise removal, air exciter, bass enhancer, consonant
  transient, dereverberation, room tone, spectral contrast, vitalizer.
- Add the Tier 3 ML effects (RNNoise, speech denoiser, voice gate) with model sourcing and
  licensing recorded.
- Source additional public-domain / CC0 spoken-word fixtures and record provenance in
  `testdata/audio/FIXTURES.md`.
- Choose and add a packaging adapter (standalone app, single VST, or per-effect VST) once the
  effect set and signal-sharing decisions settle.

## Host integration

- Validate Lamath, Glirdir, and Linnod as VST3 bundles in Ableton and Logic on macOS.
- Add a CLAP adapter for Lamath, Glirdir, and Linnod alongside the existing VST3 entry points.
- Add an AU adapter for Lamath, Glirdir, and Linnod for Logic Pro X.

## Shared infrastructure

- Extract more VST3 controller and patch/library plumbing only when a repeated shape has at least two active consumers beyond the current shared parameter mirror, parameter formatting, state, factory, message helpers, patch filename policy, and sample-library recovery helpers. Candidate areas include patch mirror update flow, patch-to-processor message routing, and controller restart/status handling if those shapes remain duplicated after current product work settles.

## Performance and CI

- Add a self-hosted Linux runner for `make bench` with baseline storage and regression diffs.

## Pitch-shift fidelity (Resample Pro)

See the technique catalog at [dsp/pitch-shift-techniques.md](dsp/pitch-shift-techniques.md) for
the active and retained techniques.

- Add a multiresolution / dual-window STFT analysis to Resample Pro: a long window at low
  frequencies for bass resolution, a short window at high frequencies for transients, recovering
  the ~15 dB bass-resolution headroom a single 4096 window trades away.
- Add an isolated drum-kit fixture (kick, snare, hi-hat, toms, and a loop) to `testdata/audio/`
  so transient-handling techniques and the 87.5 %-overlap transient behavior can be evaluated on
  percussive material.
- Evaluate an HPSS dual-path (median-filter harmonic/percussive split, phase-vocoder harmonic
  plus short-frame overlap-add percussive) for Resample Pro once a drum/mix fixture exists.
- Evaluate noise morphing (re-excite the stochastic component with fresh phase) for the Resample
  Pro residual on breathy and noisy material.

## DSP documentation

- Document the remaining DSP modules: `WaveguideResonator`, `SynthEngine`, `Svf`, `DelayLine`, `FirstOrderAllpass`, smoothing types, onset detector, pitch detector, phrase analysis.
- Extend the `make docs` plot set: ModalBank parameter/brightness/strike-position sweeps and spectrum plots; per-preset comparisons; group-delay plots where relevant.
