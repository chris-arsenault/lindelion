# 0049 — Lamath Tube register-key voice: mode-choked vent, source-fed body, tracked embouchure

- Status: Accepted
- Date: 2026-06-10

## Context

ADR-0032's driven wind voice gave the Tube a working low register, and a first register-key
model (a resistive side-hole shunt at one third of the bore, mode ratio 2.88) made notes above
the break speak on the third bore mode. Reference auditions against the owner clarinet fixtures
then showed the vented register failing on every axis at once: the body color band (1.2–2 kHz,
the clarinet's defining h3 region) was 14 dB under the reference; a breath wash buried the voiced
lines 23 dB closer than the reference's −44.6 dB voiced-to-breath ratio; the attack took over a
second to bloom (reference: 70 ms), making short notes chirp; notes above C5 did not speak at
all; and the sustained pitch sat flat by an amount the bore tuning could not explain.

Tap-level and kill-switch measurement attributed each symptom to a distinct physical-model gap,
and each fix was auditioned as a paired A/B render before being baked in.

## Decision

The vented register is a coordinated set of register-scoped mechanisms, all gated on the
`Register Break` note and inert below it (the approved low register is bit-identical):

- **Mode-choked vent shunt.** The vent stays the proven resistive shunt everywhere except a
  narrow zero-phase-at-center notch at the played mode (the register chimney's anti-resonance,
  Q 2 bandpass subtraction in the shunt branch). The bore fundamental keeps its full resistive
  loading — which is what makes the reed abandon the low register — while the played mode keeps
  its oscillation margin. With the mode unloaded the bore speaks its natural third mode, so the
  sounding-to-bore ratio is the measured 2.994 rather than the resistive model's drag-fitted
  2.88, and the register's chronic flatness disappears.
- **Source-fed register body admittance.** The vented bore's standing wave carries no sounding
  h3 (three times the note is not a bore mode), so the register's body color radiates from the
  reed source spectrum instead, as a real clarion does through its open-hole lattice: a cascaded
  fourth-order bandpass at 3f plus a fourth-order band window over the h4–h7 reed-character
  lines, both radiation-only (the body's reaction onto the bore keeps its low-register gains).
  The chalumeau-only even-harmonic machinery (h4 notch, bore-period odd-mode projection comb) is
  released above the break, where the vent breaks the even-cancelling symmetry.
- **Coherent radiation.** Above the break, the radiating paths (reed slot, register color) take
  the reed's coherent source — the in-loop breath-noise dither is scaled to zero (its period-2
  stability job is a low-register concern) and the shed-jet turbulence stays in the bore where
  the bore filters it, instead of being re-emitted raw as hiss.
- **Tracked embouchure.** The reed aperture resonance floors at three times the sounding pitch
  above the break, pinning the aperture's phase lag — and therefore the reed's pumping gain —
  at its A4 value up the register. Without it the margin crosses zero just above C5 and the
  upper register is silent. A fitted lift-ratio phase trim in the reed compensation keeps the
  tracked notes on the A440 grid.
- **Tongue-release attack overpressure.** Every vented note-on (including legato changes)
  transiently drives the effective blowing pressure toward 0.75, decaying to the patch pressure
  with a 70 ms time constant and hard-ceilinged at 0.78 — under the measured 0.85 choke cliff —
  so the bloom matches the reference's ~70 ms instead of ~200 ms.
- **Fingering persists through release.** The register state (vent, bore ratio, embouchure
  tracking) is held by the last played note, not the held-note state, so note-off only stops the
  breath and the vented bore rings down without a topology snap.

## Alternatives considered

- **Frequency-flat resistive shunt (the prior model)** — selects the register mode reliably, but
  its ~4 dB per-transit loss at the played mode leaves so little margin that the note blooms in
  over a second and dies entirely above C5. Kept as the f0-loading core; superseded at the mode
  by the choke notch.
- **Inertive (one-pole) vent, the textbook small-hole model** — frees the mode, but any
  minimum-phase rolloff drags ~45° of phase into the bore fundamental's loading, weakening and
  detuning its suppression: the low register speaks instead. Measured and rejected; only a
  zero-phase-at-center notch frees the mode without touching f0.
- **A second register vent for the upper range** — the real-instrument analogy for the C5+
  collapse, but inapplicable here: the model's bore scales per note, so the single vent's
  relative position stays mode-ideal, and the silent renders showed the vent doing its job. The
  collapse was reed pumping gain, fixed by embouchure tracking.
- **Per-note cents tables / output EQ for pitch and tone** — rejected throughout; every fix is a
  mechanism in the model, with at most smooth fitted corrections (the lift-ratio phase trim) in
  the same style as the existing warm-bore tuning ramp.

## Consequences

- The vented register A4–G5 plays within ±2.4 cents of the A440 grid at consistent levels, with
  65–75 ms attacks, reference-level voiced-to-breath ratio, and the reference's h3/h4/h5
  reed-character lines. Concert C6 and above speaks but is altissimo-grade in tuning.
- The register voice is calibrated as a system: the source-fed radiation gains are tied to the
  oscillation operating point, so loop-margin changes (vent, reed, pressure) require re-measuring
  them against the reference fixtures.
- The breath character above the break is now deliberately clean; reintroducing audible breath
  is a future source-side mechanism (shaped breath, `docs/backlog.md`), not a return of raw
  dither radiation.
- Soft-blown vented notes (velocity ≈ 40) still sit below the oscillation threshold — the
  remaining register defect tracked in `docs/backlog.md`.
- The audition A/B handles used to land these mechanisms were removed once the model was
  approved; re-deriving comparison points requires git history, not a patch switch.
