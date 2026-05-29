# ADR-0010: Resample Pro Pitch-Shift Fidelity Strategy

## Status

Accepted

## Date

2026-05-29

## Context

Resample Pro is the Laroche–Dolson-family phase vocoder behind Linnod's pitch shifting:
STFT analysis → formant-envelope gain → phase propagation → transient handling →
windowed-sinc resample. A fidelity program evaluated candidate techniques for the formant
envelope, phase propagation, transient handling, and STFT overlap against an objective battery.

Two measurement facts shaped every choice. First, synthetic fixtures (pure tones, harmonic
stacks) sit at the inter-partial measurement floor (≈ −230 dB) and cannot discriminate between
phase-coherence strategies — a real fixture library (public-domain ensemble, University of Iowa
isolated instruments, owner vocals) was required to choose. Second, the crunch artifact is
reliably detected by the inter-partial-floor metric; the `>6 kHz` high-frequency-artifact ratio
detects aliasing-type crunch but misses mid-band roughness, so the floor metric is the primary
gate. Both batteries and the metric validation live in the `lindelion-pitch-shift` tests.

## Decision

The active engine is: **True-Envelope formant estimation + RTPGHI phase propagation + 87.5 %
STFT overlap** (`analysis_hop = fft_size / 8`), FFT size 4096, with whole-frame transient
re-initialization plus the direct-transient splice.

Peak-locking (Laroche–Dolson identity phase-locking) and bin-level COG transient handling are
retained in the codebase and selected by compile-time constants
(`RESAMPLE_PRO_PHASE_PROPAGATION`, `RESAMPLE_PRO_TRANSIENT_HANDLING`); flipping either constant
swaps the strategy with no other change.

## Consequences

- True-Envelope rides harmonic peaks rather than the moving-RMS mean, locating formants
  accurately; it holds the downstream pure-tone residual and spectral-peak dominance contracts.
- 87.5 % overlap lowers the inter-partial phasiness floor by 8–15 dB versus 75 % on real
  tonal/vocal material (e.g. cello −118 → −129 dB, sung vocal −59 → −68 dB) with no transient
  softening on the real transients tested.
- RTPGHI lowers the phasiness floor further and, at 87.5 % overlap, is cleaner than peak-locking
  at extreme upshift (+12 st HF artifact: sax −20.8 → −25.3 dB, sung vocal −14.2 → −26.7 dB),
  which resolves the regression that had kept it inactive at 75 % overlap.
- The added analysis cost (doubled frame count, the RTPGHI magnitude-ordered heap) is paid at
  setup time only; Linnod renders Resample Pro variants during preparation (ADR-0009), so the
  audio thread is unaffected.
- Transient-side conclusions are bounded by the available real material: there is no isolated
  drum-kit fixture, so the "no transient softening" and bin-level-COG findings rest on cymbal,
  tambourine, and ensemble transients only.

## Alternatives

- **Peak-locking phase propagation.** Near-optimal on clean periodic tones (its design point)
  and the prior default, but at 87.5 % overlap RTPGHI matches or beats it on real material
  including extreme-upshift HF artifact. Retained and selectable.
- **Bin-level COG transient handling.** Reinitializes only bins whose center of gravity sits at
  or after the attack. On real transient-over-sustained material it preserved the attack
  modestly better at 75 % overlap, but the gain reverses at the active 87.5 % overlap, and a
  centered-transient frame is COG-inseparable (it collapses the attack crest). Retained,
  inactive.
- **75 % overlap.** A synthetic sweep favored it on the grounds that 87.5 % softened a synthetic
  impulse; that crest drop was a pure-impulse fixture artifact that does not appear on real
  transients, and the synthetic phasiness differences were sub-audible.
- **Larger FFT (8192).** Improves bass-tonal resolution by ~15 dB but halves time resolution.
  4096 is the balance; the bass headroom is left to a future multiresolution window.
- **Signalsmith weighted multi-prediction phase.** An alternative phase-coherence method; moot
  now that RTPGHI is the active phase path and meets the contracts.
- **HPSS dual-path and noise morphing.** Transient-separation and stochastic-component
  techniques aimed at drums/breath; no demonstrable gap on the available real material justified
  the architectural cost. Tracked in the backlog.

See the technique catalog at [`../dsp/pitch-shift-techniques.md`](../dsp/pitch-shift-techniques.md)
for the per-technique status map and implementation pointers.
