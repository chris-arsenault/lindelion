# ADR-0011: Waveguide Tube Tuning Correction and 2D Mesh Resonator

## Status

Accepted

## Date

2026-05-30

## Context

Lamath's waveguide family had two open items. The 1D Tube bore resonated roughly an octave below
the requested `frequency_hz` above ~165 Hz: its terminations are asymmetric — an inverting mouth
and a non-inverting end — which makes it a quarter-wave resonator, but the tuning assumed the
string's half-wave relationship. Separately, a rectangular 2D digital-waveguide mesh existed only
as a `#[cfg(test)]` prototype, gated behind a promotion checklist (allocation-free processing,
fixed memory, stability at parameter extremes, finite output, per-voice CPU, and an objective
render distinct from ModalBank).

A measurement prerequisite shaped the tuning work: a struck bore's strike response is harmonically
rich and body-coloured, so a DFT magnitude-peak scan is pulled off the fundamental by the spectral
envelope. A periodicity-faithful estimator (sub-sample autocorrelation) was needed to read tube
tuning, and a sub-cent magnitude-peak estimator to validate the cleaner string. Both, and the
permanent steady-state tuning gate, live in `lindelion-dsp-utils` and the waveguide measurement
tests.

## Decision

**Tube tuning.** Tune the bore as a quarter-wave resonator (`cycle_divisor = 4.0`) and compensate
each boundary lowpass's phase delay at the played pitch, so `frequency_hz` is the played pitch.
Upgrade the shared `DelayLine` fractional read from linear to 4-point Lagrange (cubic)
interpolation, which flattens the fractional-delay group delay and keeps the quarter-wave's short
loops in tune at high frequencies. Loop damping is calibrated to an explicit T60(f): the loop gain
pins the fundamental to the requested T60 and the loop filter's roll-off gives higher partials a
faster, calibrated decay. The octave change to existing Tube patches is accepted and recorded in
the changelog.

**2D mesh.** Promote the mesh to its own selectable resonator model named **Mesh**, alongside
Modal and the 1D Waveguide and not behind the String/Tube style. It exposes six physical controls
(material, size, damping, tension, strike position, pickup spread) and is opt-in: the default patch
and all saved patches keep their stored model.

## Consequences

- Existing Tube patches sound an octave higher, since the Tube previously played roughly an octave
  flat above ~165 Hz.
- The cubic `DelayLine` read is shared by every waveguide loop: String tuning is now within ~1.5
  cents across 30 Hz–4 kHz, and Tube tuning within 3 cents through its tunable range.
- Tube tuning accuracy tapers in the top octave: the quarter-wave round trip is only ~5.5 samples
  at 4 kHz / 44.1 kHz, where a sub-sample interpolation residual reaches tens of cents. The
  steady-state gate verifies String across the full range and Tube over its accurate range; the
  bore stays finite, bounded, and decaying across the whole span.
- The patch schema gains `ResonatorConfig::Mesh`; the resonator Model parameter becomes a three-way
  Modal/Waveguide/Mesh selector, with twelve new mesh parameters (six per slot) and ten editor
  slots. Old patches deserialize unchanged.
- The Mesh runs a fixed 14×10 scattering grid: allocation-free `process_sample` and allocation-free
  in-place re-tuning, with fixed per-voice memory. Its per-voice CPU is carried by a `mesh_512`
  Criterion benchmark alongside string/tube.
- Technique-level detail and implementation pointers are catalogued in
  [`../dsp/waveguide-techniques.md`](../dsp/waveguide-techniques.md).

## Alternatives

- **Leave the Tube octave-flat.** Rejected: `frequency_hz` must be the played pitch. The octave
  shift to stored patches is the cost of correctness.
- **Per-boundary-sign adaptive divisor.** Switching half/quarter-wave tuning on the end-reflection
  sign was rejected: the bore is a quarter-wave resonator by design, and a continuous openness
  control should not flip the tuning regime.
- **Compensate filter group delay instead of phase delay.** Group delay matches only near DC;
  resonance is set by accumulated phase, so phase delay is the correct quantity as the pitch nears
  the cutoff.
- **Allpass fractional-delay interpolation.** A first-order allpass is stateful and cannot serve a
  `DelayLine` read called at several positions per sample; stateless 4-point Lagrange was chosen.
- **Tube oversampling for top-octave tuning.** Running the bore loop at 2–4× internal rate would
  recover top-octave accuracy but is a cross-cutting architectural addition; deferred in favour of
  the documented physical range.
- **Keep the mesh prototype-only.** Rejected: it clears every promotion gate, including an
  objective render distinct from ModalBank plus body-color routing.
- **Hide the mesh behind a String/Tube style.** Rejected: a 2D surface is a distinct model, not a
  1D bore boundary mode; it gets its own selectable type.
- **Name it Plate or Membrane.** "Mesh" was chosen to cover both the stiff-plate and tensioned-
  membrane regimes the `material` control morphs between.
