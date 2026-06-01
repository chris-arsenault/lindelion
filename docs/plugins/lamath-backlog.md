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
- Add cross-resonator coupling where one resonator output can partially feed another resonator input (the inter-slot half of the cross-coupling/sympathetic item; cross-voice sympathetic resonance shipped in M10 as the note-tuned sympathetic chamber, [ADR-0028](../adr/0028-surrounding-effects-and-sympathetic-chamber.md)).
- Add per-voice stereo placement for natural ensemble spread.
- Add microtuning support with Scala or `.tun` import if a product use case needs it.

---

## Shared-Body Idiophone Mode (re-strikable persistent resonator)

**Designed and planned** — [ADR-0031](../adr/0031-shared-body-idiophone-mode.md) records the decision;
the milestone plan is the working doc `SHARED-BODY-IDIOPHONE-PLAN.md` at the repository root.

Model idiophones (mesh cymbal/gong, bell) as a single **persistent, always-resonating body** that
note-ons *re-strike* rather than as a per-note voice. A strike injects an excitation (force from
velocity, pitch, and a strike position) into the **live** body without choking the existing ring;
note-off does nothing — only an explicit damp/choke gesture stops the ring. No sample library does
this (round-robin layers are the giveaway); it is feasible here because the 2D mesh and the modal
bank are real, linear-superposable physical models, not samples.

Confirmed design ([ADR-0031](../adr/0031-shared-body-idiophone-mode.md)):

- **Opt-in `shared_body` patch toggle**, default off and bit-identical to today's per-voice behavior;
  on promotes the idiophone resonator to a runtime-owned persistent body. Both behaviors coexist.
- **Mesh and modal idiophone families**, mirroring the patch resonator stack; waveguide (string/tube)
  slots are unaffected and stay polyphonic-per-voice.
- **Push strike events** (force/pitch/excitation/position) into the body via an allocation-free
  injector pool; strikes do not consume voice slots.
- **Pitch retunes the live body per strike, ring-preserving** (the existing state-preserving `retune`
  path), so the body is melodically playable.
- **Explicit damp via a configurable key-switch range**; note-off does nothing.
- **The body's own decay is the envelope** (the per-note amp envelope steps aside); the body gets its
  own energy follower and gain staging, summed at runtime scope like the M10 sympathetic chamber.

- **Today** is voice-per-note: each `Voice` owns its own resonator, so N held notes = N separate
  bodies. `retrigger_resonators` (default off) already preserves a *reused* voice's ring on a
  same-note re-hit-after-release — the seed of this — but it is not a shared body: held re-hits and
  different pitches spawn separate resonators, and the per-note amp envelope re-shapes the output.
- **Mechanism:** promote the idiophone resonator to a runtime-owned shared singleton fed by strike
  events from the note stream; strings/tubes stay polyphonic-per-voice. The M10 sympathetic chamber
  (`plugins/lamath/src/dsp/sympathetic_chamber.rs`,
  [ADR-0028](../adr/0028-surrounding-effects-and-sympathetic-chamber.md)) is the existing
  precedent — a persistent, steal-retuned shared resonant layer. The amp envelope must get out of
  the way for this mode (sustain = 1, no per-note release; the body's own decay is the envelope).
- **Sequencing:** built after the M11 voicing program (P2 gave the mesh a real multi-second ring;
  P4/P7 give it body coloration and register behavior). `feature-start` has been run; see
  [ADR-0031](../adr/0031-shared-body-idiophone-mode.md) and the root milestone plan
  `SHARED-BODY-IDIOPHONE-PLAN.md`.

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

## M11 Realism Polish (deferred from the voicing program)

Deferred from the M11 voicing program (`PHASE-M11-PLAN.md`) once P1–P4 left the resonators
*functioning correctly* (every family rings, sustains, holds tune, and the four are timbrally distinct).
These add **aliveness, not correct function**, and each adds real-time CPU cost the instrument may not be
able to spare. Revisit as opt-in polish **after** the instrument is level-staged, control-calibrated, and
confirmed real-time; gate any reinstatement on the per-voice CPU budget.

- **Dual-polarization string (was P5):** two slightly-detuned, coupled traveling-wave pairs (horizontal/
  vertical polarizations) for natural beating and the two-rate decay (fast attack + long aftersound) of a
  real string/piano. Roughly **doubles the String waveguide cost** (a second delay-line pair + its own
  dispersion). A waveguide architecture change; would need an ADR for the polarization model.
- **Micro-imperfection / aliveness (was P6):** a slow pitch drift/vibrato route (the per-note pitch is
  frozen today) plus a sustained noise bed (extend the M10 attack-only noise to a continuous bow/breath/
  air component). Defeatable.
- **Register-aware voicing (was P7):** per-played-note scaling of decay/brightness/inharmonicity across
  the keyboard (bass longer/darker/more inharmonic; treble shorter/purer/brighter). Voicing, not
  function — useful for keyboard consistency, deferrable to ear-tuning.

## Real-Time / CPU Budget (gating concern)

The physical-model chain is heavy and may already be near or over the real-time budget — a concern raised
during M11. Before the remaining tuning work (and before any of the deferred realism above), measure
per-voice cost (`make bench`) for each family at 1–4 voices and trim the hot stages. Known cost added
during M11: the **P4 8-stage dispersion cascade** (~16 first-order allpass ops per sub-sample per String,
on top of the 2× oversampling); measure whether it pays for itself and reduce the stage count/scope if
not. ADR-0001 already guarantees no audio-thread allocation; this is about raw per-sample cost, which it
does not cover.

---

## Product And Storage Decisions

- Add recommended default mode counts per modal template while preserving user override.
- Decide whether sample ingest should normalize stored excitations to mono 48 kHz FLAC.
- Add an explicit UI workflow for live versus offline render quality settings.

---

## UI And Workflow Polish

- Expand user-facing feedback for audio-created notes, sidechain input state, and voice ownership if Ableton validation shows ambiguity during normal use.
- Revisit realtime pitch tracker quality only if Apple Silicon host validation shows the current shared detector is the quality bottleneck.
