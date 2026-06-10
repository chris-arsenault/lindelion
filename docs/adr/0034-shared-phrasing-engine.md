# 0034 — Shared note-lifecycle phrasing engine (String + Tube)

- Status: Accepted
- Date: 2026-06-10

## Context

After the bow contact ([ADR-0033](0033-lamath-bow-velocity-wave-contact.md)) and the humanize
variance controls, both sustained Lamath voices still froze their played inputs: `effort` was a
single number sampled at note-on, so a held note had no attack development, no sustain motion, no
vibrato, and a bare gate cliff at note-off. The same deficiency existed on the String bow and the
Tube wind voice, and the intended fix — a living note lifecycle on the driver's physical inputs —
is conceptually identical for both. Host expression (CC, channel pressure, pitch bend) was
delivered by the plugin shell but ignored by both processors.

## Decision

- **One shared engine, two thin per-instrument mappings.** `lindelion-dsp-utils::phrase` owns the
  note-lifecycle machine (`PhraseEngine`) and the host performance layer (`HostExpression`); each
  product maps the engine's instrument-agnostic outputs (`intensity`, `pitch_lean_cents`, plus the
  separated `sustain_swell`/`release_multiplier` components) onto its own physical targets and
  tunes its own nominal depths. Phrasing is the deliberate sibling of `variance`'s involuntary
  humanize walks; both ride the same per-sample offset rails, composed as
  `patch base + phrase contour + humanize walk`.
- **The engine is causal, so its contours are duration-agnostic.** It never knows when a note will
  end: the attack develops asymptotically toward a plateau, the sustain swell is a slow bounded
  walk (no arcs that must resolve at an unknown end), vibrato blooms in after an onset delay, and
  the **release is reactive** — a taper that begins at note-off and holds the note gate open while
  it sounds. Anticipatory endings are the player's knowledge: they arrive through the expression
  layer, or simply by releasing the note early and letting the taper carry the ending.
- **Legato is note overlap.** A note arriving while another sounds continues the phrase (kept
  development, kept vibrato bloom; on the String also the bow stroke; on the Tube a slur); a note
  from silence is a fresh attack. One rule answers tongued/slurred for the wind and
  détaché/legato for the bow.
- **Vibrato is a separate control from the rest of the phrasing.** Pitch motion masks every other
  lifecycle element in audition and in play, so `Phrasing` (attack/swell/release) and `Vibrato`
  are independent knobs, both under the knob law (0.5 = nominal, 1.0 approaches the unmusical;
  see the humanize precedent).
- **Per-instrument physics decides the mapping, not the engine.** The String maps `intensity`
  onto its effort scale (which co-moves bow speed and force, keeping phrased dynamics
  Schelleng-invariant) and vibrato onto the played-frequency rail its intonation servo follows.
  The Tube's reed is a **threshold oscillator with starting hysteresis**: a breath ramping up from
  below speaking pressure never locks, and a multiplicative swell trough kills a soft note for
  good. The wind therefore takes no level development at the attack (`development_start = 1.0`;
  its attack character remains the effort-coupled overpressure envelope), maps swell and vibrato
  onto the breath-**pressure rail** (where the humanize walks already modulate safely), and gates
  effort only by the release taper. Wind vibrato is breath vibrato, not bore pitch.
- **Host expression rides on top, multiplicatively.** The CC1/CC11 dynamics line multiplies the
  phrase intensity (the orchestral-library idiom: a drawn lane carries the macro phrase — including
  anticipatory endings — while the automatic layer supplies micro-musicality inside it); channel
  pressure swells additively above the line, so a controller resting at zero cannot mute; pitch
  bend adds cents to each instrument's pitch rail. With no events the layer is exactly inert.
- **Defaults follow each product's accepted voice.** The String ships `Phrasing`/`Vibrato` at 0.5
  (nominal). The Tube ships both at 0.0 — like its `humanize` — because its accepted default voice
  sits near the reed's oscillation margin, making breath modulation opt-in until that voice is
  re-margined.

## Alternatives considered

- **Per-instrument phrasing implementations.** Rejected: the lifecycle, contours, knob law, and
  expression layering are instrument-independent; only the physical mapping differs. A shared
  engine keeps the two products' phrasing semantics identical by construction.
- **Lookahead/duration-aware contours** (arcs that resolve at note end). Impossible live — the
  engine is causal — and a render-only lookahead is forbidden (no render-only behavior). The
  reactive-release + expression-layer split covers the same musical ground.
- **One knob covering vibrato and lifecycle together** (strict humanize parity). Auditioned and
  rejected: vibrato masked everything else; the two are independently played dimensions.
- **CC as an authority switch** (expression replaces generated phrasing when active). Rejected for
  a stateless multiplicative composition — predictable, idempotent, and inert without events.
- **Mapping the Tube's swell onto effort like the String.** Rejected on measurement: soft notes
  near the reed threshold died permanently in the register sweep; the pressure rail carries the
  same musical motion without crossing the oscillation threshold.

## Consequences

- Bare MIDI notes phrase musically on both instruments with zero automation; a drawn CC lane takes
  over the macro dynamics without losing the micro-musicality. Live validation of the expression
  path in a host (Galad) remains the outstanding real-world check.
- At knob zeros both engines bypass exactly (bit-identical static behavior, guarded by tests).
- Walks are instance-seeded: renders are reproducible for a fixed construction order, but tests
  that assert tight margins must pin the variance/phrasing knobs to zero (execution order shifts
  the seed draw) — regime and equality tests do; variance-active guards use seed-robust bounds.
- The String's stroke-weight coupling (force rides exactly with stroke speed) keeps phrased and
  rearticulated attacks inside the Schelleng cone; any fixed weight floor at zero speed renders as
  an onset scratch.
