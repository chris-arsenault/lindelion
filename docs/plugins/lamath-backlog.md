# Lamath - Backlog

This file tracks work that is not part of the current implemented Lamath spec in [lamath.md](lamath.md). It keeps deferred validation, product expansion, and unresolved design decisions out of the current implementation document.

---

## External Validation

- Run Steinberg validator against the macOS bundle after bundle-affecting changes.
- Confirm Ableton scans Lamath, loads it as an instrument, opens and closes the editor repeatedly, and saves/reloads project state.
- Validate the optional sidechain path in Ableton with the sidechain routed and unrouted.
- Record Apple Silicon latency and CPU numbers for the all-enabled sidechain path.

---

## Whole-System Dynamic Response

Tracked in full by [ADR-0014](../adr/0014-dynamic-response-effort-energy-bus.md) through
[ADR-0017](../adr/0017-additive-physical-driver-layer.md) and the working plan at the repository
root (`DYNAMIC-RESPONSE-PLAN.md`).

- Model Lamath as a driver → coupling → resonator → two-way body → surrounding chain in which a
  shared effort/energy bus changes timbre across the whole instrument with playing dynamics.
- Hoist the per-sample linear operator derivations into a control-rate prepared model so the
  realtime path has the budget for a nonlinear core.
- Add a per-sample resonator-energy follower and a hybrid effort/energy modulation bus.
- Add a shared 2x oversampled, allocation-free nonlinear inner loop for the resonator core.
- Add energy-dependent string tension modulation, finite-amplitude bore steepening, and geometric
  (von Karman) mesh nonlinearity.
- Replace the heuristic biquad body with a two-way-coupled reduced modal body (signature modes, an
  explicit air/Helmholtz mode, and a broad formant for the modal-overlap region).
- Add a force-dependent physical driver layer (reed/lip/bow/pick/hammer) as a selectable source
  alongside sample and sidechain excitation.
- Add a coupling/contact stage (contact time, slip, strike-position spread) and the dynamic
  source-versus-body balance.
- Add effort-scaled surrounding effects: pick/breath/key mechanical noise, sympathetic resonance,
  and radiation brightening.

## Resonator And DSP Extensions

- Add a bidirectional or two-port waveguide for more accurate tube behavior, including closed-end reflection and clarinet-like response.
- Add a plate or membrane resonator model as a third resonator slot type.
- Add a banded waveguide model for bowed or glass-like timbres.
- Add cross-resonator coupling where one resonator output can partially feed another resonator input (the inter-slot half of the cross-coupling/sympathetic item; cross-voice sympathetic resonance shipped in M10 as the note-tuned sympathetic chamber, [ADR-0025](../adr/0025-surrounding-effects-and-sympathetic-chamber.md)).
- Add per-voice stereo placement for natural ensemble spread.
- Add microtuning support with Scala or `.tun` import if a product use case needs it.

---

## Shared-Body Idiophone Mode (re-strikable persistent resonator)

Model idiophones (mesh cymbal/gong, bell) as a single **persistent, always-resonating body** that
note-ons *re-strike* rather than as a per-note voice. A strike injects an excitation (force from
velocity, position from pitch and/or a strike control) into the **live** body without choking the
existing ring; note-off does nothing — only an explicit damp/choke gesture (hi-hat pedal, hand mute)
stops the ring. Pitch becomes strike location/excitation, not a retuned per-voice resonator. No
sample library does this (round-robin layers are the giveaway); it is feasible here because the 2D
mesh is a real physical model, not samples.

- **Today** is voice-per-note: each `Voice` owns its own resonator, so N held notes = N separate
  bodies. `retrigger_resonators` (default off) already preserves a *reused* voice's ring on a
  same-note re-hit-after-release — the seed of this — but it is not a shared body: held re-hits and
  different pitches spawn separate resonators, and the per-note amp envelope re-shapes the output.
- **Mechanism:** promote the idiophone resonator to a runtime-owned shared singleton fed by strike
  events from the note stream; strings/tubes stay polyphonic-per-voice. The M10 sympathetic chamber
  (`plugins/lamath/src/dsp/sympathetic_chamber.rs`,
  [ADR-0025](../adr/0025-surrounding-effects-and-sympathetic-chamber.md)) is the existing
  precedent — a persistent, steal-retuned shared resonant layer. The amp envelope must get out of
  the way for this mode (sustain = 1, no per-note release; the body's own decay is the envelope).
- **Sequencing:** build after the M11 voicing program (P2 gave the mesh a real multi-second ring;
  P4/P7 give it body coloration and register behavior), then run `feature-start` on it.

### M11 compatibility (scanned P3–P10): no hard lock-outs

The M11 voicing program does not cement the voice/resonator architecture further — it tunes DSP that
moves with the resonator whether it is per-voice or shared. Things to keep in mind when M11 runs:

- **P7 (register-aware voicing) — watch this one.** The shared-body model makes the mesh a
  *fixed-tuning struck body* (strike position = timbre), not a per-note-retuned pitched resonator, so
  per-note Mesh decay/brightness/inharmonicity scaling would be partly *replaced* (not blocked).
  Keep register-voicing a per-family function applied at trigger-config; do not over-invest in
  per-note Mesh tuning. Strings/tubes register-voicing is unaffected and correct.
- **P3 (drive-gate / note-off):** keep the resonator decoupled from a hard per-note amp choke (a
  dedicated drive release with the natural ring as the tail) — already the recommended direction,
  and it aligns with the no-choke body.
- **P6 (micro-imperfection):** for the mesh, make aliveness body-intrinsic, not per-note-pitch.
- **P9 (gain staging):** the per-voice `Voice::stage_peaks` taps would not cover a runtime-owned
  shared body; it would need its own staging (as the M10 chamber already does). Additive, not
  blocking.

---

## Product And Storage Decisions

- Add recommended default mode counts per modal template while preserving user override.
- Decide whether sample ingest should normalize stored excitations to mono 48 kHz FLAC.
- Add an explicit UI workflow for live versus offline render quality settings.

---

## UI And Workflow Polish

- Expand user-facing feedback for audio-created notes, sidechain input state, and voice ownership if Ableton validation shows ambiguity during normal use.
- Revisit realtime pitch tracker quality only if Apple Silicon host validation shows the current shared detector is the quality bottleneck.
