# 0033 — Lamath Stringed bow is a velocity-wave friction contact

- Status: Accepted — supersedes [ADR-0030](0030-bow-friction-driver.md)
- Date: 2026-06-10

## Context

ADR-0030's bow was a one-way excitation: a friction oscillator whose force was multiplied by a
calibrated injection gain and added into the string loop. The P7 render audition rejected it
(runaway crescendo on smooth, silent scratch), and the repair work uncovered structural defects in
the follow-up displacement-penalty contact as well: an ad-hoc contact admittance with the wrong
physical dimension (displacement-per-force-per-sample, making the contact sample-rate dependent),
string velocity estimated by numerically differentiating interpolated displacement reads (corrupted
whenever tension modulation or tuning glides moved the delay), a slip-force fixed-point iteration
that diverges on the falling friction branch (the multivalued Friedlander region), a synthetic
"kick" injection carrying the attack, and an output path that muted the pickup during bowing
without calibrating the body-radiation path that then carried the whole voice.

The physical target (P7): the bow is a contact coupled two-way to one persistent string — it reads
the string's state and writes its reaction back at the same point, with no render-only behavior and
no gain/EQ fixes standing in for physics.

## Decision

- **The contact solves in velocity waves** (`lindelion-string::bow`). The incoming (history)
  velocity at the bow is the sum of the two rail samples at the contact, and a transverse force `F`
  moves the contact velocity by `F·Y_c` while radiating `F·Y_c` outgoing on each rail, with the
  contact admittance **exact and derived, not tuned**: `Y_c = 1/(2·Z₀) = G_s/2 = 0.5` in the wave
  units the bridge junction already defines. All bow constants (speed, slip velocity, force) are in
  normalized wave units sized from the Schelleng cone, so the contact behaves identically at every
  host sample rate.
- **Stick/slip is the Friedlander construction solved by bracketed bisection with the
  McIntyre–Schumacher–Woodhouse hysteresis rule.** Sticking is a velocity constraint feasible while
  the required force stays inside the static cone; sliding brackets the load-line/kinetic-curve
  intersection (fixed scan + bisection counts, allocation-free) and keeps branch memory: a sticking
  contact sticks while it can, a sliding contact slides while a sliding root exists, and stick
  release jumps to the large-relative-velocity branch.
- **History reads are taken upstream and FIFO-delayed back to the contact.** Read at the contact
  itself, the finite-width window plus cubic interpolation stencil overlaps the freshly written
  outgoing field, feeding ~30% of the last force back as fake incoming wave — measured to flatten
  the string's restoring echo from the theoretical `−2r/(1+r) ≈ −1` to ≈ `−0.43`, which leaves a
  false equilibrium inside the friction cone (the bow pins; Helmholtz motion never starts). Reading
  `BOW_READ_ADVANCE_SAMPLES` upstream (where contact writes never land) and delaying through a FIFO
  yields the exact incoming wave with zero geometric error; a regression test pins the measured
  echo factor to theory.
- **The contact keeps a fixed clearance from the string ends in samples**, which is also the
  physical behavior: a bow sits at a fixed distance from the bridge while the stopped length
  shrinks, so the relative bowing point grows on short (high) strings.
- **Played effort scales bow speed and normal force together.** Loudness rides on bow speed
  (Helmholtz amplitude ∝ v_bow); co-scaling the force holds the contact at the same point of the
  Schelleng cone across the dynamic range. The note gate ramps both from zero, so the attack is the
  bow accelerating onto the string — there is no separate excitation injection.
- **While the bow is engaged, the output is the body radiation alone at its own calibration.** The
  pickup tap would expose the contact discontinuity as a dry signal a body would not radiate. The
  violin body voicing is referenced against the Iowa MIS arco recording (mode overlap, transition
  region, bridge hill at 2.4 kHz), and the broadband background radiation is high-passed two-pole
  below each family's lowest mode — a body radiates ~12 dB/oct below its first resonance, and a
  floor flat to DC audibly leaks the bow's mean-drag quasi-DC as rumble in release tails.
- **The sliding friction carries deterministic rosin noise.** The kinetic curve is scaled by
  `1 + depth·noise` (seeded xorshift through a one-pole, slip-phase-gated), producing the
  pitch-synchronous broadband bed a measured bowed tone shows at ≈ −55..−75 dB relative to the
  fundamental. Without it the model is a numerically pure line spectrum and reads as a bell/organ.
- **Scratch is an operating point at the measured chaos boundary**, not a different driver: the
  Schelleng overpressure ratio is the dial, and the periodicity cliff is sharp (measured at the
  shipped recipe's speed/position: fully periodic at pressure 0.54, a raucous pitched note at 0.56,
  unpitched crunch from 0.62). Far over the line the string stops oscillating at pitch entirely.
- **Tension modulation reads the string's own stored energy** (smoothed mean square of the boundary
  samples — an equipartition proxy), replacing the output-RMS energy bus whose pluck calibration
  permanently detuned any sustained driver.

Approximations retained deliberately (documented at their definitions): the exponential
velocity-weakening friction curve rather than thermal/elasto-plastic rosin models; the five-tap
finite-width ribbon window; the reduced modal body with decoupled loading/radiation scales.

## Alternatives considered

- **Keep the displacement-penalty contact.** Rejected: the admittance constant has the wrong
  physical dimension (the force-to-string coupling gains +6 dB/oct of spurious high-frequency
  content), every constant is per-sample (host-sample-rate-dependent sound), and the velocity
  estimate requires numerical differentiation that delay modulation corrupts.
- **Fixed-point iteration for the slip force.** Rejected on measurement: on the falling friction
  branch the iteration's contraction factor exceeds 1 and it oscillates between forces ~3× apart
  forever; the returned value depends on iteration-count parity. Part of the old "scratch" was this
  numerical chatter.
- **Split the waveguide at the bow** (separate nut-side and bridge-side loops, the canonical
  bowed-string architecture). This avoids read/write contamination structurally, but breaks the
  unified two-rail machinery the pluck path, pickup taps, excitation injection, and continuous
  tuning state all share. The upstream-read FIFO achieves the same exactness inside the two-rail
  structure.
- **Thermal (Smith–Woodhouse) or elasto-plastic friction.** Better hysteresis realism, materially
  higher cost and state; the friction-curve form is kept as the documented approximation and the
  solver — not the curve — was the audible defect.
- **Post-render EQ/gain repairs.** Forbidden by the P7 constraints; every level in the bowed path
  is a calibration of a physical transfer (radiativity, friction scale) with a measured reference.

## Consequences

- Smooth bowing and the boundary-point scratch are accepted by audition; the inter-harmonic noise
  bed and harmonic envelope sit within a few dB of the Iowa reference across most of the spectrum.
- The contact is sample-rate independent and renders are deterministic (seeded noise).
- The chaos boundary is pitch-dependent at fixed patch parameters: on the alternating C4–C5 scale
  the scratch notes vary in character and B4 period-doubles to a sub-octave. Torsional contact
  loss (a resistive term in the contact solve, the standard reduction of torsional-wave absorption)
  widens the stable raucous band and is the planned lever.
- The bow cannot be placed closer than the clearance to either end; at very high pitches the
  effective bowing point migrates toward the middle, mirroring a real instrument's fixed
  bow-bridge distance.
- The slip solve costs a bounded ~26 `exp()` evaluations per sliding sample at the oversampled
  rate; sticking samples cost one comparison.
