# Shared-Body Idiophone Mode — Implementation Plan

Promote Lamath's idiophone resonator (2D mesh, modal bank) to a single **runtime-owned, persistent
body** that note-ons *re-strike* instead of allocating a per-note voice, behind an opt-in patch
toggle. A strike injects excitation (force from velocity, pitch, strike position) into the live body
without choking the existing ring; pitch retunes the body per strike (ring-preserving); note-off does
nothing; an explicit key-switch range damps the ring. The per-note amp envelope steps aside — the
body's own decay is the envelope. Out of scope: the per-voice string/tube path (unchanged), new
resonator models, and any change to default (`shared_body = false`) behavior.

Durable decision record: [ADR-0031](docs/adr/0031-shared-body-idiophone-mode.md). This plan is a
temporary working doc; it is the single source of truth for execution until the feature ships, then it
is deleted and the durable surfaces (ADR, spec, CHANGELOG) carry the record.

## Confirmed decisions

1. **Opt-in toggle.** New `shared_body: bool` on the patch, default `false`. Off is bit-identical to
   today's per-voice idiophone behavior; on promotes the idiophone resonator to the shared body. Both
   coexist.
2. **Mesh + modal idiophone families.** The shared body mirrors the patch's idiophone resonator stack
   (Mesh, Modal). Waveguide (String/Tube) slots are unaffected by the toggle and stay
   polyphonic-per-voice; the toggle no-ops for non-idiophone slots.
3. **Pitch retunes the body per strike, ring-preserving.** Each strike retunes the live body to the
   note via the existing state-preserving `retune` path (no buffer clear), then injects. Melodically
   playable; last strike wins the tuning while the prior ring keeps propagating.
4. **Explicit damp via a configurable key-switch range.** Notes in the range damp the body (ramp to
   silence); notes outside strike. Note-off does nothing to the ring.

## Context / reuse map

Source-of-truth files the executor re-derives reference behavior from:

- **Precedent — persistent, runtime-owned resonant layer:** `plugins/lamath/src/dsp/sympathetic_chamber.rs`
  + its wiring in `plugins/lamath/src/runtime.rs` (`ResonatorProcessor` ownership;
  `run_sympathetic_chamber`; summing order engine → chamber → master in `process_with_runtime_input`).
  [ADR-0028](docs/adr/0028-surrounding-effects-and-sympathetic-chamber.md). **Reuse the ownership and
  preallocation pattern; replace its pull feed with a push strike queue.**
- **State-preserving retune (the core of decision 3):** `ResonatorEngine::configure` /
  `try_configure_preserving_state` / `retune` in `plugins/lamath/src/dsp/voice/resonator_stack.rs`.
  This already retunes Modal/Waveguide/Mesh without clearing state when `reset_state = false`. **Reuse
  as-is for the per-strike retune.**
- **Resonator stack to mirror:** `plugins/lamath/src/dsp/voice/resonator_stack.rs` (slots A/B, routing,
  `process_sample(excitation, energy, effort, drive_gate)`, `staged_output`). The shared body holds one
  persistent instance of this stack restricted to idiophone families. **Reuse-but-relocate** from voice
  scope to runtime scope.
- **Mesh + modal models:** `plugins/lamath/src/dsp/waveguide/mesh_2d.rs` (+ `mesh_2d/runtime.rs`),
  `plugins/lamath/src/dsp/modal.rs`. Strike = scalar excitation injected at a pre-baked Gaussian
  strike-position weight; idiophone presets Bell/GlassBowl/MetalBar. **Reuse as-is.**
- **Excitation playback (the injector pool source):** `plugins/lamath/src/dsp/excitation.rs` +
  `VoiceExcitation` in `plugins/lamath/src/dsp/voice/mod.rs` (per-voice f32 cursor, slot selection,
  round-robin). **Reuse the cursor/selection mechanics in a runtime-owned injector pool.**
- **Energy follower + gain staging:** `plugins/lamath/src/dsp/energy_follower.rs`,
  `plugins/lamath/src/dsp/master_stage.rs`, and per-family makeup
  ([ADR-0029](docs/adr/0029-m11-gain-staging.md)). The body needs its own follower + staging (the
  per-voice `stage_peaks` taps do not cover a runtime-owned body). **Reuse the follower; add body-scoped
  staging.**
