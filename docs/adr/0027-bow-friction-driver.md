# ADR-0027: Bow Friction Driver

## Status

Accepted

## Date

2026-06-01

## Context

The additive physical-driver layer ([ADR-0017](0017-additive-physical-driver-layer.md)) shipped the
feed-forward pick and the self-oscillating reed as selectable excitation sources alongside the sample/
sidechain path. The M11 voicing program adds the **bow** — a continuous stick-slip friction driver that
sustains a bowed (Helmholtz) tone on the String waveguide for as long as a note is held, the one sustained
excitation the String lacked.

A bow is a self-oscillator like the reed: it must lock the string loop into a limit cycle from the
negative-resistance region of the friction curve, and it must stop driving on note-off so the string rings
out at its natural decay. Two things had to be settled: the friction model, and — because a self-oscillator
has no fixed output level — the injection calibration and how the bow's level relates to a plucked note.

## Decision

- **An exponential stick-slip friction model.** The friction coefficient is
  `μ(Δv) = μ_d + (μ_s − μ_d)·e^(−|Δv|/v0)` in the relative bow–string velocity `Δv` (Smith / McIntyre-
  Schumacher-Woodhouse lineage): high near zero relative velocity (the string sticks to the bow) and
  falling to the dynamic floor as it slips, so the negative-slope shoulder supplies the energy that
  sustains the motion. The player effort sets the bow normal force; a note-on excitation transient
  kick-starts the motion; the note-state drive gate lifts the bow on release.
- **Three controls, exposed as `0..1` parameters** (M11 P10): pressure depth (effort→normal force), bow
  speed (limit-cycle amplitude/brightness), and friction sharpness (stick-slip transition width). The
  `Driver Type` selector gains a fourth value (`Bow`), and the three controls register and round-trip like
  the reed's.
- **Injection calibrated to a sane forte level (M11 P9).** The bow's limit-cycle amplitude scales with its
  injection gain above a lock threshold (~0.08). The initial gain (4.0) drove the locked cycle to
  energy-bus RMS ~8.7 — far above full scale, slamming everything downstream. It is lowered to **0.12**
  (output clamp **0.5**), which keeps a robust margin above the lock threshold while settling at a forte
  ~0.3 RMS. A self-oscillator's locked cycle cannot be pushed arbitrarily low without un-locking, so the
  bow is intrinsically louder than a pluck; the residual ~8× is handled by a per-driver output trim in the
  gain-staging layer ([ADR-0026](0026-m11-gain-staging.md)), not by starving the oscillator.

## Consequences

- The String gains a true indefinite-while-held sustained voice, distinct from the reed (which sustains the
  bore, not the string).
- The bow is now reachable from the host (a `Driver Type` value plus three parameters); previously the DSP
  existed but no parameter surface exposed it.
- Like the reed, the bow is a stability edge (a self-oscillator inside the 2× loop); it is asserted bounded
  and finite under hard drive, and the master soft-clip ([ADR-0026]) bounds its output downstream.
- The injection and trim are calibrated to the current String level; a change to that level requires
  re-checking the lock margin and the per-driver trim.

## Alternatives

- **A simpler velocity-driven excitation (no friction curve).** Rejected: without the negative-resistance
  region it cannot self-sustain a Helmholtz limit cycle — it would be a one-shot excitation, not a bow.
- **Tame the bow purely by lowering its injection to a pluck-equal level.** Rejected: below the lock
  threshold the oscillation collapses, so the bow would stop sustaining; the residual level difference is
  corrected output-side instead.
