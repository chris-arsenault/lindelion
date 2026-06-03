# 0032 — Lamath Tube is a driven wind voice (no struck/silent tube)

- Status: Accepted
- Date: 2026-06-02

## Context

The Lamath render-catalog audition (`review/lamath-render-catalog-comments.json`) found the
**Tube** resonator silent in every non-reed configuration: baseline, register, and contact Tube
cases at all velocities produced "no audible sound."

The cause is physical, not a bug in the bore. A tube/bore is a **wind resonator** — a passive set
of standing-wave modes with no energy of its own. It does not ring when struck; it sounds only when
continuously driven by a reed, lips, or air-jet in a feedback loop with the bore (the
McIntyre–Schumacher–Woodhouse model). The shipped Tube, however, defaults to the pass-through
`Sample` driver, so a note-on injects only the builtin 64-sample impulse into a closed,
inverting, quarter-wave bore, which rings down inaudibly in tens of milliseconds. No parameter is
audible because there is no sustained standing wave to shape. The MSW reed driver
([ADR-0017](0017-additive-physical-driver-layer.md)) already exists and is two-way coupled to the
bore, but it is opt-in and voiced far past its oscillation regime (a large injection gain and
max-pressure with a squared-effort mapping), so when selected it produces a permanently-beating
high-pitched scream rather than a played tone.

The audio-quality suite passed the silence because its Tube tests exercised only non-default
operating points (positive boundary reflection, raised loop gain) on isolated cores, while the
full-synth battery gated audibility on `rms > 0` and treated an unmeasurable decay (`T60 == None`)
as a pass — so a silent Tube satisfied every assertion.

## Decision

The **Tube resonator family is always wind-driven.** Selecting `WaveguideStyle::Tube` engages a
reed/breath driver; the struck/impulse drivers (`Sample`, `Pick`) and the `Bow` are not valid for a
Tube and are normalized to the wind driver when a patch is loaded. The instrument therefore cannot
hold a silent Tube configuration.

- **Reed regime.** The reed is voiced onto the canonical MSW/STK operating range: silent below the
  oscillation threshold, a clean pitched tone just above it, brightening as breath pressure rises,
  and squeak/overblow reached only at the top of the range. Exact constants are tuning-phase work,
  grounded in the cited reed-feedback literature and guarded by stability tests across register and
  drive.
- **Sustain.** A held note blows continuously and stops shortly after note-off — the Tube is the one
  sustaining Lamath family (Modal/String/Mesh remain struck/decaying).
- **Termination.** The bore termination (open ↔ closed: bright/all-harmonic ↔ woody/odd-harmonic)
  is retained as the primary timbral control but **narrowed to the audibly-useful band**, so every
  position of the control makes a clearly different sound.
- **Excitation layers.** The layered-excitation model is retained for the Tube as **breath /
  articulation injection** — the tongued attack and optional breath texture that kick-start and
  color the reed — rather than a struck pulse. Velocity layers become articulation layers.

This decision instantiates a general principle, recorded as a critical rule in `AGENTS.md`: **the
instrument does not ship configurations that produce no sound.**

## Alternatives

- **Keep the Tube struck and raise its level.** Rejected: a struck bore is physically a faint click
  regardless of gain; boosting it amplifies a transient, not a tone, and leaves every Tube parameter
  inaudible.
- **Default the Tube to the reed but keep struck drivers selectable.** Rejected: this re-admits
  silent Tube+Sample/Pick/Bow combinations into the shipped patch space — the exact defect being
  fixed.
- **Give the bore an internal self-excitation source.** Rejected: a bore has no internal energy
  source; the reed/breath *is* the source, and a correct two-way-coupled one already exists.

## Consequences

- Tube + Sample/Pick/Bow combinations are removed from the valid patch space. Legacy patches and
  sessions naming those combinations normalize to the wind driver on load — they load to an audible
  Tube, never to silence.
- **Drivers are per-resonator.** The patch carries a driver for each resonator slot (`driver`,
  `driver_b`), so the Tube ⇒ reed normalization is scoped to the slot that holds the Tube and never
  converts a co-resident non-Tube waveguide. `driver_b` is a `#[serde(default)]` field, so older
  single-driver patches still deserialize (slot B defaults to the transparent driver). The host
  parameter surface maps to slot A's driver; slot B's driver is patch-controlled.
- The reed re-voicing recalibrates its operating constants; values are tuning-phase work but bounded
  by stability/finite tests across register and extreme drive (per
  [ADR-0016](0016-oversampled-nonlinear-inner-loop.md), the reed runs in the existing 2x oversampled
  loop, and per [ADR-0001](0001-allocation-free-audio-thread.md) the audio path stays
  allocation-free).
- A full-synth, **shipped-default** perceptual test proves the driven Tube sustains audibly and that
  its parameters audibly change the tone — closing the suite gaps that let the silence ship.
- The render catalog's silent Tube cases are deleted and replaced with driven-Tube demonstrators:
  the now-audible baseline/register/reed cases plus a C-major-scale **articulation** group (tongued
  vs slurred).
- **The reed terminates the bore mouth.** Voicing M1 revealed that a struck-style reed (injected as
  a source at the strike position while the bore kept its own passive −0.36 mouth reflection) cannot
  hold a clean pitch — two terminations fight and the loop period-doubles to sub-harmonics. The reed
  is therefore wired as the *mouth boundary itself*: it reads the bore's returning wave, solves the
  scattering junction (the implicit `u = g(breath − 2·p⁻ − Z·u)`, Smith PASP), and its scattered wave
  *replaces* the passive mouth reflection (the mouth loss filter stays in the loop, so the tuning
  holds; the energy-driven steepening also stays in the loop, though it does not yet produce a strong
  brightening — see the deferred note below). The reed flow itself is the physical beating reed —
  opening closing to zero at a closing pressure, orifice (Bernoulli √) flow. With this the wind tube
  locks to its tuned fundamental across the C2–C6 register. Implemented as a Tube-only wind path
  (`process_sample_wind`); the struck/pick/bow strike-injection path is bit-unchanged.
- **Monophonic, with articulation as note-change behavior.** A reed Tube is a one-note-at-a-time
  wind instrument, so a Tube patch forces `polyphony = 1` on load (the editor should later *lock*
  this rather than override it silently — see the backlog). Articulation is therefore how the single
  voice changes notes: a **tongued** note is separated (the reed goes silent in the gap, then
  re-onsets with the excitation kick); a **slurred** note overlaps (the reed keeps blowing and the
  bore frequency glides to the new pitch). The breath ramps in (~4 ms) and the frequency runs through
  an 8 ms glide, so onsets and slurred changes don't click.
- **Gain-staged and warmed.** The reed self-oscillates far hotter than the struck excitation the
  per-family makeup was calibrated for, so a reed-driver output trim brings it to the matched forte
  level instead of slamming the master clipper into a uniform bit-crushed saw. The bore's odd-harmonic
  spectrum is voiced via `loop_filter_cutoff`.
- **Accepted divergences / deferred:** the termination range is *physically* self-narrowing (an open
  bell radiates too much for the reed to sustain, so only the closed-bell band oscillates) rather than
  param-clamped; brassiness-with-effort is weak (the velocity dynamic is largely timbral via the
  pressure window, not a strong cuivré brightening) and is left to the backlog; the top octave reads a
  few cents flat. The output is an odd-harmonic square rather than a fully formant-shaped clarinet —
  usable, with brightness filterable downstream.
- Scoped to the Tube. Modal/String/Mesh are unchanged; the broken Bow driver and the broader
  perceptual-test-layer overhaul are tracked separately.