- **Strike force + pitch derivation:** `Voice::trigger` (`voice/mod.rs`): `velocity_to_gain`,
  `semitones_to_ratio(midi_note - 60 + bend)`, `base_frequency`. **Reuse the formulas at the strike
  enqueue site.**
- **Note dispatch / routing branch point:** `plugins/lamath/src/runtime/event_handling.rs`
  (`note_on_with_latch_capture`, `start_voice_in_runtime`, `note_off`, `handle_control`) and
  `plugins/lamath/src/runtime.rs` (`process_with_runtime_input`). **The toggle branches here:
  shared-body-on idiophone note-ons enqueue strikes instead of starting voices.**
- **Patch / parameters / state:** `plugins/lamath/src/patch.rs`, `src/parameters/{paths,registry,helpers}.rs`,
  state round-trip. **Add the toggle + key-switch range as new fields/params (mirror
  `retrigger_resonators`).**
- **UI:** the Lamath Vizia editor in `lindelion-ui` (platform-gated; compile-checked on Linux, not
  rendered by `make ci`). **Add the toggle + damp-range controls.**

Built new: the `SharedBody` runtime module (persistent stack + strike queue + injector pool + damp +
body-scoped staging), and its objective audio tests.

## Cross-cutting constraints

- **Allocation-free audio path** ([ADR-0001](docs/adr/0001-allocation-free-audio-thread.md)): the body,
  strike queue, and injector pool are preallocated and bounded; every new audio-thread behavior carries
  a focused no-allocation test. *(Re-verified: the chamber and mesh runtime are already allocation-free
  with preallocated buffers — the same discipline applies.)*
- **Toggle-off regression guard:** with `shared_body = false`, render output stays bit-identical to
  pre-feature behavior. Every milestone re-asserts this with a render-equivalence test.
