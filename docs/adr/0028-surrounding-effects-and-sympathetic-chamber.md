# ADR-0028: Surrounding Effects and a Cross-Voice Sympathetic Chamber

## Status

Accepted

## Date

2026-05-31

## Context

Milestone M10 of the dynamic-response program ([ADR-0014](0014-dynamic-response-effort-energy-bus.md))
adds the last link of the chain — the *surrounding* effects: effort/energy-scaled mechanical noise,
radiation brightening, and sympathetic resonance. Two questions had no existing ADR home (the milestone's
`[DECISION]`): **how sympathetic resonance is routed**, and **where the surrounding effects attach**.

True sympathetic resonance is inherently *cross-voice*: one note's energy must ring resonant structures
that other notes also excite and that persist independent of any single voice. A per-voice bank cannot do
this. The resonant structure must also be **tuned to the notes actually played** (not a fixed drone) and
**driven by the playing energy** — and it must not turn the voice engine into a stateful DSP processor.

## Decision

- **Ship all three surrounding effects**, each a `0..1` depth on a new `SurroundingConfig`, defaulting to
  `0` (defeated) so a default patch is unchanged.
- **Mechanical noise and radiation brightening are per-voice, host-rate**, in a `SurroundingStage`
  between the resonator output and the output stage. Mechanical noise is an effort-scaled attack burst —
  a bright pick click (high-passed, fast decay) plus a band-limited breath rush (band-passed, slower
  decay) — from a **seeded, allocation-free xorshift PRNG** (deterministic, so it respects the unit-test
  hygiene rule). Radiation brightening is an **energy-scaled high-shelf** (`BiquadCoefficients::high_shelf`),
  bounded. Per the [ADR-0014] split, **effort** drives the noise and **energy** the brightening.
- **Sympathetic resonance is a cross-voice `SympatheticChamber` wired as a send/return at the runtime,
  not on the engine.** This is the corrected design (the first cut bolted a fixed-drone bank onto the
  engine; revised here). Specifics:
  - **A runtime-owned send/return.** The chamber lives on `ResonatorProcessor` (the orchestration layer
    that already owns the engine and the patch). Each block the runtime renders the voices (engine
    **untouched** — pure voice summation), then runs the chamber over the mixed buffer and sums its
    return back. The engine knows nothing about it; there is no engine state, post-mix pass, or
    config side-channel on the engine.
  - **Tuned to the played notes, not a fixed drone.** The chamber is a pool of Karplus-Strong damped
    delay loops. The runtime tunes them from the **currently-sounding voices' pitches**, read each block
    from the engine's voice slots (a pull model — robust to every note source: MIDI, audio-created, all
    become voices). A string keeps ringing after its note ends (that *is* the sympathy) and is reused
    when its pitch is replayed or stolen (least-active first) when the pool is full. Because each loop
    rings at its fundamental *and harmonics*, a string tuned to a held note blooms when a harmonically
    related note sounds — the voices ring **each other**.
  - **Energy-scaled send.** The chamber runs its own measured-energy follower (the shared
    `EnergyFollower`) on the mix and scales the send into the strings by a **squared, dynamics-
    concentrating** curve — the aggregate analogue of the per-voice M2 energy bus — so soft playing
    barely stirs the strings and hard playing blooms.
  - **Passive, bounded, defeatable, stereo.** Loop gain < 1 plus an in-loop low-pass; defeated (no-op)
    at depth 0; `silence()` clears the tail on patch change / reset; strings are spread across the
    stereo field for a wide sympathetic body.

## Consequences

- The engine diff for the sympathetic feature is **zero** — it stays a voice orchestrator. All
  sympathetic state and processing live in the chamber + a handful of runtime call sites.
- The sympathetic depth is read live from the patch each block (no note-on side-channel), so it applies
  immediately, even mid-sustain. `silence()` is wired to patch change; the chamber is reconstructed on
  full processor rebuild/reset, so there is no lingering-tail lifecycle gap.
- The pull model (tuning from sounding voices) means the resonant set always tracks what is actually
  playing, including release tails, with no hooks in the note-start paths.
- The shared `EnergyFollower` was promoted from `voice/` to `dsp/energy_follower.rs` now that it has a
  second consumer (the chamber), as its own doc anticipated.
- The string count, loop gain (sustain), send/return gains, and damping are first-principles voicings;
  their final values are part of the M11 empirical calibration pass.

## Alternatives

- **Global bank bolted onto the engine (the first cut).** Turned the stateless engine into a stateful
  DSP processor with a post-mix pass and a `note_on` config side-channel, used a fixed drone unrelated to
  the music, and left no way to silence the tail. Rejected and reverted in favour of the runtime
  send/return chamber.
- **Per-voice sympathetic bank.** Self-contained and engine-free, but fundamentally cannot ring one
  voice from another — it is not sympathetic resonance. Rejected.
- **Cross-resonator feed** (the backlog item: feed resonator A's output into B's input). A distinct,
  still-open inter-slot coupling idea; not the cross-note sympathy this milestone is about.
- **Scale-quantized fixed strings.** Simpler than a note-tracking pool, but reintroduces a fixed-drone
  flavour; the pool tracks exactly what you play (and extends to microtuning/bends later).
