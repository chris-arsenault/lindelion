# speech/signals

Speech analysis-signal derivation library: the ten normalized signals the speech effects
consume — speech presence, voicing score/state, fricative activity, sibilance energy, onset
flux, pitch and confidence, spectral flux, and harmonic-to-noise ratio.

Each effect **self-derives** the signals it needs from shared crates on demand — there is no
compute-once shared bus. The signals re-use `lindelion-pitch-detect` (SwiftF0) for pitch,
`lindelion-onset-detect`'s flux engine for onset/flux, and `lindelion-dsp-utils` filters and
the shared envelope follower for the envelope-based signals — all re-tuned for speech rather
than sustained musical notes.

A shared compute-once context is a benchmark-gated contingency, not a goal: it is built only if
profiling shows real, shared, expensive redundancy. See the analysis-signal strategy and
two-axis benchmark gate in [HOTMIC-PORT-PLAN.md](../../HOTMIC-PORT-PLAN.md).