- **Engine stays a voice orchestrator** (mirror the chamber's containment): the shared body, strike
  queue, and injector pool live on `ResonatorProcessor`, never on `SynthEngine`. *(Re-verified: the
  chamber adds zero engine state; this feature holds the same line.)*
- **Strings/tubes unchanged:** the per-voice waveguide path is not touched; the toggle no-ops for
  non-idiophone slots. *(Re-verified against `resonator_stack.rs` family dispatch.)*
- **Test hygiene** ([AGENTS.md](AGENTS.md)): `make ci` units stay fast/pure/in-memory (no-alloc +
  behavior). Multi-second ring/stability/tuning sweeps go behind the per-crate `integration-tests`
  feature and run via `make test-integration`; keep a cheap in-memory unit alongside each heavy test.

## Milestones

### M0 — Patch + parameter surface (no behavior change)
Add the control surface with zero runtime effect yet.
- Add `shared_body: bool` (default `false`) and a configurable damp key-switch range to `patch.rs`,
  mirroring `retrigger_resonators`; wire through `parameters/{paths,registry,helpers}` and state
  round-trip.
- **[DECISION]** Default damp key-switch range (e.g. lowest octave A0–B0). Proposed default stated; user
  may adjust.
- Exit: `make ci` green; new fields round-trip through patch TOML and DAW state; render is bit-identical
  to pre-change with the toggle absent/off.

### M1 — `SharedBody` skeleton (persistent, allocation-free, silent) [depends on M0]
Stand up the runtime-owned body, owned and summed but not yet fed.
- New `SharedBody` module: one persistent resonator stack (mesh + modal idiophone families) mirroring
  the patch idiophone config, preallocated; owned by `ResonatorProcessor` like `SympatheticChamber`.
- Summed into the mix at runtime scope behind the toggle; defeated (no-op) when off.
- Exit: `make ci` green incl. a no-allocation test for the body's per-block path; body is constructed,
  owned by the runtime, and bit-identical-to-today when the toggle is off.

### M2 — Push strike path + injector pool [depends on M1]
Make note-ons strike the body.
- Branch in `event_handling`/`runtime`: with the toggle on and an idiophone slot, a note-on enqueues a
  strike `{ force, pitch, excitation selection, strike position }` instead of starting a voice; strikes
  bypass voice allocation/polyphony.
- Preallocated, allocation-free injector pool plays the selected excitation sample(s) into the live body
  at the strike position (reusing `excitation.rs` cursor/selection mechanics).
- Exit: `make ci` green incl. no-alloc; a strike produces a ring; multiple overlapping strikes
  superimpose without choking the existing ring; toggle-off path unaffected.

### M3 — Per-strike ring-preserving retune [depends on M2]
Make the body melodically playable per decision 3.
- Each strike retunes the body to the note via the state-preserving `retune` path (no buffer clear)
  before injecting; the prior ring keeps propagating in persisted state.
- Exit: `make ci` green; objective unit test that a second strike at a new pitch does **not** reset the
  first strike's decaying energy, and that the body's tuning tracks the most recent strike.

### M4 — Key-switch damp / choke [depends on M2]
Stop the ring only on the explicit gesture.
- Notes in the configured damp range ramp the body toward silence (a `silence`-style decay ramp); notes
  outside strike; note-off does nothing to the ring.
- Handle via the note dispatch branch (key-switch range check before the strike enqueue).
- Exit: `make ci` green; the damp key silences the body within the ramp; out-of-range notes strike;
  note-off leaves the ring intact.

### M5 — Body energy follower + gain staging + summing order [depends on M1]
Give the body its own dynamics and level discipline.
- Body-scoped `EnergyFollower` feeds mesh geometric drive / modal nonlinearity; body-scoped makeup /
  staging ([ADR-0029](docs/adr/0029-m11-gain-staging.md)) since per-voice `stage_peaks` taps don't cover
  it.
- Fix the runtime summing order: engine → shared body → sympathetic chamber → master, so the body's
  output feeds the chamber and master like any other voice energy.
- Exit: `make ci` green incl. a staging unit test; body energy demonstrably drives its nonlinearity;
  level sits within the family staging targets.

### M6 — Amp-envelope bypass + static post-body coloration [depends on M2]
Make the body's decay the envelope.
- In shared-body mode the per-note amp envelope steps aside (sustain = 1, no per-note release); output
  filter/saturation apply as a static post-body coloration stage; per-strike attack (mechanical-noise
  burst) still arms per strike.
- Exit: `make ci` green; shared-body output is not gated by a per-note amp release; static coloration
  and per-strike attack noise verified by unit tests.

### M7 — UI controls [depends on M0]
Expose the new surface in the editor.
- Add the `shared_body` toggle and damp-range controls to the Lamath Vizia editor (`lindelion-ui`);
  compile-checked on Linux, rendered on the macOS build.
- Exit: `make ci` green (editor compile-checks pass); controls bound to the new params.

### M8 — Tuning + objective audio fidelity/stability [depends on M3, M4, M5, M6]
Voice it and prove it is stable.
- Heavy sweeps behind the `integration-tests` feature: multi-second ring length, stability under dense
  overlapping strikes (no runaway / energy stays bounded), dynamics response (soft vs hard strike),
  retune-while-ringing continuity, and damp-ramp behavior. Cheap in-memory units mirror each.
- **[DECISION]** Final voicing by taste: body decay scaling, choke ramp time, injector pool size,
  per-strike attack feel, mesh-vs-modal balance. Objective bounds (stability, level, no-runaway) are
  automated; the taste values are the user's call.
- Exit: `make ci` green **and** `make test-integration` green for the new shared-body sweeps; stability
  and no-runaway bounds met across families.

### Docs at completion [depends on M8]
- Update `docs/plugins/lamath.md` (current-state spec) to describe shared-body mode as implemented.
- Add a CHANGELOG version heading for the accumulated change and bump the Lamath/workspace version to
  match (per the repo's no-"Unreleased" rule).
- Delete this root plan once the durable surfaces carry the record.

### Decisions needing your input
| Where | Decision you own |
| ----- | ---------------- |
| M0 | Default damp key-switch range (proposed: lowest octave A0–B0). |
| M8 | Final voicing by taste — body decay scaling, choke ramp time, injector pool size, per-strike attack feel, mesh-vs-modal balance (objective stability/level bounds are automated). |

## Handoff

This plan stops at milestone altitude. To execute, run `plan-phase` on one milestone (start with **M0**)
to expand it into ordered, file-level steps with red→green verification, then run the companion
execution prompt.
