# ADR-0029: M11 Gain Staging — Per-Family Makeup and Master Soft-Clip

## Status

Accepted

## Date

2026-06-01

## Context

The M11 instrument-voicing program ([ADR-0014](0014-dynamic-response-effort-energy-bus.md)) left the
resonator families correctly *voiced* but badly *staged*. Measured at a single full-velocity voice:

- The instrument was ~30 dB too quiet — a String/Mesh note output ≈ −52 dBFS RMS.
- The families emerged ~38 dB apart — Modal ≈ −1.7 dBFS peak, but String/Tube/Mesh ≈ −36/−28/−40 dBFS.
- There was no master ceiling: dense polyphony, and the newly self-oscillating bow driver, could exceed
  full scale.

The constraint that shaped the solution is the [ADR-0014] **energy bus**: the dynamic nonlinearities
(tension, bore steepening, mesh geometric coupling, radiation, the sympathetic send) key off the
*measured resonator energy* (`observe_energy(resonator_output)`). Their references were calibrated
(M11 P8) to the **raw physical vibration level**. Any level staging must therefore not move the signal
the energy bus observes, or it would silently re-scale every dynamic effect.

## Decision

- **Per-family output makeup, applied output-side of the energy tap.** Each resonator family carries a
  fixed makeup (Modal `0.6×`, Tube `12×`, String `32×`, Mesh `49×`) that brings its single full-velocity
  voice to ≈ −6 dBFS peak, so switching family gives a consistent loudness and the instrument sits at a
  usable level. Modal is the loudness reference (its sound is untouched; only its level trims down).
  The makeup is **matched on peak, not RMS** — the families' crest factors differ ~8× (a plucky String vs
  a sustained Modal), so peak-matching keeps any family from clipping while RMS-matching would blow the
  plucky families' transients past full scale.
- **Makeup is applied per-resonator, before the A/B mix; the energy tap stays on the raw mix.** A single
  post-mix makeup over-amplifies a loud+quiet mix (a default Modal+waveguide parallel patch reached RMS
  2.6 because the loud Modal component took the blended makeup). `ResonatorStack::process_sample` returns
  the raw mix (for the energy tap) and stores a `staged_output` — the per-resonator-made-up mix — that the
  voice reads for the audio path. The M2 energy bus is therefore untouched: P8's calibration holds by
  construction.
- **A per-driver trim folds into the makeup.** A self-oscillating bow ([ADR-0030](0030-bow-friction-driver.md))
  is intrinsically ~8× a pluck at the output, so a `Bow` driver scales the waveguide makeup by `0.2×` to
  land a bowed note at a forte level instead of slamming the limiter.
- **A master soft-clip safety stage** (`dsp/master_stage.rs`) runs on the final stereo mix at the runtime,
  after the sympathetic chamber. It is a stateless, lookahead-free, allocation-free per-sample soft
  clipper: the **identity below a −6 dBFS knee** (single notes and quiet passages are bit-exact), then a
  soft knee that asymptotes to a **−1 dBFS ceiling**. There is **no loudness normalization** — the static
  per-family makeup sets the level, and auto-normalizing would squash the M11 P8 dynamic range; only the
  makeup and the safety clip act.

## Consequences

- The energy bus remains the raw physical-amplitude tracker, so the P8 dynamic-effect references need no
  change when the output level moves — staging and dynamics are decoupled.
- Single notes are untouched by the master stage; only dense polyphony rides into the soft knee, gently
  and transparently rather than being level-flattened or hard-clipped.
- The makeup is per-family/per-driver, not a learned or metered normalization, so it is deterministic and
  allocation-free, but it is calibrated to the *current* family levels — a future change to a family's raw
  output level requires re-checking its makeup (guarded by the gated calibration battery).
- One reference observes the post-makeup mix rather than the raw bus: the sympathetic send
  ([ADR-0028](0028-surrounding-effects-and-sympathetic-chamber.md)), recalibrated to the staged level.

## Alternatives

- **Makeup before the energy tap (re-scaling the bus, with the references scaled to match).** Rejected:
  it couples staging to the dynamic-effect calibration and muddies the "energy bus = physical vibration"
  invariant; every level change would ripple into the references.
- **A lookahead brickwall limiter.** Rejected: it adds latency and allocation (a delay line), violating
  [ADR-0001](0001-allocation-free-audio-thread.md); a per-sample soft clipper is transparent enough for a
  final safety stage on an instrument that is already staged below the ceiling.
- **Auto-loudness normalization.** Rejected: it would undo the P8 dynamic range the program exists to
  create.
