# ADR-0031: Shared-Body Idiophone Mode (re-strikable persistent resonator)

## Status

Accepted

## Date

2026-06-01

## Context

Lamath is voice-per-note: each `Voice` owns its own resonator stack, so N held notes are N separate
bodies, and the per-voice amp envelope reshapes each one's output. `retrigger_resonators = false`
already preserves a *reused* voice's ring on a same-note re-hit (via the state-preserving
`try_configure_preserving_state`/`retune` path), but it is not a shared body: different pitches and
held re-hits spawn separate resonators, and the amp envelope still gates the output.

Real idiophones — a cymbal, gong, or bell — are not polyphonic voices. They are a **single physical
body** that every strike excites in place. Striking again while it rings does not start a second
cymbal; it injects fresh energy into the live one. The ring is not stopped by lifting the stick
(note-off) but by an explicit damp (a hand, a hi-hat pedal). No sample library models this — the
round-robin layer is the giveaway — but it is feasible here because the 2D mesh (and the parallel
modal bank) are real, linear-superposable physical models, not samples.

The M10 sympathetic chamber ([ADR-0028](0028-surrounding-effects-and-sympathetic-chamber.md)) is the
existing precedent for a **persistent, runtime-owned, cross-voice resonant layer** wired as a
send/return on `ResonatorProcessor`. The idiophone body wants the same *ownership* and *persistence*,
but a different *feed*: the chamber is tuned by a per-block **pull** of sounding-voice pitches (it
needs no force or exact timing); a struck body needs discrete **strike events** carrying velocity
(force), pitch, excitation selection, and sample-accurate timing.

The shape-changing choices (settled with the user) were: how the mode is entered; which resonator
families it covers; what a strike's pitch does to the body; and how the ring is stopped.

## Decision

Add an opt-in **shared-body idiophone mode**: a runtime-owned, persistent resonator stack that
note-ons *re-strike* instead of allocating a per-note voice. Specifics:

- **Opt-in per-patch toggle, not automatic.** A new `shared_body: bool` on the patch, default
  `false`. Off is **bit-identical to today's** per-voice idiophone behavior. On promotes the
  instrument's idiophone resonator to the shared body. The existing polyphonic pitched mesh/modal
  stays fully available; the two behaviors coexist. (Rejected: making Mesh *always* shared, which
  would silently change existing patches and delete polyphonic mesh.)
- **Mesh and modal idiophone families, mirroring the patch stack.** The shared body is one persistent
  instance of the patch's resonator stack (A/B slots + routing) restricted to idiophone families
  (Mesh, Modal). Waveguide (String/Tube) slots are **unaffected by the toggle and stay
  polyphonic-per-voice**, so the backlog's "strings/tubes stay per-voice" holds literally; the toggle
  no-ops for non-idiophone slots.
- **Strike events via a push path, not the chamber's pull.** Note-on enqueues a strike
  `{ force = velocity→gain, pitch, excitation selection, strike position }` to the runtime-owned body.
  A small, preallocated, allocation-free **injector pool** plays the selected excitation sample(s)
  into the live body at the strike position, preserving Lamath's samples-as-excitation identity.
  Strikes **do not consume voice slots**; polyphony does not apply in this mode.
- **Pitch retunes the live body per strike, ring-preserving.** Each strike retunes the body to the
  note via the existing state-preserving `retune` path (no buffer clear), then injects — so the body
  is melodically playable: last strike wins the tuning while the prior ring keeps propagating in the
  persisted state until it decays or is overwritten. (Rejected for the default: a fixed-tuning body
  where pitch only moves strike position; available later as a voicing if wanted.)
- **The body's decay is the envelope.** The per-note amp envelope gets out of the way in this mode
  (sustain = 1, no per-note release); the body's own physical decay shapes the tail. Output coloration
  (filter/saturation) and per-strike attack noise still apply, the former as a static post-body stage,
  the latter armed per strike.
- **Explicit damp via a key-switch range; note-off does nothing.** A configurable MIDI-note range
  damps the body (ramps it toward silence); notes outside the range strike. Note-off otherwise has no
  effect on the ring.
- **Its own energy follower and gain staging, summed at runtime scope.** Like the chamber, the body
  runs its own measured-energy follower (feeding mesh geometric drive / modal nonlinearity) and its own
  makeup/staging ([ADR-0029](0029-m11-gain-staging.md)); the per-voice `stage_peaks` taps do not cover
  a runtime-owned body. Defeated (no-op) when the toggle is off; `silence()` clears the tail on patch
  change / reset.

## Consequences

- The voice engine stays a voice orchestrator; the shared body and the strike queue live on
  `ResonatorProcessor` with a handful of runtime call sites, mirroring the chamber's containment.
- A new persistent, runtime-owned resonator stack is allocated up front and is allocation-free per
  block and per strike (injector pool and strike queue are preallocated and bounded), preserving
  [ADR-0001](0001-allocation-free-audio-thread.md).
- The `shared_body` toggle is structural-ish only insofar as it changes how note-ons are routed; it is
  read per block and applies live, with `silence()` wired to patch change so there is no
  lingering-tail lifecycle gap.
- Idiophone strikes no longer pass through the per-voice output envelope; voicing that previously
  leaned on the amp envelope for idiophones must instead come from the body decay and the static
  post-body coloration.
- Register-aware per-note Mesh/Modal tuning (the deferred M11 P7 item) is partly *replaced* for the
  shared body: strike position = timbre, with the per-strike retune supplying melodic pitch; do not
  over-invest in per-note shared-body tuning beyond what retune + position give.
- The send/return summing order at runtime (engine → shared body → sympathetic chamber → master) must
  be fixed so the body's output feeds the chamber and master like any other voice energy.

## Alternatives

- **Automatic promotion by resonator model** (Mesh/Modal always shared). Smaller surface, no new
  parameter, but silently changes existing mesh patches and removes polyphonic pitched mesh. Rejected
  in favor of an opt-in toggle that keeps both behaviors.
- **Instrument-wide `StruckBody` source mode** alongside Off/AudioCreatesNotes. Broadest reach, but
  reinterprets *all* note-ons as strikes regardless of resonator type, conflicting with the
  strings-stay-per-voice requirement. Rejected.
- **Fixed-tuning body (pitch moves strike position only).** The most physically honest cymbal/gong
  model and the backlog's first sketch, but not melodically playable. Rejected as the default; retained
  as a possible voicing axis.
- **Pull model like the sympathetic chamber** (poll sounding pitches per block). Loses per-strike
  force and sample-accurate timing — fatal for a struck body where dynamics and attack timing are the
  point. Rejected in favor of a push strike queue.
- **Reuse a never-released per-voice slot as the body.** Re-uses the voice path but drags the amp
  envelope, voice stealing, and polyphony accounting into a thing that is conceptually one persistent
  object. Rejected in favor of a dedicated runtime-owned body.
