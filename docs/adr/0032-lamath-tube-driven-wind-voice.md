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
  param-clamped; the top octave reads a few cents flat. The output is an odd-harmonic square rather
  than a fully formant-shaped clarinet — usable, with brightness filterable downstream.
- Scoped to the Tube. Modal/String/Mesh are unchanged; the broken Bow driver and the broader
  perceptual-test-layer overhaul are tracked separately.

## Update — item B: dynamics and brightness-with-effort (2026-06-03)

The originally-deferred weak brassiness-with-effort was investigated by measurement (a γ-sweep
characterising the reed map through the full synth). Findings:

- **The reed's timbre is nearly blowing-pressure-invariant.** The beating-reed limit-cycle
  amplitude saturates (~5 dB across the usable γ range) and its harmonic content (h3≈0.42, h5≈0.16)
  is essentially flat with γ — so playing harder changed neither level nor brightness. This is why
  the velocity dynamic was inaudible: the energy-gated bore steepening was referenced to a measured
  energy that itself barely moved.
- **The "overblow above CEIL" that justified the narrow pressure window was an autocorrelation octave
  artifact**, not a real instability: an odd-harmonic tone is anti-periodic at T/2, so the
  autocorrelation pitch estimator picks the 2T lag and reports f/2. A direct DFT confirms the reed
  holds the true fundamental across the whole low/mid register at any blowing pressure.

The attempted fix (the chosen "effort-referenced physical steepening" direction): the reed wind path
drives the existing finite-amplitude bore steepening and bell radiation from the **player effort
(blowing pressure)** — the physical drive — instead of the amplitude-saturated measured-energy bus
(`set_brightness_effort`, `steepening_energy == effort²`). The reed pressure window is also widened
modestly (now that the false overblow constraint is gone) for a ~10 dB level dynamic. By the offline
metrics this looked like success (centroid ≈2460→3900 Hz at C4 mf→ff). **Audition says otherwise — see
the correction below.**

- **Altissimo squeak is kept on purpose.** The top octave genuinely period-doubles when overblown at
  fortissimo (a *real* DFT subharmonic, not the artifact). This is physically accurate reed behaviour
  and is a desired degeneracy — not clamped away. The pitch guards assert a clean fundamental only in
  the low/mid register; the altissimo is guarded for audibility and boundedness, not pitch.
- **Pitch metric corrected.** The autocorrelation-based tube pitch tests are replaced with a DFT
  f/2-subharmonic check that does not octave-error on the odd-harmonic spectrum (the same T1 trap that
  let the original silence ship).

### Correction (audition) — the effort-brightness direction is wrong; bell+bore redesign pending

WAV audition of the velocity ladder and a bell on/off A/B (`tube_dynamics` render group) overturned the
metric-based success above. The offline centroid/DFT measures **did not distinguish a square from a
clarinet** and are not to be trusted for this — audition is the arbiter (the standing T1 lesson).

- Across velocity the voice reads as **louder, not brighter** — no musical cuivré.
- The **bell HF-radiation tap is the dominant tone problem**: bell **on** = a square wave, bell **off**
  = a (digital) clarinet — an exceptional difference, with peak/RMS wildly different too. The tap is
  not energy-conserving (it reflects the bell-incident wave into the loop *and* re-emits a high-pass of
  it at gain > 1, gated by effort²), so at effort it builds the loop into a square. Item B therefore
  brightens loud notes *by squaring them* — the opposite of a cuivré.
- The reed's own spectrum is blowing-pressure-invariant, so an effort-gated output EQ can never be a
  real cuivré; brightness must come from the **source** (the reed beating duty cycle shifting with
  blowing pressure).

**Real fix (planned, not yet done):** redesign the bell + bore termination together — an
energy-conserving open end (reflected = low-pass kept in the loop, radiated = the *complementary*
high-pass to the output, unity gain) plus brightness generated at the reed source — then formant/body
coloration so the result reads warm. A `bell_radiation` patch scale (`0..1`, default `1.0`) was added as
the audition/diagnostic A/B handle for the current tap. Tracked in `LAMATH-TUBE-DEFICIENCIES.md`.

- **Still deferred:** a fully formant-shaped (vs odd-harmonic-square) spectrum; the few-cents
  high-register flatness; an agile reed onset chiff and shaped (vs white-noise) breath.
