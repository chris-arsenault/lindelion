# ADR-0021: String Two-Way Body Coupling and Output Blend

## Status

Accepted

## Date

2026-05-31

## Context

Milestone M7 of the dynamic-response program ([ADR-0014](0014-dynamic-response-effort-energy-bus.md))
replaces the String waveguide's body model. Before M7 the body was a one-way **post-EQ**: a bank of
cached biquads applied to the resonator pickup. That cannot reproduce what a real instrument body does
— it only colours the output and never changes which modes the string sustains. The program's
confirmed decisions require the body to be "two-way coupled, never a post-EQ," and the M7 plan stated
the radiated sound is "the body's surface motion, **not the string pickup**."

Two things surfaced during execution that this record captures, because they are lasting design
decisions with no existing ADR home (M4/M5/M6 implemented [ADR-0014]/[ADR-0016] without new
architecture; the body had no ADR of its own):

1. How the body couples to the string loop.
2. What the String actually radiates — which deviates from the planning-time "not the string pickup"
   statement.

## Decision

- **Two-way coupling via a passive wave-digital (WDF) bridge.** A reduced modal body — a handful of
  signature modes, an air/Helmholtz cavity mode, a broad formant, plus a small flat background
  admittance — is the bridge admittance. The bridge reflection is solved as a passive WDF junction
  (`|R| ≤ 1` at any mode Q), so the resonant body loads the string loop (partials near a body
  resonance lose energy and decay faster) and **never destabilises** it. This replaces the heuristic
  post-EQ entirely. Two voicings ship (guitar, violin).
- **Output = body radiation summed with a string pickup tap.** Contrary to the planning-time "not the
  string pickup" framing, the String output blends the body's radiated motion with a pickup-position
  tap of the string. The two-way loop loading colours **both** terms (the pickup reads the
  body-loaded loop), so the body's decay signature is present regardless of the mix.

## Consequences

- **The body absorbs energy.** A body-coupled note sustains measurably less than a bare string,
  especially in the body's dense low modal range — physically correct, and a deliberate change from
  the old post-EQ. The bare-loop `loop_gain → T60` calibration no longer holds exactly on the String;
  decay is co-determined by the body. Tests assert body absorption rather than bare-loop matching.
- **`pickup_position` stays a material control and loudness is preserved.** Pure body-radiation output
  was ~10x quieter than the old pickup-EQ and made `pickup_position` inert; blending the pickup tap
  back keeps the control meaningful and the String at parity with the other resonators. The body's
  flat background admittance is therefore kept small (it no longer has to carry the broadband pitch —
  the pickup does), which also keeps it from over-damping the whole string.
- The two-way body adds a small frequency-dependent bridge phase, so String tuning tapers slightly in
  the very top octave (where the half-wave loop is only a few samples long).
- Cross-resonator loudness balance and the body coupling depth (`BODY_GAIN_SCALE`, background
  admittance) are first-principles values to be re-confirmed in the M11 calibration pass.

## Alternatives

- **Pure body-radiation output (no pickup tap).** The most literal reading of "the radiated sound is
  the body, not the pickup." Rejected: ~10x quieter, and it makes `pickup_position` inert for the
  String. Retained in spirit — the body radiation is still a real term in the blend.
- **Keep the one-way post-EQ body.** Rejected by the program's confirmed decision: a post-EQ cannot
  change which modes the string sustains (no wolf-note, no faster near-mode decay).
- **Make the body the string's mouth/bridge boundary outright (no excitation injection).** Heavier
  waveguide surgery; the passive WDF bridge already loads the loop two-way without it.
