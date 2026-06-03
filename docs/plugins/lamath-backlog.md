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
[ADR-0017](../adr/0017-additive-physical-driver-layer.md).

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

## M11 Realism Polish (deferred from the voicing program)

Deferred from the M11 voicing program once P1–P4 left the resonators
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

## Driven Wind Tube (post-M1)

The driven wind Tube voice ([ADR-0032](../adr/0032-lamath-tube-driven-wind-voice.md)) shipped with
two constraints applied silently by patch normalization (reed driver forced, polyphony forced to 1)
and a placeholder excitation. These make those constraints explicit, playable, and better-voiced.

- Surface the Tube's forced settings in the editor instead of silently overriding them: when a slot
  is a Tube, the polyphony control is **locked to 1 (mono)** and the driver is **locked to the reed**
  in the UI, with a tooltip explaining why, rather than the patch quietly rewriting the values on
  load. (Today `normalize_drivers_for_resonator_models` overrides both without user-visible feedback.)
- Make wind articulation a bound, playable control: a **key-switch range and/or an automatable
  parameter** selects tongued vs slurred phrasing (and any future articulations) so a player can
  change articulation live, rather than it being implicit in note timing only.
- Ship a **better default reed excitation layer** than the generic builtin impulse: a tuned breath/
  tongue onset sample (or shaped noise burst) so a fresh Tube patch has a musical chiff out of the
  box rather than the shared struck-instrument builtin.
- Strengthen **brassiness-with-effort**: today the velocity dynamic is largely timbral via the
  pressure window and the energy-driven bore steepening does not reliably brighten a loud note. Give
  the Tube a clear cuivré bloom (a brighter, edgier fortissimo) as a guarded A/B axis.
- Give the Tube a more **formant-shaped, clarinet-like spectrum** rather than the current odd-harmonic
  square — bore resonances coloring the output into formants, so register and termination read as
  bigger timbral changes — if downstream filtering proves insufficient.
